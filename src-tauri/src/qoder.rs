use crate::{
    executor::{Capabilities, Descriptor, Doctor, Executor, SessionRef},
    model::{now, Task, OUTPUT_LIMIT},
    process::{send, OwnedChild},
    protocol::{error_message, read_message},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter};

#[derive(PartialEq)]
enum Phase {
    Initialize,
    Session,
    Prompt,
    Ended,
}
struct AcpSession {
    phase: Phase,
    resume: bool,
    output: String,
    files: Option<TaskFiles>,
}
impl AcpSession {
    fn new(resume: bool) -> Self {
        Self {
            phase: Phase::Initialize,
            resume,
            output: String::new(),
            files: None,
        }
    }
    fn receive(
        &mut self,
        task: &mut Task,
        message: &Value,
        cwd: &Path,
    ) -> Result<Vec<Value>, String> {
        if self.phase == Phase::Ended {
            return Ok(vec![]);
        }
        if message.get("jsonrpc").is_some_and(|v| v != "2.0") {
            return Err("Qoder 返回不兼容的 JSON-RPC 协议".into());
        }
        if let Some(method) = message["method"].as_str() {
            let session = task.session_ref.as_ref().map(|s| s.id.as_str());
            let matching = message["params"]["sessionId"]
                .as_str()
                .is_some_and(|id| Some(id) == session);
            if message.get("id").is_some() {
                let response = if method == "session/request_permission" {
                    task.event(
                        "Qoder 工具权限请求已拒绝；客户端不授予写入或命令执行权限",
                        "approval",
                        "工作台",
                    );
                    if task.status == "cancelling" {
                        cancelled_permission(message)
                    } else {
                        deny_permission(message)
                    }
                } else if method == "fs/read_text_file" && matching {
                    match self
                        .files
                        .as_ref()
                        .ok_or("任务目录读取未授权".to_string())
                        .and_then(|files| files.read_text(&message["params"]))
                    {
                        Ok(content) => {
                            json!({"jsonrpc":"2.0","id":message["id"],"result":{"content":content}})
                        }
                        Err(error) => rpc_error(&message["id"], -32000, &error),
                    }
                } else {
                    rpc_error(
                        &message["id"],
                        -32601,
                        "Orbit 未开放此客户端能力或会话已失效",
                    )
                };
                return Ok(vec![response]);
            }
            if method != "session/update" || !matching || self.phase != Phase::Prompt {
                return Ok(vec![]);
            }
            let update = &message["params"]["update"];
            match update["sessionUpdate"].as_str() {
                Some("agent_message_chunk") if update["content"]["type"] == "text" => {
                    if let Some(text) = update["content"]["text"].as_str() {
                        task.confirm_source_input();
                        let remaining = OUTPUT_LIMIT.saturating_sub(self.output.chars().count());
                        self.output.extend(text.chars().take(remaining));
                        if text.chars().count() > remaining {
                            if let Some(root) = task.nodes.first_mut() {
                                root.output_truncated = true;
                            }
                        }
                        task.collect_answer("qoder-response", &self.output);
                        crate::conversation::qoder_output(task,&self.output,false,task.nodes.first().is_some_and(|n|n.output_truncated));
                        task.event("收到执行器文本输出", "output", "Qoder");
                    }
                }
                Some("tool_call" | "tool_call_update") => {
                    task.confirm_source_input();
                    let title = update["title"].as_str().unwrap_or("工具状态更新");
                    task.event(
                        &format!(
                            "{} · {}",
                            title,
                            update["status"].as_str().unwrap_or("进行中")
                        ),
                        "tool",
                        "Qoder",
                    );
                    if let Some(root) = task.nodes.first_mut() {
                        root.summary = title.chars().take(500).collect();
                    }
                }
                _ => {}
            }
            return Ok(vec![]);
        }
        let expected = match self.phase {
            Phase::Initialize => 1,
            Phase::Session => 2,
            Phase::Prompt => 3,
            Phase::Ended => 0,
        };
        if message["id"].as_u64() != Some(expected) {
            return Ok(vec![]);
        }
        if message.get("error").is_some() {
            return Err(format!(
                "Qoder 请求被拒绝（请核对 CLI 登录与配置）：{}",
                error_message(&message["error"])
            ));
        }
        let result = message.get("result").ok_or("Qoder 响应缺少 result")?;
        match self.phase {
            Phase::Initialize => {
                if result["protocolVersion"] != 1 {
                    return Err("Qoder ACP 协议版本不兼容，仅支持 v1".into());
                }
                let load = result["agentCapabilities"]["loadSession"] == true;
                task.capabilities = Some(Capabilities::acp(load));
                if self.resume && !load {
                    return Err("当前 Qoder 未声明支持会话加载；未发送补充，已有交付保留".into());
                }
                self.phase = Phase::Session;
                task.event("Qoder ACP 初始化成功", "system", "工作台");
                let request = if self.resume {
                    let session = task
                        .session_ref
                        .as_ref()
                        .filter(|s| s.provider == "qoder" && s.protocol == "acp-v1")
                        .ok_or("Qoder 会话引用不匹配")?;
                    rpc(
                        2,
                        "session/load",
                        json!({"sessionId":session.id,"cwd":cwd,"mcpServers":[]}),
                    )
                } else {
                    rpc(2, "session/new", json!({"cwd":cwd,"mcpServers":[]}))
                };
                Ok(vec![request])
            }
            Phase::Session => {
                if !self.resume {
                    let id = result["sessionId"]
                        .as_str()
                        .filter(|id| !id.is_empty() && id.len() <= 400)
                        .ok_or("Qoder 未返回有效 sessionId")?;
                    task.session_ref = Some(SessionRef {
                        provider: "qoder".into(),
                        protocol: "acp-v1".into(),
                        id: id.into(),
                        cwd: Some(cwd.to_string_lossy().into()),
                        metadata: json!({"taskId":task.id}),
                        extra: Default::default(),
                    });
                } else if result.get("sessionId").is_some_and(|id| {
                    Some(id.as_str().unwrap_or(""))
                        != task.session_ref.as_ref().map(|s| s.id.as_str())
                }) {
                    return Err("Qoder 加载返回了其他会话，未发送补充".into());
                }
                task.root_node();
                if let Some(root) = task.nodes.first_mut() {
                    root.model = "由 Qoder CLI 管理 · 实际模型未返回".into();
                    root.detail_notice =
                        Some("ACP 当前未提供可核对的子 Agent 历史；工具进度见运行记录".into());
                }
                let session = task
                    .session_ref
                    .as_ref()
                    .ok_or("Qoder 会话未创建")?
                    .id
                    .clone();
                let text=task.output_instruction(&task.execution_input());
                // ACP has no turn ID: this token identifies only our prompt request.
                task.turn_id = Some(format!(
                    "{}:3",
                    task.run_id.as_deref().ok_or("运行标识缺失")?
                ));
                task.event("会话已就绪，启动本轮任务", "system", "工作台");
                self.phase = Phase::Prompt;
                Ok(vec![rpc(
                    3,
                    "session/prompt",
                    json!({"sessionId":session,"prompt":[{"type":"text","text":text}]}),
                )])
            }
            Phase::Prompt => {
                let reason = result["stopReason"]
                    .as_str()
                    .ok_or("Qoder 未返回有效停止原因")?;
                task.confirm_source_input();
                task.status = match reason {
                    "end_turn" => "completed",
                    "cancelled" => "interrupted",
                    _ => "failed",
                }
                .into();
                task.finished_at = Some(now());
                task.approvals.clear();
                task.event(
                    &format!("Qoder 本轮结束：{reason}"),
                    if task.status == "failed" {
                        "error"
                    } else {
                        "system"
                    },
                    "Qoder",
                );
                for node in &mut task.nodes {
                    node.status = task.status.clone();
                    node.summary = format!("本轮结束：{reason}");
                }
                if reason=="end_turn" {
                    crate::conversation::qoder_output(task,&self.output,true,task.nodes.first().is_some_and(|n|n.output_truncated));
                    task.delivery_candidate.capture("qoder-response",&self.output);
                    if task.nodes.first().is_some_and(|n|n.output_truncated){task.delivery_candidate.truncated=true;}
                }
                self.phase = Phase::Ended;
                Ok(vec![])
            }
            Phase::Ended => Ok(vec![]),
        }
    }
}
fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn initialize() -> Value {
    rpc(
        1,
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":cfg!(unix),"writeTextFile":false},"terminal":false},"clientInfo":{"name":"orbit-workbench","version":"0.1.0"}}),
    )
}
fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn deny_permission(message: &Value) -> Value {
    let option = message["params"]["options"]
        .as_array()
        .and_then(|options| options.iter().find(|o| o["kind"] == "reject_once"));
    let outcome = match option.and_then(|o| o["optionId"].as_str()) {
        Some(id) => json!({"outcome":"selected","optionId":id}),
        None => json!({"outcome":"cancelled"}),
    };
    json!({"jsonrpc":"2.0","id":message["id"],"result":{"outcome":outcome}})
}
fn cancelled_permission(message: &Value) -> Value {
    json!({"jsonrpc":"2.0","id":message["id"],"result":{"outcome":{"outcome":"cancelled"}}})
}
struct TaskFiles {
    root: PathBuf,
    directory: File,
}
impl TaskFiles {
    fn open(root: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Open the exact authorized directory once, without resolving a replacement symlink.
            let directory = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(root)
                .map_err(|_| "任务目录不可用或含有符号链接")?;
            Ok(Self {
                root: root.to_path_buf(),
                directory,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = root;
            Err("此平台尚未开放 ACP 文件读取".into())
        }
    }
    fn for_task(root: &Path, task_id: &str) -> Result<Self, String> {
        let files = Self::open(root)?;
        if files.read_text(&json!({"path":root.join(".orbit-task")}))? != task_id {
            return Err("会话目录不属于当前任务，未发送补充".into());
        }
        Ok(files)
    }
    #[cfg(unix)]
    fn open_file(&self, path: &Path) -> Result<File, String> {
        use std::{
            ffi::CString,
            os::unix::{
                ffi::OsStrExt,
                io::{AsRawFd, FromRawFd},
            },
        };
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| "文件位于当前任务授权目录之外")?;
        let parts = relative.components().collect::<Vec<_>>();
        if parts.is_empty()
            || parts
                .iter()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("文件路径越界或无效".into());
        }
        let mut directory = self
            .directory
            .try_clone()
            .map_err(|_| "任务目录句柄不可用")?;
        for (index, part) in parts.iter().enumerate() {
            let name = CString::new(part.as_os_str().as_bytes()).map_err(|_| "文件路径无效")?;
            let last = index + 1 == parts.len();
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | if last {
                    libc::O_NONBLOCK
                } else {
                    libc::O_DIRECTORY
                };
            let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err("文件不可读或含有符号链接".into());
            }
            let file = unsafe { File::from_raw_fd(fd) };
            if last {
                if !file.metadata().map_err(|_| "文件信息不可用")?.is_file() {
                    return Err("只支持普通文本文件".into());
                }
                return Ok(file);
            }
            directory = file;
        }
        Err("文件路径无效".into())
    }
    #[cfg(not(unix))]
    fn open_file(&self, _path: &Path) -> Result<File, String> {
        Err("此平台尚未开放 ACP 文件读取".into())
    }
    fn read_text(&self, params: &Value) -> Result<String, String> {
        let path = Path::new(params["path"].as_str().ok_or("文件路径缺失")?);
        if !path.is_absolute() {
            return Err("只接受绝对文件路径".into());
        }
        let file = self.open_file(path)?;
        if file.metadata().map_err(|_| "文件信息不可用")?.len() > 256_000 {
            return Err("文件超过 256 KB 读取限制".into());
        }
        let mut bytes = Vec::new();
        file.take(256_001)
            .read_to_end(&mut bytes)
            .map_err(|_| "读取文件失败")?;
        if bytes.len() > 256_000 {
            return Err("文件超过读取限制".into());
        }
        let text = String::from_utf8(bytes).map_err(|_| "只支持 UTF-8 文本文件")?;
        if params["line"].is_null() && params["limit"].is_null() {
            return Ok(text);
        }
        let line = if params["line"].is_null() {
            1
        } else {
            params["line"]
                .as_u64()
                .filter(|n| *n > 0)
                .ok_or("起始行无效")?
        };
        let limit = if params["limit"].is_null() {
            256_000
        } else {
            params["limit"].as_u64().ok_or("行数限制无效")?.min(256_000)
        };
        Ok(text
            .lines()
            .skip(line.saturating_sub(1).min(256_000) as usize)
            .take(limit as usize)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}
fn read_text(root: &Path, params: &Value) -> Result<String, String> {
    TaskFiles::open(root)?.read_text(params)
}
fn cli_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut roots = vec![
        home.join(".local/bin"),
        home.join(".qoder/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        roots.extend(std::env::split_paths(&path));
    }
    for root in roots {
        for name in ["qodercli", "qoder"] {
            let path = root.join(name);
            if path.is_file() {
                return path;
            }
        }
    }
    PathBuf::from("qodercli")
}
fn command_at(path: &Path, model: Option<&str>) -> Command {
    let mut command = Command::new(path);
    command
        .arg("--acp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(model) = model { command.arg("--model").arg(model); }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
}
pub fn doctor() -> Doctor {
    let path = cli_path();
    let mut report = Doctor {
        provider: "qoder".into(),
        available: false,
        initialized: false,
        path: path.display().to_string(),
        version: String::new(),
        message: "未找到 Qoder CLI；请安装并在 CLI 中登录".into(),
        capabilities: Capabilities::default(),
    };
    let Ok(child) = command_at(&path, None).spawn() else {
        return report;
    };
    report.available = true;
    let mut child = OwnedChild::new(child);
    let stdout = child.child.stdout.take().unwrap();
    let mut stderr = child.child.stderr.take().unwrap();
    thread::spawn(move || {
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
    });
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = tx.send(read_message(&mut BufReader::new(stdout)));
    });
    if send(child.child.stdin.as_mut().unwrap(), &initialize()).is_ok() {
        match rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(Some(message)))
                if message["id"] == 1 && message["result"]["protocolVersion"] == 1 =>
            {
                report.initialized = true;
                report.version = message["result"]["agentInfo"]["version"]
                    .as_str()
                    .unwrap_or("版本未返回")
                    .into();
                report.capabilities = Capabilities::acp(
                    message["result"]["agentCapabilities"]["loadSession"] == true,
                );
                report.message = "Qoder ACP v1 初始化成功 · 未调用模型".into();
            }
            Ok(Ok(Some(message))) if message.get("error").is_some() => {
                report.message = format!("Qoder 初始化失败：{}", error_message(&message["error"]))
            }
            _ => report.message = "Qoder ACP 初始化失败或超时；请核对 CLI 版本与登录状态".into(),
        }
    }
    if child.stop().is_err() {
        report.initialized = false;
        report.message = "Qoder 检查进程清理未确认，请核对本机进程".into();
    }
    report
}

