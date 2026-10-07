use crate::team::{CodeEvidence, CodeWorkspace, Project};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
const MAX: usize = 8 * 1024 * 1024;
fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn command(repo: &Path) -> Command {
    let mut c = Command::new(if cfg!(target_os = "macos") {
        "/usr/bin/git"
    } else {
        "git"
    });
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            c.env_remove(key);
        }
    }
    c.env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_PROTOCOL_FROM_USER", "0");
    c.args([
        "--no-pager",
        "--no-optional-locks",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-c",
        "core.attributesFile=/dev/null",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "tag.gpgsign=false",
        "-c",
        "diff.external=",
        "-C",
    ])
    .arg(repo);
    c
}
fn bounded_output(
    mut c: Command,
    input: Option<&[u8]>,
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    c.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    let mut child = c.spawn().map_err(|_| "无法启动 Git")?;
    let stdin = child.stdin.take();
    let mut stdout = child.stdout.take().ok_or("Git 输出不可用")?;
    let input = input.map(Vec::from);
    let (send, recv) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let result = (|| {
            if let (Some(bytes), Some(mut pipe)) = (input, stdin) {
                pipe.write_all(&bytes).map_err(|_| "Git 输入失败")?;
            }
            let mut bytes = vec![];
            std::io::Read::by_ref(&mut stdout)
                .take((MAX + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| "Git 输出读取失败")?;
            if bytes.len() > MAX {
                return Err("Git 输出超过 8 MiB");
            };
            Ok(bytes)
        })();
        let _ = send.send(result);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let result = recv
        .recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|_| "Git 操作超时")
        .and_then(|r| r);
    let mut status = None;
    if result.is_ok() {
        while std::time::Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(s)) => {
                    status = Some(s);
                    break;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => break,
            }
        }
    }
    if status.is_none() {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = reader.join();
    let bytes = result.map_err(str::to_string)?;
    Ok((status.ok_or("Git 退出未在时限内确认")?, bytes))
}
fn extensions(repo: &Path) -> Result<(), String> {
    let mut c = command(repo);
    c.args([
        "config",
        "--get-regexp",
        r"^(extensions\.partialclone|remote\..*\.promisor|filter\..*\.(clean|smudge|process)|merge\..*\.driver)$",
    ]);
    let (status, _) = bounded_output(c, None)?;
    if status.code() == Some(1) {
        Ok(())
    } else if status.success() {
        Err(
            "仓库含 partial clone/promisor 或外部 filter/merge driver，当前受控 Git 模式不支持"
                .into(),
        )
    } else {
        Err("无法核对仓库 Git 扩展配置".into())
    }
}
fn run(
    repo: &Path,
    args: &[&str],
    index: Option<&Path>,
    input: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    extensions(repo)?;
    let mut c = command(repo);
    c.args(args);
    if let Some(i) = index {
        c.env("GIT_INDEX_FILE", i);
    }
    c.env("GIT_AUTHOR_NAME", "Orbit")
        .env("GIT_AUTHOR_EMAIL", "orbit@localhost")
        .env("GIT_COMMITTER_NAME", "Orbit")
        .env("GIT_COMMITTER_EMAIL", "orbit@localhost");
    let (status, bytes) = bounded_output(c, input)?;
    if !status.success() {
        return Err(format!(
            "Git {} 未成功，原工作目录与引用保留",
            args.first().unwrap_or(&"操作")
        ));
    }
    Ok(bytes)
}
fn text(repo: &Path, args: &[&str]) -> Result<String, String> {
    String::from_utf8(run(repo, args, None, None)?)
        .map(|s| s.trim().into())
        .map_err(|_| "Git 返回非 UTF-8 数据".into())
}
fn verify(p: &Project) -> Result<PathBuf, String> {
    let repo = PathBuf::from(&p.repo);
    if repo.canonicalize().map_err(|_| "项目已不可用")? != repo
        || !oid(&p.base)
        || !p.target.starts_with("refs/heads/")
    {
        return Err("Git 项目身份无效".into());
    }
    baseline_budget(&repo, &p.base)?;
    let common = text(&repo, &["rev-parse", "--git-common-dir"])?;
    let common = if Path::new(&common).is_absolute() {
        PathBuf::from(common)
    } else {
        repo.join(common)
    }
    .canonicalize()
    .map_err(|_| "Git 目录不可用")?;
    if common.to_str() != Some(&p.common_dir) {
        return Err("Git 仓库归属已变化".into());
    }
    Ok(repo)
}
pub fn preflight(path: &Path, target: &str) -> Result<Project, String> {
    let path = path.canonicalize().map_err(|_| "项目目录不存在")?;
    let repo = PathBuf::from(text(&path, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .map_err(|_| "Git 项目不可用")?;
    if !target.starts_with("refs/heads/") || target.len() > 250 {
        return Err("请选择完整的本地目标分支".into());
    }
    text(&repo, &["check-ref-format", target])?;
    if !run(
        &repo,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
        None,
        None,
    )?
    .is_empty()
    {
        return Err("项目有未提交改动，请先保存；工作台不会复制或覆盖它们".into());
    }
    let base = text(
        &repo,
        &["rev-parse", "--verify", &format!("{target}^{{commit}}")],
    )?;
    let tree = tree(&repo, &base)?;
    if tree.len() > 2000
        || tree.keys().any(|k| k == ".gitmodules")
        || tree.values().any(|(m, _)| m != "100644" && m != "100755")
    {
        return Err("项目超过 2000 文件或包含子模块／符号链接，暂不支持写任务".into());
    }
    let common = text(&repo, &["rev-parse", "--git-common-dir"])?;
    let common = if Path::new(&common).is_absolute() {
        PathBuf::from(common)
    } else {
        repo.join(common)
    }
    .canonicalize()
    .map_err(|_| "无法解析 Git 目录")?;
    baseline_budget(&repo, &base)?;
    Ok(Project {
        repo: repo.to_string_lossy().into(),
        common_dir: common.to_string_lossy().into(),
        base,
        target: target.into(),
    })
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() < 4096
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && s != ".git")
        && !path.chars().any(char::is_control)
}
fn tree(repo: &Path, commit: &str) -> Result<BTreeMap<String, (String, String)>, String> {
    if !oid(commit) {
        return Err("Git 对象身份无效".into());
    }
    let bytes = run(repo, &["ls-tree", "-r", "-z", commit], None, None)?;
    let mut result = BTreeMap::new();
    for row in bytes.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let row = std::str::from_utf8(row).map_err(|_| "文件名不是 UTF-8，无法安全预览")?;
        let (info, path) = row.split_once('\t').ok_or("Git tree 格式无效")?;
        let fields: Vec<_> = info.split(' ').collect();
        if fields.len() != 3 || fields[1] != "blob" || !oid(fields[2]) || !safe_path(path) {
            return Err("不支持的 Git 文件对象或路径".into());
        }
        result.insert(path.into(), (fields[0].into(), fields[2].into()));
    }
    Ok(result)
}
fn owned(root: &Path, id: &str, w: &CodeWorkspace) -> Result<PathBuf, String> {
    let canonical = root.canonicalize().map_err(|_| "管理目录不存在")?;
    let root = canonical.as_path();
    if !crate::team::token(id) {
        return Err("工作区身份无效".into());
    }
    let expected = root.join("worktrees").join(id);
    if Path::new(&w.directory) != expected
        || expected.canonicalize().map_err(|_| "工作区不可用")? != expected
    {
        return Err("工作区路径与所有者不一致".into());
    }
    let saved: CodeWorkspace = serde_json::from_slice(
        &fs::read(root.join("ownership").join(format!("{id}.json")))
            .map_err(|_| "工作区归属记录不存在")?,
    )
    .map_err(|_| "工作区归属记录损坏")?;
    if saved.project != w.project || saved.directory != w.directory {
        return Err("工作区归属已变化".into());
    }
    verify(&w.project)?;
    let actual = text(&expected, &["rev-parse", "--git-common-dir"])?;
    let common = if Path::new(&actual).is_absolute() {
        PathBuf::from(actual)
    } else {
        expected.join(actual)
    }
    .canonicalize()
    .map_err(|_| "工作区 Git 归属不可用")?;
    if common.to_str() != Some(&w.project.common_dir) {
        return Err("工作区已指向其他 Git 仓库".into());
    }
    Ok(expected)
}
pub fn prepare(root: &Path, id: &str, p: &Project) -> Result<CodeWorkspace, String> {
    let canonical = root.canonicalize().map_err(|_| "管理目录不存在")?;
    let root = canonical.as_path();
    if !crate::team::token(id) {
        return Err("任务身份无效".into());
    }
    let repo = verify(p)?;
    safe_directory(&root.join("ownership"))?;
    safe_directory(&root.join("worktrees"))?;
    fs::create_dir_all(root.join("ownership")).map_err(|_| "无法创建归属目录")?;
    fs::create_dir_all(root.join("worktrees")).map_err(|_| "无法创建工作区目录")?;
    let directory = root.join("worktrees").join(id);
    let w = CodeWorkspace {
        project: p.clone(),
        directory: directory.to_string_lossy().into(),
        snapshot: None,
        snapshots: vec![],
        artifact_id: None,
    };
    let marker = root.join("ownership").join(format!("{id}.json"));
    if directory.exists() {
        owned(root, id, &w)?;
        return Ok(w);
    }
    // Marker is outside the writer's sandbox. A failed checkout is preserved for explicit recovery.
    if marker.exists() {
        return Err("先前工作区创建结果需核对，归属记录已保留".into());
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .map_err(|_| "无法保存工作区归属")?;
    f.write_all(&serde_json::to_vec(&w).unwrap())
        .and_then(|_| f.sync_all())
        .map_err(|_| "工作区归属保存失败")?;
    run(
        &repo,
        &[
            "worktree",
            "add",
            "--detach",
            directory.to_str().ok_or("路径编码无效")?,
            &p.base,
        ],
        None,
        None,
    )?;
    owned(root, id, &w)?;
    Ok(w)
}
#[cfg(unix)]
fn read_file(root: &File, path: &str) -> Result<Option<(String, Vec<u8>)>, String> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::MetadataExt;
    let mut current = root.try_clone().map_err(|_| "目录不可读")?;
    let parts: Vec<_> = path.split('/').collect();
    for (n, part) in parts.iter().enumerate() {
        let name = std::ffi::CString::new(*part).map_err(|_| "文件名无效")?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if n + 1 == parts.len() {
                0
            } else {
                libc::O_DIRECTORY
            };
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            let e = std::io::Error::last_os_error();
            return if e.kind() == std::io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err("文件含符号链接或无法安全读取".into())
            };
        }
        current = unsafe { File::from_raw_fd(fd) };
    }
    let before = current.metadata().map_err(|_| "无法读取文件属性")?;
    if !before.is_file() || before.nlink() != 1 || before.len() > 2 * 1024 * 1024 {
        return Err("只捕获最多 2 MiB 的普通单链接文件；不支持设备、FIFO 或符号链接".into());
    }
    let mut content = vec![];
    std::io::Read::by_ref(&mut current)
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut content)
        .map_err(|_| "文件读取失败")?;
    let after = current.metadata().map_err(|_| "文件状态不可用")?;
    if before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || content.len() as u64 != after.len()
    {
        return Err("文件在捕获期间变化，请重新保存快照".into());
    }
    Ok(Some((
        if before.mode() & 0o111 != 0 {
            "100755"
        } else {
            "100644"
        }
        .into(),
        content,
    )))
}
#[cfg(not(unix))]
fn read_file(_root: &File, _path: &str) -> Result<Option<(String, Vec<u8>)>, String> {
    Err("此平台暂未开放可写工作区捕获".into())
}
fn index(root: &Path) -> Result<PathBuf, String> {
    safe_directory(&root.join("indexes"))?;
    fs::create_dir_all(root.join("indexes")).map_err(|_| "无法创建索引目录")?;
    Ok(root.join("indexes").join(uuid::Uuid::new_v4().to_string()))
}
fn write_commit(
    repo: &Path,
    root: &Path,
    base: &str,
    entries: &BTreeMap<String, (String, String)>,
) -> Result<(String, String), String> {
    let i = index(root)?;
    let result = (|| {
        run(repo, &["read-tree", "--empty"], Some(&i), None)?;
        for (path, (mode, object)) in entries {
            run(
                repo,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("{mode},{object},{path}"),
                ],
                Some(&i),
                None,
            )?;
        }
        let tree = String::from_utf8(run(repo, &["write-tree"], Some(&i), None)?)
            .map_err(|_| "tree 编码无效")?
            .trim()
            .to_string();
        let commit = text(
            repo,
            &[
                "commit-tree",
                &tree,
                "-p",
                base,
                "-m",
                "Orbit immutable delivery snapshot",
            ],
        )?;
        Ok((tree, commit))
    })();
    let _ = fs::remove_file(i);
    result
}
fn evidence(repo: &Path, base: &str, tree: &str, commit: &str) -> Result<CodeEvidence, String> {
    let bytes = run(
        repo,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            base,
            commit,
            "--",
        ],
        None,
        None,
    )?;
    let complete = bytes.len() <= 100000
        && !bytes.windows(12).any(|w| w == b"Binary files")
        && std::str::from_utf8(&bytes).is_ok();
    let diff = if bytes.len() <= 100000 {
        String::from_utf8_lossy(&bytes).into()
    } else {
        format!(
            "Diff 超过预览上限（{} bytes），完整评审与合并已禁用。",
            bytes.len()
        )
    };
    Ok(CodeEvidence {
        commit: commit.into(),
        tree: tree.into(),
        base: base.into(),
        diff,
        complete,
    })
}
pub fn snapshot(root: &Path, id: &str, w: &CodeWorkspace) -> Result<CodeEvidence, String> {
    let directory = owned(root, id, w)?;
    let repo = verify(&w.project)?;
    #[cfg(unix)]
    let directory_fd = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&directory)
            .map_err(|_| "工作区不可安全读取")?
    };
    #[cfg(not(unix))]
    let directory_fd = File::open(&directory).map_err(|_| "工作区不可读")?;
    let names = run(
        &directory,
        &["ls-files", "-c", "-o", "--exclude-standard", "-z"],
        None,
        None,
    )?;
    let listed = names
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| {
            std::str::from_utf8(s)
                .map(str::to_string)
                .map_err(|_| "文件名不是 UTF-8")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let observed = check_directory(&directory_fd, &directory, &listed)?;
    let mut entries = BTreeMap::new();
    let mut total = 0;
    for name in names.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let name = std::str::from_utf8(name).map_err(|_| "文件名不是 UTF-8")?;
        if !safe_path(name) {
            return Err("工作区文件路径无效".into());
        }
        if entries.contains_key(name) {
            continue;
        }
        let content = read_file(&directory_fd, name)?;
        if content.is_none() && observed.contains(name) {
            return Err("文件在检查后消失，请重新保存快照".into());
        }
        if let Some((mode, bytes)) = content {
            total += bytes.len();
            if total > MAX || entries.len() >= 2000 {
                return Err("快照超过 8 MiB/2000 文件，请缩小范围".into());
            }
            let hash = String::from_utf8(run(
                &repo,
                &["hash-object", "--no-filters", "-w", "--stdin"],
                None,
                Some(&bytes),
            )?)
            .map_err(|_| "blob 编码无效")?
            .trim()
            .into();
            entries.insert(name.into(), (mode, hash));
        }
    }
    owned(root, id, w)?;
    let (tree, commit) = write_commit(&repo, root, &w.project.base, &entries)?;
    let e = evidence(&repo, &w.project.base, &tree, &commit)?;
    retain(root, &w.project, &e)?;
    Ok(e)
}
pub fn integrate(
    root: &Path,
    p: &Project,
    snapshots: &[CodeEvidence],
) -> Result<CodeEvidence, String> {
    let repo = verify(p)?;
    if snapshots.is_empty()
        || snapshots.len() > 3
        || snapshots
            .iter()
            .any(|s| s.base != p.base || !oid(&s.commit) || !oid(&s.tree))
    {
        return Err("集成快照来源无效".into());
    }
    let base = tree(&repo, &p.base)?;
    let mut merged = base.clone();
    let mut changes = vec![];
    for s in snapshots {
        let snapshot = tree(&repo, &s.commit)?;
        if text(&repo, &["rev-parse", &format!("{}^{{tree}}", s.commit)])? != s.tree {
            return Err("快照 tree 与 commit 不一致".into());
        }
        let keys: std::collections::BTreeSet<_> =
            base.keys().chain(snapshot.keys()).cloned().collect();
        for path in keys {
            if base.get(&path) == snapshot.get(&path) {
                continue;
            }
            if changes.iter().any(|old: &String| {
                old == &path
                    || path.starts_with(&format!("{old}/"))
                    || old.starts_with(&format!("{path}/"))
            }) {
                return Err(format!(
                    "子任务在 {path} 有重叠变更，请人工处理后重新保存快照"
                ));
            }
            changes.push(path.clone());
            if let Some(entry) = snapshot.get(&path) {
                merged.insert(path, entry.clone());
            } else {
                merged.remove(&path);
            }
        }
    }
    let (tree, commit) = write_commit(&repo, root, &p.base, &merged)?;
    let e = evidence(&repo, &p.base, &tree, &commit)?;
    retain(root, p, &e)?;
    Ok(e)
}
pub fn merge_ref(p: &Project, commit: &str) -> Result<(), String> {
    let repo = verify(p)?;
    if !oid(commit) {
        return Err("集成提交无效".into());
    }
    let refs = text(&repo, &["worktree", "list", "--porcelain"])?;
    if refs
        .lines()
        .any(|line| line == format!("branch {}", p.target))
    {
        return Err(
            "目标分支正在某个工作目录中检出，请先切离该分支再确认合并；工作台不会更新用户文件"
                .into(),
        );
    }
    if text(&repo, &["rev-parse", "--verify", &p.target])? != p.base {
        return Err("目标分支已变化，请重新建立计划与基线".into());
    }
    text(&repo, &["merge-base", "--is-ancestor", &p.base, commit])?;
    run(
        &repo,
        &["update-ref", &p.target, commit, &p.base],
        None,
        None,
    )?;
    Ok(())
}
pub fn ref_oid(p: &Project) -> Result<String, String> {
    let repo = verify(p)?;
    text(&repo, &["rev-parse", "--verify", &p.target])
}

