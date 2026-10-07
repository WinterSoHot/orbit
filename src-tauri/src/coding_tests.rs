use crate::coding;
use std::{fs, path::Path, process::Command};
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("/usr/bin/git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.fsmonitor=false",
        ])
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git fixture failed");
    String::from_utf8(out.stdout).unwrap().trim().into()
}
pub(crate) fn repo() -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("orbit-git-delivery-{}", uuid::Uuid::new_v4()));
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main", "--template="]);
    git(&repo, &["config", "user.name", "Fixture"]);
    git(&repo, &["config", "user.email", "fixture@localhost"]);
    fs::write(repo.join("one.txt"), "base\n").unwrap();
    git(&repo, &["add", "--", "one.txt"]);
    git(&repo, &["commit", "-m", "base"]);
    git(&repo, &["branch", "delivery"]);
    (root, repo)
}
#[test]
fn immutable_snapshot_and_unchecked_target_cas_preserve_user_worktree() {
    let (root, repo) = repo();
    let p = coding::preflight(&repo, "refs/heads/delivery").unwrap();
    let owned = root.join("owned");
    fs::create_dir(&owned).unwrap();
    let w = coding::prepare(&owned, "worker-a", &p).unwrap();
    fs::write(Path::new(&w.directory).join("one.txt"), "changed\n").unwrap();
    fs::write(Path::new(&w.directory).join("new.txt"), "new\n").unwrap();
    let evidence = coding::snapshot(&owned, "worker-a", &w).unwrap();
    assert!(evidence.complete);
    assert!(evidence.diff.contains("changed"));
    let integrated = coding::integrate(&owned, &p, &[evidence.clone()]).unwrap();
    assert!(coding::merge_ref(&p, &integrated.commit).is_ok());
    assert_eq!(
        git(&repo, &["rev-parse", "refs/heads/delivery"]),
        integrated.commit
    );
    assert_eq!(fs::read_to_string(repo.join("one.txt")).unwrap(), "base\n");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), p.base);
    assert!(coding::merge_ref(&p, &evidence.commit).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn hooks_filters_symlinks_and_overlapping_files_cannot_bypass_git_boundary() {
    let (root, repo) = repo();
    let sentinel = root.join("executed");
    fs::create_dir_all(repo.join(".git/hooks")).unwrap();
    let hook = repo.join(".git/hooks/post-checkout");
    fs::write(
        &hook,
        format!("#!/bin/sh\ntouch '{}'\n", sentinel.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let p = coding::preflight(&repo, "refs/heads/delivery").unwrap();
    let owned = root.join("owned");
    fs::create_dir(&owned).unwrap();
    let a = coding::prepare(&owned, "a", &p).unwrap();
    let b = coding::prepare(&owned, "b", &p).unwrap();
    assert!(!sentinel.exists());
    for w in [&a, &b] {
        fs::write(Path::new(&w.directory).join("one.txt"), "changed").unwrap();
    }
    let sa = coding::snapshot(&owned, "a", &a).unwrap();
    let sb = coding::snapshot(&owned, "b", &b).unwrap();
    assert!(coding::integrate(&owned, &p, &[sa, sb]).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(repo.join("one.txt"), Path::new(&a.directory).join("escape"))
            .unwrap();
        assert!(coding::snapshot(&owned, "a", &a).is_err());
        fs::remove_file(Path::new(&a.directory).join("escape")).unwrap();
    }
    git(
        &repo,
        &[
            "config",
            "filter.evil.clean",
            &format!("touch {}", sentinel.display()),
        ],
    );
    assert!(coding::snapshot(&owned, "a", &a).is_err());
    assert!(!sentinel.exists());
    let mut checked = p.clone();
    checked.target = "refs/heads/main".into();
    assert!(coding::merge_ref(&checked, &p.base).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn writing_protocol_fails_closed_and_code_backup_preserves_reachable_objects() {
    use serde_json::json;
    let (root, repo) = repo();
    let p = coding::preflight(&repo, "refs/heads/delivery").unwrap();
    let owned = root.join("owned");
    fs::create_dir(&owned).unwrap();
    let w = coding::prepare(&owned, "safe", &p).unwrap();
    let mut task = crate::model::Task::new("code".into(), "change".into(), "research".into());
    task.code_workspace = Some(w.clone());
    for method in ["thread/start", "thread/resume", "thread/fork"] {
        let r = crate::protocol::writing_thread(&task, json!({"method":method,"params":{}}));
        assert_eq!(r["params"]["cwd"], w.directory);
        assert_eq!(r["params"]["sandbox"], "workspace-write");
        assert_eq!(
            r["params"]["config"]["sandbox_workspace_write.writable_roots"],
            json!([w.directory])
        );
        assert_eq!(r["params"]["config"]["mcp_servers"], json!({}));
    }
    let turn = crate::protocol::writing_turn(&task, json!({"params":{}}));
    let policy = turn["params"]["sandboxPolicy"].clone();
    let response = json!({"cwd":w.directory,"sandbox":policy});
    assert!(crate::protocol::validate_writing_response(&task, &response).is_ok());
    for key in [
        "networkAccess",
        "excludeSlashTmp",
        "excludeTmpdirEnvVar",
        "writableRoots",
    ] {
        let mut bad = response.clone();
        bad["sandbox"].as_object_mut().unwrap().remove(key);
        assert!(crate::protocol::validate_writing_response(&task, &bad).is_err());
    }
    let mut outside = response.clone();
    outside["sandbox"]["writableRoots"] = json!([w.directory, p.common_dir]);
    assert!(crate::protocol::validate_writing_response(&task, &outside).is_err());
    assert!(crate::protocol::empty_mcp(
        &json!({"data":[],"nextCursor":null})
    ));
    for result in [
        json!({"data":[]}),
        json!({"data":[],"nextCursor":"more"}),
        json!({"data":[{"name":"server"}],"nextCursor":null}),
    ] {
        assert!(!crate::protocol::empty_mcp(&result));
    }
    fs::write(Path::new(&w.directory).join("one.txt"), "backup\n").unwrap();
    let e = coding::snapshot(&owned, "safe", &w).unwrap();
    assert_eq!(
        git(
            &repo,
            &["rev-parse", &format!("refs/orbit/snapshots/{}", e.commit)]
        ),
        e.commit
    );
    task.code_workspace.as_mut().unwrap().snapshot = Some(e.clone());
    let bundles = coding::export_bundles(&[task]).unwrap();
    assert_eq!(bundles.len(), 1);
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&bundles[0].bundle)
        .unwrap();
    let file = root.join("delivery.bundle");
    fs::write(&file, bytes).unwrap();
    git(&repo, &["bundle", "verify", file.to_str().unwrap()]);
    assert!(owned
        .join("operations")
        .join(format!("{}.json", e.commit))
        .exists());
    #[cfg(unix)]
    {
        let path = Path::new(&w.directory).join("fifo");
        let name = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(coding::snapshot(&owned, "safe", &w).is_err());
        fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    {
        let wd = Path::new(&w.directory);
        fs::write(wd.join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir(wd.join("ignored")).unwrap();
        std::os::unix::fs::symlink("/private/tmp", wd.join("ignored/escape")).unwrap();
        assert!(coding::snapshot(&owned, "safe", &w).is_ok());
        fs::create_dir(wd.join("visible")).unwrap();
        let name = std::ffi::CString::new(wd.join("visible/fifo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(coding::snapshot(&owned, "safe", &w).is_err());
        fs::remove_file(wd.join("visible/fifo")).unwrap();
        std::os::unix::fs::symlink("/private/tmp", wd.join("outside")).unwrap();
        assert!(coding::snapshot(&owned, "safe", &w).is_err());
        fs::remove_file(wd.join("outside")).unwrap();
        let mut dir = wd.to_path_buf();
        for _ in 0..34 {
            dir = dir.join("nested");
            fs::create_dir(&dir).unwrap();
        }
        assert!(coding::snapshot(&owned, "safe", &w).is_err());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn promisor_repository_is_rejected_before_object_access() {
    let (root, repo) = repo();
    let sentinel = root.join("transport-started");
    let blob = git(&repo, &["rev-parse", "HEAD:one.txt"]);
    fs::remove_file(repo.join(".git/objects").join(&blob[..2]).join(&blob[2..])).unwrap();
    git(&repo, &["config", "remote.origin.promisor", "true"]);
    git(
        &repo,
        &[
            "config",
            "remote.origin.url",
            &format!("ext::sh -c 'touch {}'", sentinel.display()),
        ],
    );
    assert!(coding::preflight(&repo, "refs/heads/delivery").is_err());
    assert!(!sentinel.exists());
    fs::remove_dir_all(root).unwrap();
}