struct Run {
    task: Task,
    child: OwnedChild,
    input: mpsc::SyncSender<Value>,
    writer_closed: Arc<AtomicBool>,
    state: AcpSession,
    closed: bool,
}
fn queue(input: &mpsc::SyncSender<Value>, message: Value) -> Result<(), String> {
    input
        .try_send(message)
        .map_err(|_| "Qoder 发送队列已满或关闭，执行结果需核对".into())
}
// Keep cancellation cleanup independent of the result of persisting its snapshot.
fn publish_cancellation(
    delay: Duration,
    publish: impl FnOnce() -> Result<(), String>,
    on_timeout: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    thread::spawn(move || {
        thread::sleep(delay);
        on_timeout();
    });
    publish()
}
fn write_messages(
    mut input: ChildStdin,
    rx: mpsc::Receiver<Value>,
    closed: Arc<AtomicBool>,
) -> Result<(), String> {
    while !closed.load(Ordering::Acquire) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => {
                if closed.load(Ordering::Acquire) {
                    break;
                }
                send(&mut input, &message)?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}
#[derive(Clone)]
pub struct QoderExecutor {
    starting:Arc<Mutex<bool>>,
    store: Arc<Store>,
    runs: Arc<Mutex<HashMap<String, Arc<Mutex<Run>>>>>,
}
impl QoderExecutor {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            starting:Arc::new(Mutex::new(false)),
            store,
            runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    fn begin_start(&self)->Result<std::sync::MutexGuard<'_,bool>,String>{let g=self.starting.lock().unwrap();if *g{Err("工作台正在关闭，不能启动 Qoder".into())}else{Ok(g)}}
    fn publish(&self, app: &AppHandle, task: Task) -> Result<(), String> {
        match self.store.save_existing_task(task)? {
            Some(saved) => {
                let _ = app.emit("runtime-task", saved);
                Ok(())
            }
            None => Err("任务归属已变化，停止发送执行请求".into()),
        }
    }
    fn fail(&self, app: &AppHandle, run: &Arc<Mutex<Run>>, error: &str, status: &str) {
        let mut r = run.lock().unwrap();
        if r.closed {
            return;
        }
        r.closed = true;
        r.writer_closed.store(true, Ordering::Release);
        r.state.phase = Phase::Ended;
        finish_failure(&mut r.task, error, status);
        if r.child.stop().is_err() {
            r.task.event(
                "拥有的 Qoder 进程清理未确认，已阻止新运行",
                "error",
                "工作台",
            );
        }
        let task = r.task.clone();
        drop(r);
        if let Err(error) = self.publish(app, task) {
            let _ = app.emit("runtime-warning", error);
        }
    }
    fn working_directory(&self, task: &Task, resume: bool) -> Result<PathBuf, String> {
        let runs = self.store.directory.join("runs");
        std::fs::create_dir_all(&runs).map_err(|_| "无法创建运行目录")?;
        let runs = runs.canonicalize().map_err(|_| "运行目录不可用")?;
        if resume {
            let session = task
                .session_ref
                .as_ref()
                .filter(|s| s.provider == "qoder" && s.protocol == "acp-v1")
                .ok_or("Qoder 会话引用不匹配")?;
            let cwd = PathBuf::from(session.cwd.as_deref().ok_or("原会话工作目录缺失")?);
            let actual = cwd.canonicalize().map_err(|_| "原会话工作目录不可用")?;
            if actual != cwd
                || actual.parent() != Some(runs.as_path())
                || !actual.is_dir()
                || session.metadata["taskId"] != task.id
            {
                return Err("原会话工作目录已变化；已有交付保留".into());
            }
            let owner = read_text(&actual, &json!({"path":actual.join(".orbit-task")}))?;
            if owner != task.id {
                return Err("会话目录不属于当前任务，未发送补充".into());
            }
            Ok(actual)
        } else {
            let id = task
                .run_id
                .as_deref()
                .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                .ok_or("运行标识无效")?;
            let cwd = runs.join(id);
            std::fs::create_dir(&cwd).map_err(|_| "无法创建本轮目录")?;
            std::fs::write(cwd.join(".orbit-task"), task.id.as_bytes())
                .map_err(|_| "无法保存任务目录归属")?;
            Ok(cwd)
        }
    }
}
fn finish_failure(task: &mut Task, error: &str, status: &str) {
    task.status = status.into();
    task.finished_at = Some(now());
    task.approvals.clear();
    task.unconfirm_directions();
    for node in &mut task.nodes {
        node.status = status.into();
        node.summary = "本轮执行已终止".into();
    }
    task.event(error, "error", "工作台");
}
impl Executor for QoderExecutor {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "qoder".into(),
            name: "Qoder".into(),
            protocol: "acp-v1".into(),
            description: "本机 CLI · ACP 接入，续交付以握手能力为准".into(),
            permission_note: "仅开放任务目录读取；拒绝工具权限请求、客户端写入与终端，非 OS 沙箱"
                .into(),
            capabilities: Capabilities::acp(false),
        }
    }
    fn doctor(&self) -> Doctor {
        doctor()
    }
    fn models(&self) -> Result<Vec<crate::executor::ExecutorModel>, String> { model_catalog() }
    fn ensure_idle(&self) -> Result<(), String> {
        for run in self.runs.lock().unwrap().values() {
            let mut r = run.lock().unwrap();
            if !r.closed {
                return Err("当前已有 Qoder 任务运行，请先结束或中断它".into());
            }
            if r.child.stop().is_err() {
                return Err("Qoder 进程清理未确认，不能启动新任务".into());
            }
        }
        Ok(())
    }
    fn ownership(&self)->Vec<crate::executor::Ownership>{
        self.runs.lock().unwrap().values().filter_map(|run|{let r=run.lock().unwrap();(!r.closed||!r.child.stopped).then(||crate::executor::Ownership{task_id:r.task.id.clone(),run_id:r.task.run_id.clone().unwrap(),session_id:r.task.session_ref.as_ref().map(|s|s.id.clone())})}).collect()
    }
    fn abort_start(&self,app:AppHandle,id:String)->Result<(),String>{
        let run=self.runs.lock().unwrap().get(&id).cloned().ok_or("运行不存在")?;
        self.fail(&app,&run,"启动已取消","interrupted");let mut r=run.lock().unwrap();r.child.stop()
    }
    fn ensure_task_idle(&self, task_id: &str) -> Result<(), String> {
        for run in self.runs.lock().unwrap().values() {
            let r = run.lock().unwrap();
            if r.task.id == task_id && (!r.closed || !r.child.stopped) {
                return Err("该 Qoder 任务仍在运行或清理尚未确认".into());
            }
        }
        Ok(())
    }
    fn launch(
        &self,
        app: AppHandle,
        mut task: Task,
        resume_anchor: Option<String>,
    ) -> Result<Task, String> {
        let handoff=self.begin_start()?;
        let resume = resume_anchor.is_some();
        let cwd = match self.working_directory(&task, resume) {
            Ok(cwd) => cwd,
            Err(error) => {
                finish_failure(&mut task, &error, "failed");
                self.publish(&app, task.clone())?;
                return Ok(task);
            }
        };
        #[cfg(unix)]
        let files = match TaskFiles::for_task(&cwd, &task.id) {
            Ok(files) => Some(files),
            Err(error) => {
                finish_failure(&mut task, &error, "failed");
                self.publish(&app, task.clone())?;
                return Ok(task);
            }
        };
        #[cfg(not(unix))]
        let files = None;
        let mut command = command_at(&cli_path(), task.requested_model.as_deref());
        let child = match command.current_dir(&cwd).spawn() {
            Ok(child) => child,
            Err(_) => {
                finish_failure(
                    &mut task,
                    "无法启动 Qoder CLI，请先在设置中检查连接；已有交付保留",
                    "failed",
                );
                self.publish(&app, task.clone())?;
                return Ok(task);
            }
        };
        let mut child = OwnedChild::new(child);
        let child_input = child.child.stdin.take().unwrap();
        let output = child.child.stdout.take().unwrap();
        let mut stderr = child.child.stderr.take().unwrap();
        thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        let (input, receiver) = mpsc::sync_channel(8);
        let writer_closed = Arc::new(AtomicBool::new(false));
        let mut state = AcpSession::new(resume);
        state.files = files;
        let run = Arc::new(Mutex::new(Run {
            task: task.clone(),
            child,
            input,
            writer_closed: writer_closed.clone(),
            state,
            closed: false,
        }));
        let writer_runtime = self.clone();
        let writer_run = run.clone();
        let writer_app = app.clone();
        thread::spawn(move || {
            if let Err(error) = write_messages(child_input, receiver, writer_closed) {
                writer_runtime.fail(&writer_app, &writer_run, &error, "unknown");
            }
        });
        {
            let mut runs = self.runs.lock().unwrap();
            runs.retain(|_, run| {let r=run.lock().unwrap();!r.closed||!r.child.stopped});
            runs.insert(task.run_id.clone().unwrap(), run.clone());
        }
        drop(handoff);
        let _ = app.emit("runtime-task", task.clone());
        if queue(&run.lock().unwrap().input, initialize()).is_err() {
            self.fail(&app, &run, "Qoder 初始化发送失败", "failed");
            return Ok(run.lock().unwrap().task.clone());
        }
        let runtime = self.clone();
        let reader_run = run.clone();
        let reader_app = app.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let message = match read_message(&mut reader) {
                    Ok(Some(message)) => message,
                    Ok(None) => {
                        runtime.fail(
                            &reader_app,
                            &reader_run,
                            "Qoder 连接关闭，在途结果需核对",
                            "unknown",
                        );
                        break;
                    }
                    Err(error) => {
                        runtime.fail(&reader_app, &reader_run, &error, "unknown");
                        break;
                    }
                };
                let mut r = reader_run.lock().unwrap();
                if r.closed {
                    break;
                }
                let before = r.task.revision;
                let Run { task, state, .. } = &mut *r;
                let requests = match state.receive(task, &message, &cwd) {
                    Ok(requests) => requests,
                    Err(error) => {
                        let status = if r.state.phase == Phase::Prompt {
                            "unknown"
                        } else {
                            "failed"
                        };
                        drop(r);
                        runtime.fail(&reader_app, &reader_run, &error, status);
                        break;
                    }
                };
                if r.task.revision != before {
                    if let Err(error) = runtime.publish(&reader_app, r.task.clone()) {
                        drop(r);
                        runtime.fail(&reader_app, &reader_run, &error, "unknown");
                        break;
                    }
                }
                let mut send_error = false;
                for request in requests {
                    if queue(&r.input, request).is_err() {
                        send_error = true;
                        break;
                    }
                }
                if send_error {
                    drop(r);
                    runtime.fail(
                        &reader_app,
                        &reader_run,
                        "Qoder 请求发送结果未知，请勿重复发送",
                        "unknown",
                    );
                    break;
                }
                if r.state.phase == Phase::Ended {
                    r.closed = true;
                    r.writer_closed.store(true, Ordering::Release);
                    if r.child.stop().is_err() {
                        r.task
                            .event("Qoder 进程清理未确认，已阻止新运行", "error", "工作台");
                        let _ = runtime.publish(&reader_app, r.task.clone());
                    }
                    break;
                }
            }
        });
        let runtime = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(25));
            let pending = {
                let r = run.lock().unwrap();
                !r.closed && r.state.phase != Phase::Prompt
            };
            if pending {
                runtime.fail(&app, &run, "Qoder 启动超时，状态需核对", "unknown");
            }
        });
        Ok(task)
    }
    fn interrupt(&self, app: AppHandle, id: String) -> Result<(), String> {
        let run = self
            .runs
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or("Qoder 运行已失效")?;
        let mut r = run.lock().unwrap();
        if r.closed {
            return Err("运行已结束".into());
        }
        if r.task.status == "cancelling" {
            return Ok(());
        }
        let session = r.task.session_ref.as_ref().map(|s| s.id.clone());
        if r.state.phase != Phase::Prompt || session.is_none() {
            drop(r);
            self.fail(&app, &run, "已停止 Qoder 启动，外部状态需核对", "unknown");
            return Ok(());
        }
        let request = json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session.unwrap()}});
        if queue(&r.input, request).is_err() {
            drop(r);
            self.fail(&app, &run, "Qoder 中断发送结果未知", "unknown");
            return Err("中断发送结果未知".into());
        }
        r.task.status = "cancelling".into();
        r.task
            .event("已请求 Qoder 中断，等待本轮结束确认", "system", "工作台");
        let task = r.task.clone();
        drop(r);
        let runtime = self.clone();
        let timeout_app = app.clone();
        publish_cancellation(
            Duration::from_secs(10),
            || self.publish(&app, task),
            move || {
                runtime.fail(
                    &timeout_app,
                    &run,
                    "Qoder 中断确认超时；已尝试停止自有进程，结果需核对",
                    "unknown",
                )
            },
        )
    }

    fn refresh(&self, task: &Task) {
        for run in self.runs.lock().unwrap().values() {
            run.lock().unwrap().task.merge_platform(task);
        }
    }
    fn forget(&self, task_id: &str) {
        self.runs
            .lock()
            .unwrap()
            .retain(|_, r| {let r=r.lock().unwrap();r.task.id != task_id||!r.closed||!r.child.stopped});
    }
    fn shutdown(&self) {
        let mut gate=self.starting.lock().unwrap();*gate=true;
        for run in self.runs.lock().unwrap().values() {
            let mut r = run.lock().unwrap();
            if !r.closed {
                r.closed = true;
                r.writer_closed.store(true, Ordering::Release);
                finish_failure(&mut r.task, "App 已关闭，Qoder 在途结果需核对", "unknown");
            }
            if r.child.stop().is_err() {
                r.task
                    .event("拥有的 Qoder 进程清理未确认", "error", "工作台");
            }
            let _ = self.store.save_existing_task(r.task.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn task() -> crate::model::Task {
        let mut task = crate::model::Task::new("test".into(), "original".into(), "research".into());
        task.provider = "qoder".into();
        task
    }
    #[test]
    fn source_receipt_requires_remote_prompt_response_not_local_request_token() {
        use crate::{sources::SourceInput,store::Store};
        let dir=std::env::temp_dir().join(format!("orbit-qoder-source-{}",uuid::Uuid::new_v4()));
        let store=Store::open(dir.clone()).unwrap();let mut task=task();
        task.source_inputs.push(SourceInput::new(task.run_id.clone().unwrap(),"initial",task.prompt.clone(),vec![],task.run_id.clone(),None));store.save_task(task.clone()).unwrap();
        let mut state=AcpSession::new(false);let cwd=std::path::Path::new("/tmp/task");
        state.receive(&mut task,&json!({"id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}),cwd).unwrap();
        let requests=state.receive(&mut task,&json!({"id":2,"result":{"sessionId":"s"}}),cwd).unwrap();
        assert_eq!(requests[0]["params"]["prompt"][0]["text"],crate::delivery::instruction(&task.execution_input()));
        assert!(task.turn_id.is_some());let pending=store.save_existing_task(task.clone()).unwrap().unwrap();assert_eq!(pending.source_inputs[0].status,"pending");
        let mut failed=task.clone();failed.status="unknown".into();failed.event("write failed","error","fixture");let unknown=store.save_existing_task(failed).unwrap().unwrap();assert_eq!(unknown.source_inputs[0].status,"unknown");
        state.receive(&mut task,&json!({"method":"session/update","params":{"sessionId":"foreign","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"wrong"}}}}),cwd).unwrap();assert_eq!(task.source_inputs[0].status,"pending");
        task.revision=unknown.executor_revision.unwrap();
        state.receive(&mut task,&json!({"id":3,"result":{"stopReason":"end_turn"}}),cwd).unwrap();
        let accepted=store.save_existing_task(task).unwrap().unwrap();assert_eq!(accepted.source_inputs[0].status,"accepted");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn acp_handshake_session_and_foreign_events() {
        let mut task = task();
        let mut state = AcpSession::new(false);
        let requests=state.receive(&mut task,&json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}),std::path::Path::new("/tmp/task")).unwrap();
        assert_eq!(requests[0]["method"], "session/new");
        let requests = state
            .receive(
                &mut task,
                &json!({"id":2,"result":{"sessionId":"s"}}),
                std::path::Path::new("/tmp/task"),
            )
            .unwrap();
        assert_eq!(requests[0]["method"], "session/prompt");
        assert_eq!(requests[0]["params"]["prompt"][0]["text"], crate::delivery::instruction("original"));
        assert!(task.can_resume());
        state.receive(&mut task,&json!({"method":"session/update","params":{"sessionId":"foreign","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"wrong"}}}}),std::path::Path::new("/tmp/task")).unwrap();
        assert!(task.nodes[0].output.is_empty());
        state.receive(&mut task,&json!({"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"answer"}}}}),std::path::Path::new("/tmp/task")).unwrap();
        state
            .receive(
                &mut task,
                &json!({"id":3,"result":{"stopReason":"end_turn"}}),
                std::path::Path::new("/tmp/task"),
            )
            .unwrap();
        assert_eq!(task.status, "completed");
        assert!(task.artifacts.is_empty());
        assert_eq!(task.conversation[0].text,"answer");
        assert_eq!(task.conversation[0].status,"completed");
    }
    #[test]
    fn acp_resume_rechecks_capability_and_does_not_copy_replayed_text() {
        let mut task = task();
        task.session_ref = Some(crate::executor::SessionRef {
            provider: "qoder".into(),
            protocol: "acp-v1".into(),
            id: "old".into(),
            cwd: Some("/tmp/original".into()),
            metadata: serde_json::Value::Null,
            extra: Default::default(),
        });
        task.supplements.push(crate::model::Supplement {
            source_thread_id: None,
            run_id: task.run_id.clone().unwrap(),
            previous_turn_id: "previous".into(),
            text: "follow up".into(),
            created_at: 0,
        });
        let mut state = AcpSession::new(true);
        assert!(state.receive(&mut task,&json!({"id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":false}}}),std::path::Path::new("/tmp/original")).is_err());
        let mut state = AcpSession::new(true);
        let requests=state.receive(&mut task,&json!({"id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}}),std::path::Path::new("/tmp/original")).unwrap();
        assert_eq!(requests[0]["method"], "session/load");
        assert_eq!(requests[0]["params"]["sessionId"], "old");
        assert_eq!(requests[0]["params"]["cwd"], "/tmp/original");
        state.receive(&mut task,&json!({"method":"session/update","params":{"sessionId":"old","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"OLD DELIVERY"}}}}),std::path::Path::new("/tmp/original")).unwrap();
        let requests = state
            .receive(
                &mut task,
                &json!({"id":2,"result":{}}),
                std::path::Path::new("/tmp/original"),
            )
            .unwrap();
        assert_eq!(requests[0]["params"]["prompt"][0]["text"], crate::delivery::instruction("follow up"));
        assert!(task.nodes[0].output.is_empty());
    }
    #[test]
    fn acp_permissions_and_cancel_are_protocol_confirmed() {
        let mut task = task();
        let mut state = AcpSession::new(false);
        state
            .receive(
                &mut task,
                &json!({"id":1,"result":{"protocolVersion":2}}),
                std::path::Path::new("/tmp"),
            )
            .unwrap_err();
        let replies = deny_permission(
            &json!({"id":"approval","params":{"options":[{"optionId":"no","kind":"reject_once"},{"optionId":"yes","kind":"allow_once"}]}}),
        );
        assert_eq!(replies["result"]["outcome"]["optionId"], "no");
        let replies = deny_permission(
            &json!({"id":10,"params":{"options":[{"optionId":"permanent","kind":"reject_always"},{"optionId":"once","kind":"reject_once"}]}}),
        );
        assert_eq!(replies["result"]["outcome"]["optionId"], "once");
        let replies = deny_permission(
            &json!({"id":11,"params":{"options":[{"optionId":"permanent","kind":"reject_always"}]}}),
        );
        assert_eq!(replies["result"]["outcome"]["outcome"], "cancelled");
        task.session_ref = Some(crate::executor::SessionRef {
            provider: "qoder".into(),
            protocol: "acp-v1".into(),
            id: "s".into(),
            cwd: None,
            metadata: serde_json::Value::Null,
            extra: Default::default(),
        });
        state.phase = Phase::Prompt;
        task.root_node();
        task.status = "cancelling".into();
        state
            .receive(
                &mut task,
                &json!({"id":3,"result":{"stopReason":"cancelled"}}),
                std::path::Path::new("/tmp"),
            )
            .unwrap();
        assert_eq!(task.status, "interrupted");
        assert!(task.artifacts.is_empty());
        state.phase = Phase::Prompt;
        task.status = "cancelling".into();
        let replies=state.receive(&mut task,&json!({"id":44,"method":"session/request_permission","params":{"sessionId":"s","options":[{"optionId":"deny","kind":"reject_once"}]}}),std::path::Path::new("/tmp")).unwrap();
        assert_eq!(replies[0]["result"]["outcome"]["outcome"], "cancelled");
        for reason in ["max_tokens", "max_turn_requests", "refusal"] {
            let mut task = super::tests::task();
            let mut state = AcpSession::new(false);
            state.phase = Phase::Prompt;
            state
                .receive(
                    &mut task,
                    &json!({"id":3,"result":{"stopReason":reason}}),
                    std::path::Path::new("/tmp"),
                )
                .unwrap();
            assert_eq!(task.status, "failed");
            assert!(task.events.last().unwrap().text.contains(reason));
        }
    }
    #[test]
    fn acp_file_reads_reject_escape_symlink_and_large_content() {
        let root = std::env::temp_dir().join(format!("orbit-acp-read-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("read.md"), "one\ntwo\nthree").unwrap();
        assert_eq!(
            read_text(
                &root,
                &json!({"path":root.join("read.md"),"line":2,"limit":1})
            )
            .unwrap(),
            "two"
        );
        assert!(read_text(&root, &json!({"path":"/etc/hosts"})).is_err());
        std::fs::write(root.join("large"), vec![b'x'; 300000]).unwrap();
        assert!(read_text(&root, &json!({"path":root.join("large")})).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hosts", root.join("escape")).unwrap();
            assert!(read_text(&root, &json!({"path":root.join("escape")})).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn replaced_root_cannot_redirect_file_reads() {
        let base = std::env::temp_dir().join(format!("orbit-acp-root-{}", uuid::Uuid::new_v4()));
        let root = base.join("authorized");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("text.md"), "INTERNAL").unwrap();
        std::fs::write(outside.join("text.md"), "EXTERNAL").unwrap();
        assert_eq!(
            read_text(&root, &json!({"path":root.join("text.md")})).unwrap(),
            "INTERNAL"
        );
        std::fs::write(root.join(".orbit-task"), "task").unwrap();
        let files = TaskFiles::for_task(&root, "task").unwrap();
        std::fs::rename(&root, base.join("original")).unwrap();
        std::os::unix::fs::symlink(&outside, &root).unwrap();
        let result = read_text(&root, &json!({"path":root.join("text.md")}));
        assert_eq!(
            files
                .read_text(&json!({"path":root.join("text.md")}))
                .unwrap(),
            "INTERNAL"
        );
        assert!(files
            .read_text(&json!({"path":outside.join("text.md")}))
            .is_err());
        std::fs::remove_dir_all(base).unwrap();
        assert!(result.is_err(), "replaced root read: {result:?}");
    }
    #[test]
    #[cfg(unix)]
    fn cancellation_cleanup_survives_failed_and_superseded_publication() {
        use std::os::unix::process::CommandExt;
        for stale in [false, true] {
            let dir =
                std::env::temp_dir().join(format!("orbit-acp-cancel-{}", uuid::Uuid::new_v4()));
            let store = Store::open(dir.clone()).unwrap();
            let mut snapshot = task();
            store.save_task(snapshot.clone()).unwrap();
            snapshot.status = "cancelling".into();
            snapshot.event("cancel queued", "system", "workbench");
            let moved = dir.with_extension("original");
            if stale {
                let mut newer = snapshot.clone();
                newer.event("reader output", "output", "Qoder");
                store.save_existing_task(newer).unwrap().unwrap();
            } else {
                std::fs::rename(&dir, &moved).unwrap();
                std::fs::write(&dir, "blocked directory").unwrap();
            }
            let child = Command::new("/bin/sh")
                .args(["-c", "sleep 20"])
                .stdin(Stdio::piped())
                .process_group(0)
                .spawn()
                .unwrap();
            let owned = Arc::new(Mutex::new(OwnedChild::new(child)));
            let timeout_child = owned.clone();
            let (done, wait) = mpsc::channel();
            let result = publish_cancellation(
                Duration::from_millis(30),
                || {
                    store
                        .save_existing_task(snapshot)
                        .and_then(|saved| saved.map(|_| ()).ok_or("superseded".into()))
                },
                move || {
                    let _ = done.send(timeout_child.lock().unwrap().stop());
                },
            );
            assert!(result.is_err());
            let stopped = wait.recv_timeout(Duration::from_secs(2));
            owned.lock().unwrap().stop().unwrap();
            if stale {
                std::fs::remove_dir_all(&dir).unwrap();
            } else {
                std::fs::remove_file(&dir).unwrap();
                std::fs::remove_dir_all(&moved).unwrap();
            }
            assert!(
                matches!(stopped, Ok(Ok(()))),
                "cancel cleanup not armed for stale={stale}: {stopped:?}"
            );
        }
    }
    #[test]
    #[cfg(unix)]
    fn fifo_is_rejected_without_waiting_for_a_writer() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let root = std::env::temp_dir().join(format!("orbit-acp-fifo-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let fifo = root.join("fifo");
        let path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        let now = std::time::Instant::now();
        assert!(read_text(&root, &json!({"path":fifo})).is_err());
        assert!(now.elapsed() < Duration::from_secs(1));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn blocked_writer_can_be_stopped_independently_and_queue_is_bounded() {
        use std::os::unix::process::CommandExt;
        let child = Command::new("/bin/sh")
            .args(["-c", "sleep 20"])
            .stdin(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut owned = OwnedChild::new(child);
        let input = owned.child.stdin.take().unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let closed = Arc::new(AtomicBool::new(false));
        let marker = closed.clone();
        let (done, wait) = mpsc::channel();
        let writer = thread::spawn(move || {
            let _ = done.send(write_messages(input, rx, marker));
        });
        queue(&tx, json!({"payload":"x".repeat(300_000)})).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert!(matches!(wait.try_recv(), Err(mpsc::TryRecvError::Empty)));
        closed.store(true, Ordering::Release);
        owned.stop().unwrap();
        assert!(wait.recv_timeout(Duration::from_secs(2)).is_ok());
        writer.join().unwrap();
        let (tx, _rx) = mpsc::sync_channel(1);
        assert!(queue(&tx, json!({})).is_ok());
        assert!(queue(&tx, json!({})).is_err());
    }
    #[test]
    fn restore_directory_requires_current_task_owner() {
        let dir = std::env::temp_dir().join(format!("orbit-acp-owner-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(dir.clone()).unwrap());
        let executor = QoderExecutor::new(store);
        let mut task = task();
        let cwd = executor.working_directory(&task, false).unwrap();
        task.session_ref = Some(SessionRef {
            provider: "qoder".into(),
            protocol: "acp-v1".into(),
            id: "s".into(),
            cwd: Some(cwd.to_string_lossy().into()),
            metadata: json!({"taskId":task.id}),
            extra: Default::default(),
        });
        assert_eq!(executor.working_directory(&task, true).unwrap(), cwd);
        std::fs::write(cwd.join(".orbit-task"), "other-task").unwrap();
        assert!(executor.working_directory(&task, true).is_err());
        std::fs::remove_file(cwd.join(".orbit-task")).unwrap();
        assert!(executor.working_directory(&task, true).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn acp_stdio_fixture_delivers_two_versions_without_replay() {
        use std::os::unix::process::CommandExt;
        let dir =
            std::env::temp_dir().join(format!("orbit-acp-transport-{}", uuid::Uuid::new_v4()));
        let store = Store::open(dir.clone()).unwrap();
        let cwd = dir.canonicalize().unwrap();
        let mut task = task();
        store.save_task(task.clone()).unwrap();
        for round in 0..2 {
            if round == 1 {
                task = store
                    .continue_task(
                        &task.id,
                        task.revision,
                        &task.run_id,
                        &task.turn_id,
                        "follow up",
                    )
                    .unwrap()
                    .0;
            }
            let mut child = OwnedChild::new(
                Command::new("python3")
                    .arg(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/fixtures/qoder-acp.py"
                    ))
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .process_group(0)
                    .spawn()
                    .unwrap(),
            );
            let mut input = child.child.stdin.take().unwrap();
            let stdout = child.child.stdout.take().unwrap();
            let (tx, rx) = mpsc::sync_channel(32);
            let reader = thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                while let Ok(Some(msg)) = read_message(&mut reader) {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            });
            send(&mut input, &initialize()).unwrap();
            let mut state = AcpSession::new(round == 1);
            while state.phase != Phase::Ended {
                let message = rx.recv_timeout(Duration::from_secs(3)).unwrap();
                let before = task.revision;
                let requests = state.receive(&mut task, &message, &cwd).unwrap();
                if before != task.revision {
                    let saved=store.save_existing_task(task.clone()).unwrap().unwrap();task.merge_platform(&saved);
                }
                for request in requests {
                    send(&mut input, &request).unwrap();
                }
            }
            child.stop().unwrap();
            drop(rx);
            reader.join().unwrap();
            task=store.task(&task.id).unwrap();
        }
        assert_eq!(task.artifacts.len(), 2);
        assert_eq!(task.artifacts[0].content, "FIRST");
        assert_eq!(task.artifacts[1].content, "SECOND");
        assert_eq!(
            Store::open(dir.clone())
                .unwrap()
                .task(&task.id)
                .unwrap()
                .artifacts
                .len(),
            2
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}


fn model_catalog() -> Result<Vec<crate::executor::ExecutorModel>, String> {
    #[cfg(unix)] { crate::executor::qoder_model_catalog(&capture_model_names(&cli_path(), Duration::from_secs(20))?) }
    #[cfg(not(unix))] { Err("此平台尚未开放 Qoder 模型目录".into()) }
}

#[cfg(unix)]
fn capture_model_names(path: &Path, timeout: Duration) -> Result<String, String> {
    use std::{io::BufRead, os::unix::process::CommandExt};
    let marker = format!("ORBIT_EXIT_{}", uuid::Uuid::new_v4());
    // Keep the group leader alive until cleanup, including after the CLI exits.
    // CLI arguments are separate argv, never interpolated into this fixed script.
    let mut command = Command::new("/bin/sh");
    command.args(["-c", r#"marker=$1; shift; "$@" </dev/null; code=$?; printf '\n%s:%s\n' "$marker" "$code"; IFS= read -r hold"#, "orbit-model-query"])
        .arg(&marker).arg(path).arg("--list-models")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let mut child = OwnedChild::new(command.spawn().map_err(|_| "无法启动 Qoder CLI，请先检查连接")?);
    let stdout = child.child.stdout.take().unwrap();
    let mut stderr = child.child.stderr.take().unwrap();
    thread::spawn(move || { let _ = std::io::copy(&mut stderr, &mut std::io::sink()); });
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout).take(65537);
        let result = (|| {
            let mut output = String::new();
            loop {
                let mut line = String::new();
                let count = reader.read_line(&mut line).map_err(|_| "无法读取 Qoder 模型目录")?;
                if count == 0 { return Err("Qoder 模型目录未返回退出状态".into()); }
                if let Some(code) = line.strip_prefix(&format!("{marker}:")).and_then(|s|s.strip_suffix('\n')) {
                    let status = code.parse::<u8>().map_err(|_| "Qoder 返回无效退出状态")?;
                    return if status == 0 { Ok(output) } else { Err(format!("Qoder 模型目录查询失败（退出码 {status}），请检查 CLI 登录状态")) };
                }
                output.push_str(&line);
                if output.len() > 65536 { return Err("Qoder 模型目录超过大小限制".into()); }
            }
        })();
        let _ = tx.send(result);
    });
    let result = rx.recv_timeout(timeout).map_err(|_| "Qoder 模型目录查询超时".to_string()).and_then(|r|r);
    child.stop()?;
    result
}


#[cfg(test)]
mod model_selection_tests {
    use super::*;
    #[test]
    fn selected_model_is_a_single_acp_argument() {
        let command = command_at(Path::new("qoder"), Some("Qwen3.8-Max"));
        assert_eq!(command.get_args().map(|v| v.to_str().unwrap()).collect::<Vec<_>>(), vec!["--acp", "--model", "Qwen3.8-Max"]);
        assert_eq!(command_at(Path::new("qoder"), None).get_args().count(),1);
    }
    #[test]
    #[ignore = "explicit local catalog inspection; no session or inference"]
    fn local_executor_catalogs_without_inference() {
        let qoder = model_catalog().unwrap();
        assert!(!qoder.is_empty());
        let codex = crate::runner::model_catalog().unwrap();
        assert!(!codex.is_empty());
        println!("Read actual model catalogs: Codex {}, Qoder {}",codex.len(),qoder.len());
    }
}


#[cfg(all(test, unix))]
mod catalog_supervisor_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn catalog_command_checks_exit_timeout_and_cleans_descendants() {
        let dir=std::env::temp_dir().join(format!("orbit-catalog-process-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path=dir.join("cli");
        let write=|script:&str| {std::fs::write(&path,format!("#!/bin/sh\n{script}\n")).unwrap();std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o700)).unwrap();};
        write("printf 'MODEL\\nQwen-test\\n'");
        assert!(capture_model_names(&path,Duration::from_secs(2)).unwrap().contains("Qwen-test"));
        write("printf 'MODEL\\nQwen-test\\n'; exit 7");
        assert!(capture_model_names(&path,Duration::from_secs(2)).unwrap_err().contains("退出码 7"));
        write("sleep 60");
        assert!(capture_model_names(&path,Duration::from_millis(50)).unwrap_err().contains("超时"));
        write(&format!("sleep 60 & echo $! > '{}'; printf 'MODEL\\nQwen-test\\n'",dir.join("pid").display()));
        capture_model_names(&path,Duration::from_secs(2)).unwrap();
        let pid=std::fs::read_to_string(dir.join("pid")).unwrap();
        let state=Command::new("/bin/ps").args(["-o","stat=","-p",pid.trim()]).output().unwrap();
        let state=String::from_utf8_lossy(&state.stdout);
        assert!(state.trim().is_empty()||state.trim().starts_with('Z'),"descendant still alive: {state}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[cfg(all(test,unix))]
mod shutdown_handoff_test{
 use super::*;use std::os::unix::process::CommandExt;
 #[test]fn closing_waits_for_spawn_registration_and_refuses_later_starts(){
  let dir=std::env::temp_dir().join(format!("orbit-qoder-shutdown-{}",uuid::Uuid::new_v4()));let r=QoderExecutor::new(Arc::new(Store::open(dir.clone()).unwrap()));let gate=r.begin_start().unwrap();let child=OwnedChild::new(Command::new("/bin/sh").args(["-c","sleep 30"]).process_group(0).spawn().unwrap());let task=Task::new("test".into(),"goal".into(),"research".into());let id=task.run_id.clone().unwrap();let(tx,_rx)=mpsc::sync_channel(1);let run=Arc::new(Mutex::new(Run{task,child,input:tx,writer_closed:Arc::new(AtomicBool::new(false)),state:AcpSession::new(false),closed:false}));r.runs.lock().unwrap().insert(id,run.clone());let(done,finished)=mpsc::channel();let other=r.clone();let worker=thread::spawn(move||{other.shutdown();done.send(()).unwrap();});assert!(finished.recv_timeout(Duration::from_millis(50)).is_err());drop(gate);finished.recv_timeout(Duration::from_secs(5)).unwrap();worker.join().unwrap();assert!(run.lock().unwrap().child.stopped);assert!(r.begin_start().is_err());std::fs::remove_dir_all(dir).unwrap();
 }
}