pub fn default_target(path: &Path) -> Result<String, String> {
    text(path, &["symbolic-ref", "HEAD"])
}
pub fn workspace_path(root: &Path, id: &str, w: &CodeWorkspace) -> Result<PathBuf, String> {
    owned(root, id, w)
}

fn safe_directory(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir()) {
        return Err("管理子目录不可替换或使用符号链接".into());
    }
    Ok(())
}
fn snapshot_ref(e: &CodeEvidence) -> String {
    format!("refs/orbit/snapshots/{}", e.commit)
}
fn retain(root: &Path, p: &Project, e: &CodeEvidence) -> Result<(), String> {
    let repo = verify(p)?;
    let reference = snapshot_ref(e);
    let journal = root.join("operations");
    safe_directory(&journal)?;
    fs::create_dir_all(&journal).map_err(|_| "不能保存 Git 操作记录")?;
    let record = journal.join(format!("{}.json", e.commit));
    let bytes=serde_json::to_vec(&serde_json::json!({"project":p,"kind":"retain","old":null,"new":e.commit,"target":reference})).map_err(|_|"操作记录编码失败")?;
    if record.exists() {
        if fs::read(&record).map_err(|_| "操作记录不可读")? != bytes {
            return Err("Git 操作记录不一致".into());
        }
    } else {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&record)
            .map_err(|_| "无法创建操作记录")?;
        f.write_all(&bytes)
            .and_then(|_| f.sync_all())
            .map_err(|_| "操作记录保存失败")?;
    }
    if let Ok(current) = text(&repo, &["rev-parse", "--verify", &reference]) {
        if current == e.commit {
            return Ok(());
        }
        return Err("快照引用已被更改".into());
    }
    run(
        &repo,
        &[
            "update-ref",
            &reference,
            &e.commit,
            &"0".repeat(e.commit.len()),
        ],
        None,
        None,
    )?;
    Ok(())
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleExport {
    pub project: Project,
    pub commits: Vec<String>,
    pub encoding: &'static str,
    pub bundle: String,
}
pub fn export_bundles(tasks: &[crate::model::Task]) -> Result<Vec<BundleExport>, String> {
    use base64::Engine;
    let mut projects: BTreeMap<String, (Project, std::collections::BTreeSet<String>)> =
        BTreeMap::new();
    for t in tasks {
        if let Some(w) = &t.code_workspace {
            for e in w.snapshots.iter().chain(w.snapshot.iter()) {
                projects
                    .entry(w.project.common_dir.clone())
                    .or_insert_with(|| (w.project.clone(), Default::default()))
                    .1
                    .insert(e.commit.clone());
            }
        }
        if let Some(i) = &t.team_input {
            if let (Some(e), Ok(plan)) =
                (&i.code, serde_json::from_str::<crate::team::Plan>(&i.plan))
            {
                if let Some(p) = plan.project {
                    projects
                        .entry(p.common_dir.clone())
                        .or_insert_with(|| (p, Default::default()))
                        .1
                        .insert(e.commit.clone());
                }
            }
        }
        if let Some(w) = &t.team {
            if let (Some(p), Some(e)) = (&w.plan.project, &w.integration) {
                projects
                    .entry(p.common_dir.clone())
                    .or_insert_with(|| (p.clone(), Default::default()))
                    .1
                    .insert(e.commit.clone());
            }
        }
    }
    let mut result = vec![];
    for (_, (p, commits)) in projects {
        let repo = verify(&p)?;
        let names: Vec<_> = commits
            .iter()
            .map(|c| format!("refs/orbit/snapshots/{c}"))
            .collect();
        for (c, r) in commits.iter().zip(&names) {
            if text(&repo, &["rev-parse", "--verify", r])? != *c {
                return Err("快照对象引用缺失，不能导出完整代码数据".into());
            }
        }
        let mut args = vec!["bundle", "create", "-"];
        args.extend(names.iter().map(String::as_str));
        let bytes = run(&repo, &args, None, None)?;
        result.push(BundleExport {
            project: p,
            commits: commits.into_iter().collect(),
            encoding: "base64",
            bundle: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    Ok(result)
}

fn baseline_budget(repo: &Path, base: &str) -> Result<(), String> {
    let sizes = run(repo, &["ls-tree", "-r", "-l", "-z", base], None, None)?;
    let mut total = 0;
    for row in sizes.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let info = std::str::from_utf8(row)
            .map_err(|_| "文件信息无效")?
            .split('\t')
            .next()
            .ok_or("文件信息无效")?;
        let size = info
            .split_whitespace()
            .last()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or("非普通文件")?;
        if size > 2 * 1024 * 1024 {
            return Err("基线单个文件超过 2 MiB".into());
        }
        total += size;
        if total > MAX {
            return Err("基线文件总量超过 8 MiB".into());
        }
    }
    Ok(())
}
#[cfg(unix)]
fn check_directory(
    root: &File,
    repo: &Path,
    listed: &[String],
) -> Result<std::collections::HashSet<String>, String> {
    use std::os::fd::{AsRawFd, FromRawFd};
    struct Dir(*mut libc::DIR);
    impl Drop for Dir {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    fn ignored(repo: &Path, path: &str) -> Result<bool, String> {
        extensions(repo)?;
        let mut c = command(repo);
        c.args(["check-ignore", "-q", "--", path]);
        let (s, _) = bounded_output(c, None)?;
        match s.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err("无法核对忽略目录".into()),
        }
    }
    fn walk(
        fd: &File,
        repo: &Path,
        prefix: &str,
        listed: &[String],
        count: &mut usize,
        files: &mut std::collections::HashSet<String>,
        depth: usize,
        deadline: std::time::Instant,
    ) -> Result<(), String> {
        if depth > 32 || std::time::Instant::now() > deadline {
            return Err("工作区目录检查超限".into());
        }
        let duplicate = unsafe {
            libc::openat(
                fd.as_raw_fd(),
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if duplicate < 0 {
            return Err("无法安全打开目录".into());
        }
        let pointer = unsafe { libc::fdopendir(duplicate) };
        if pointer.is_null() {
            unsafe {
                libc::close(duplicate);
            }
            return Err("目录不可枚举".into());
        }
        let directory = Dir(pointer);
        loop {
            #[cfg(target_os = "macos")]
            unsafe {
                *libc::__error() = 0;
            }
            #[cfg(target_os = "linux")]
            unsafe {
                *libc::__errno_location() = 0;
            }
            let entry = unsafe { libc::readdir(directory.0) };
            if entry.is_null() {
                if std::io::Error::last_os_error().raw_os_error().unwrap_or(0) != 0 {
                    return Err("目录读取失败".into());
                }
                break;
            }
            let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
            let name = name.to_str().map_err(|_| "目录文件名不是 UTF-8")?;
            if matches!(name, "." | "..") || prefix.is_empty() && name == ".git" {
                continue;
            }
            *count += 1;
            if *count > 12000 || std::time::Instant::now() > deadline {
                return Err("工作区检查超过 12000 条目或时间限额".into());
            }
            let path = if prefix.is_empty() {
                name.into()
            } else {
                format!("{prefix}/{name}")
            };
            if !safe_path(&path) {
                return Err("工作区路径无效".into());
            }
            let c = std::ffi::CString::new(name).map_err(|_| "文件名无效")?;
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    fd.as_raw_fd(),
                    c.as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return Err("文件在捕获期间消失或不可读取".into());
            }
            let stat = unsafe { stat.assume_init() };
            let kind = stat.st_mode & libc::S_IFMT;
            if kind == libc::S_IFDIR {
                if !listed.iter().any(|p| p.starts_with(&format!("{path}/")))
                    && ignored(repo, &path)?
                {
                    continue;
                }
                let child = unsafe {
                    libc::openat(
                        fd.as_raw_fd(),
                        c.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if child < 0 {
                    return Err("目录已变化或含符号链接".into());
                }
                let child = unsafe { File::from_raw_fd(child) };
                walk(
                    &child,
                    repo,
                    &path,
                    listed,
                    count,
                    files,
                    depth + 1,
                    deadline,
                )?;
            } else if kind == libc::S_IFREG {
                files.insert(path);
            } else {
                return Err("工作区含符号链接、FIFO、socket 或设备，拒绝捕获".into());
            }
        }
        Ok(())
    }
    let mut files = std::collections::HashSet::new();
    walk(
        root,
        repo,
        "",
        listed,
        &mut 0,
        &mut files,
        0,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )?;
    Ok(files)
}
#[cfg(not(unix))]
fn check_directory(
    _: &File,
    _: &Path,
    _: &[String],
) -> Result<std::collections::HashSet<String>, String> {
    Err("此平台未开放安全写工作区捕获".into())
}
