use crate::executor::{Capabilities, Descriptor, Doctor, Executor};
use crate::process::{send, stop_child, OwnedChild};
use crate::{
    model::{now, parse_input_questions, Approval, Task},
    protocol::{
        error_message, observed_agent_nodes, project, read_message, resume_request, select_model,
        thread_request, turn_request, validate_resume, AgentHistory, AgentRead, ModelChoice,
    },
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufReader, Read},
    path::PathBuf,
    process::{ChildStdin, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

#[derive(PartialEq)]
enum Startup {
    Connecting,
    ReadingSource,
    Forking,
    AwaitingTurn,
    Running,
}

struct Run {
    task: Task,
    child: OwnedChild,
    input: ChildStdin,
    pending: HashMap<String, Value>,
    controls: HashMap<u64, String>,
    next_id: u64,
    model_cursors: Vec<String>,
    requested_model: Option<String>,
    fork_anchor: Option<Value>,
    closed: bool,
    last_emit: Instant,
    resume_turn: Option<String>,
    startup: Startup,
    buffered: Vec<Value>,
    buffered_bytes: usize,
}

impl Run {
    fn initialized_model_request(&mut self, directory: &std::path::Path) -> Value {
        if let Some(model) = self.task.requested_model.clone() {
            self.task.event(&format!("本次请求模型：{model}"), "system", "工作台");
            self.thread_request(directory, &model)
        } else {
            json!({"id":4,"method":"model/list","params":{"limit":100}})
        }
    }
    fn thread_request(&mut self, directory: &std::path::Path, model: &str) -> Value {
        self.requested_model = Some(model.into());
        match &self.resume_turn {
            Some(_) => resume_request(self.task.thread_id.as_deref().unwrap(), model),
            None => thread_request(directory, model),
        }
    }
    fn recover_writer(&mut self, id: u64, detail: &str) -> Option<Value> {
        let source = self.task.thread_id.as_deref()?;
        if id != 2
            || self.startup != Startup::Connecting
            || self.closed
            || self.resume_turn.is_none()
            || self.requested_model.is_none()
            || detail != format!("thread {source} already has an active writer")
        {
            return None;
        }
        let request =
            json!({"id":5,"method":"thread/read","params":{"threadId":source,"includeTurns":true}});
        self.startup = Startup::ReadingSource;
        self.task.event(
            "原会话被其他执行器占用，正在核对已完成的历史",
            "system",
            "工作台",
        );
        Some(request)
    }
    fn request_fork(&mut self, result: &Value) -> Result<Value, String> {
        if self.startup != Startup::ReadingSource {
            return Err("会话历史响应阶段不匹配".into());
        }
        let source = self.task.thread_id.as_deref().ok_or("原会话标识缺失")?;
        let anchor = self.resume_turn.as_deref().ok_or("原会话轮次缺失")?;
        let mut thread = result["thread"].clone();
        if !matches!(
            thread["status"]["type"].as_str(),
            Some("idle" | "notLoaded")
        ) {
            return Err("原会话仍在执行，已停止复制历史；请等待它结束".into());
        }
        thread["status"]["type"] = json!("idle");
        validate_resume(&thread, source, anchor)?;
        let mut request = resume_request(
            source,
            self.requested_model.as_deref().ok_or("请求模型缺失")?,
        );
        request["id"] = json!(6);
        request["method"] = json!("thread/fork");
        request["params"]["lastTurnId"] = json!(anchor);
        request["params"]["deferGoalContinuation"] = json!(true);
        self.fork_anchor = thread["turns"]
            .as_array()
            .and_then(|turns| turns.last())
            .cloned();
        self.startup = Startup::Forking;
        Ok(request)
    }
    fn prepare_turn(&self, result: &Value, fork: bool) -> Result<(Task, Value), String> {
        let thread = &result["thread"];
        let id = thread["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 100)
            .ok_or("执行器未返回有效 thread ID")?;
        if let Some(anchor) = &self.resume_turn {
            let source = self.task.thread_id.as_deref().ok_or("原会话标识缺失")?;
            if fork {
                if id == source
                    || thread["forkedFromId"] != source
                    || thread["turns"].as_array().and_then(|turns| turns.last())
                        != self.fork_anchor.as_ref()
                {
                    return Err("续接分支来源或历史不匹配，已停止发送补充".into());
                }
                validate_resume(thread, id, anchor)?;
            } else {
                validate_resume(thread, source, anchor)?;
            }
        }
        let mut task = self.task.clone();
        if fork {
            let source = task.thread_id.clone();
            task.supplements
                .last_mut()
                .ok_or("补充记录缺失")?
                .source_thread_id = source;
            task.event(
                "原会话被占用，已复制完成的历史创建续接分支；已有交付保留",
                "system",
                "工作台",
            );
        }
        task.thread_id = Some(id.into());
        task.root_node();
        task.nodes[0].model = result["model"].as_str().unwrap_or("实际模型未返回").into();
        let text = if self.resume_turn.is_some() {
            &task.supplements.last().ok_or("补充记录缺失")?.text
        } else {
            &task.prompt
        };
        let request = turn_request(id, text);
        task.event(
            if fork {
                "续接分支已保存，发送补充信息"
            } else if self.resume_turn.is_some() {
                "原会话已续接，发送补充信息"
            } else {
                "会话已创建，启动任务"
            },
            "system",
            "工作台",
        );
        Ok((task, request))
    }
    fn start_turn(&mut self, result: &Value) -> Result<Value, String> {
        if self.startup != Startup::Connecting || self.requested_model.is_none() {
            return Err("会话响应阶段不匹配".into());
        }
        let (task, request) = self.prepare_turn(result, false)?;
        self.task = task;
        self.startup = Startup::AwaitingTurn;
        Ok(request)
    }
    fn start_forked_turn(&mut self, result: &Value, store: &Store) -> Result<Value, String> {
        if self.startup != Startup::Forking {
            return Err("分支响应阶段不匹配".into());
        }
        let (candidate, request) = self.prepare_turn(result, true)?;
        let saved = store
            .save_existing_task(candidate)?
            .ok_or("任务记录已变化，未发送补充")?;
        self.task = saved.actor_snapshot();
        self.startup = Startup::AwaitingTurn;
        Ok(request)
    }
    fn accept_turn(&mut self, result: &Value) -> Result<Vec<Value>, String> {
        let id = result["turn"]["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("执行器未返回有效 turn ID")?;
        if self.startup != Startup::AwaitingTurn || self.resume_turn.as_deref() == Some(id) {
            return Err("新一轮标识无效，执行结果需核对".into());
        }
        self.task.turn_id = Some(id.into());
        self.task.revision += 1;
        self.startup = Startup::Running;
        self.buffered_bytes = 0;
        Ok(std::mem::take(&mut self.buffered))
    }
    fn defer_notification(&mut self, message: &Value) -> Result<bool, String> {
        if message.get("method").is_none() || self.startup == Startup::Running {
            return Ok(false);
        }
        if self.startup != Startup::AwaitingTurn {
            return Ok(true);
        }
        let bytes = message.to_string().len();
        if self.buffered.len() >= 128 || self.buffered_bytes + bytes > crate::protocol::MAX_MESSAGE
        {
            return Err("启动期间事件超过缓冲上限，执行结果需核对".into());
        }
        self.buffered_bytes += bytes;
        self.buffered.push(message.clone());
        Ok(true)
    }
    fn direction_request(&self, text: &str) -> Result<(Task, Value), String> {
        if text.trim().is_empty() || text.chars().count() > 2000 {
            return Err("补充方向应为 1–2000 字".into());
        }
        if self.closed
            || self.startup != Startup::Running
            || !matches!(self.task.status.as_str(), "running" | "approval")
        {
            return Err("运行未开始、已结束或正在中断".into());
        }
        let turn = self.task.turn_id.as_deref().ok_or("任务尚未开始")?;
        let thread = self.task.thread_id.as_deref().ok_or("会话未创建")?;
        if self.task.directions.len() >= 20 {
            return Err("已达到 20 条运行中补充上限，请等待交付后继续".into());
        }
        if self.task.directions.iter().any(|d| {
            Some(d.run_id.as_str()) == self.task.run_id.as_deref()
                && d.turn_id == turn
                && d.text == text.trim()
                && matches!(d.status.as_str(), "pending" | "unknown")
        }) {
            return Err("相同补充仍待确认，请先核对当前交付，勿重复发送".into());
        }
        let mut task = self.task.clone();
        task.directions.push(crate::model::Direction {
            id: format!("{}:{}", task.run_id.as_deref().unwrap(), self.next_id),
            run_id: task.run_id.clone().unwrap(),
            turn_id: turn.into(),
            text: text.trim().into(),
            status: "pending".into(),
            created_at: now(),
        });
        task.event("补充正文已保存，等待执行器确认", "input", "你");
        let input = format!(
            "请结合本轮原始目标处理以下补充，并在最终交付中体现：\n{}",
            text.trim()
        );
        Ok((
            task,
            json!({"id":self.next_id,"method":"turn/steer","params":{"threadId":thread,"expectedTurnId":turn,"input":[{"type":"text","text":input}]}}),
        ))
    }
    fn confirm_control(&mut self, id: u64, result: &Value) -> bool {
        let Some(kind) = self.controls.remove(&id) else {
            return false;
        };
        let key = format!("{}:{}", self.task.run_id.as_deref().unwrap_or_default(), id);
        if let Some(direction) = self.task.directions.iter_mut().find(|d| d.id == key) {
            let accepted = Some(direction.run_id.as_str()) == self.task.run_id.as_deref()
                && Some(direction.turn_id.as_str()) == self.task.turn_id.as_deref()
                && result["turnId"].as_str() == Some(direction.turn_id.as_str());
            direction.status = if accepted { "accepted" } else { "unknown" }.into();
            self.task.event(
                if accepted {
                    "执行器已确认本轮补充；请在交付中核对采用情况"
                } else {
                    "补充确认轮次不匹配，请核对交付，勿重复发送"
                },
                "system",
                "工作台",
            );
        } else {
            self.task.event(
                &format!("{}请求已接收；执行结果以状态事件为准", kind),
                "system",
                "工作台",
            );
        }
        true
    }
    fn idle(&self) -> bool {
        self.closed && self.child.stopped
    }
    fn reject_control(&mut self, id: u64) -> bool {
        let Some(label) = self.controls.remove(&id) else {
            return false;
        };
        let key = format!("{}:{}", self.task.run_id.as_deref().unwrap_or_default(), id);
        if let Some(direction) = self.task.directions.iter_mut().find(|d| d.id == key) {
            direction.status = "rejected".into();
        }
        self.task.event(
            &format!("{}请求被执行器拒绝；继续等待任务状态", label),
            "error",
            "工作台",
        );
        true
    }
    fn answer(
        &mut self,
        run_id: &str,
        approval_id: &str,
        answers: HashMap<String, String>,
    ) -> Result<Value, String> {
        if self.closed || self.task.status != "approval" {
            return Err("此请求已失效".into());
        }
        let a = self
            .task
            .approvals
            .iter()
            .find(|a| {
                a.id == approval_id
                    && a.run_id == run_id
                    && Some(a.turn_id.as_str()) == self.task.turn_id.as_deref()
            })
            .cloned()
            .ok_or("此审批已失效或已处理")?;
        let request = self
            .pending
            .get(approval_id)
            .cloned()
            .ok_or("请求已被处理")?;
        if request["id"].to_string() != a.request_id {
            return Err("请求 ID 不匹配".into());
        }
        if a.question_error.is_some() {
            return Err("澄清请求格式无效，不能提交答复".into());
        }
        if answers.len() != a.question_ids.len()
            || a.question_ids.iter().any(|key| {
                answers
                    .get(key)
                    .is_none_or(|answer| (if a.questions.iter().any(|q| &q.id == key && q.is_secret) { answer.is_empty() } else { answer.trim().is_empty() }) || answer.chars().count() > 2000)
            })
        {
            return Err("请为每个问题填写 1–2000 字的答复".into());
        }
        self.pending.remove(approval_id);
        self.task.approvals.retain(|item| item.id != approval_id);
        let answers: serde_json::Map<String, Value> = answers
            .into_iter()
            .map(|(key, answer)| {
                let preserve = a.questions.iter().any(|q| q.id == key && (q.is_secret || q.options.as_ref().is_some_and(|options| options.iter().any(|o| o.label == answer))));
                (key, json!({"answers":[if preserve {answer} else {answer.trim().to_owned()}]}))
            })
            .collect();
        Ok(json!({"id":request["id"],"result":{"answers":answers}}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    fn group_fixture(leader_exits: bool) -> (OwnedChild, i32) {
        use std::io::BufRead;
        use std::os::unix::process::CommandExt;
        let script = if leader_exits {
            "sh -c 'trap \"\" TERM; exec sleep 30' & echo $!; exit 0"
        } else {
            "trap '' TERM; sh -c 'trap \"\" TERM; exec sleep 30' & echo $!; wait"
        };
        let mut child = OwnedChild::new(
            Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
                .stdout(Stdio::piped())
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        let mut line = String::new();
        BufReader::new(child.child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        thread::sleep(Duration::from_millis(100));
        (child, line.trim().parse().unwrap())
    }
    #[cfg(unix)]
    fn descendant_running(pid: i32) -> bool {
        let output = Command::new("/bin/ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&output.stdout);
        !state.trim().is_empty() && !state.trim().starts_with('Z')
    }
    #[cfg(unix)]
    fn check_group_cleanup(leader_exits: bool) {
        let (mut child, descendant) = group_fixture(leader_exits);
        stop_child(&mut child);
        let deadline = Instant::now() + Duration::from_secs(2);
        while descendant_running(descendant) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let leaked = descendant_running(descendant);
        assert!(
            !leaked,
            "owned descendant must be stopped, including after leader exit"
        );
    }
    #[test]
    #[ignore = "explicit two-turn inference check using own test thread, no tools or user task"]
    fn local_continuation_with_real_cli() {
        check_real_continuation(false);
    }
    #[test]
    #[ignore = "explicit occupied-writer continuation on own no-tool test session"]
    fn local_occupied_writer_continuation_with_real_cli() {
        check_real_continuation(true);
    }
    fn check_real_continuation(occupied: bool) {
        let mut retained_writer = None;
        let mut retained_thread = None;
        let directory =
            std::env::temp_dir().join(format!("orbit-real-continuation-{}", uuid::Uuid::new_v4()));
        let store = Store::open(directory.clone()).unwrap();
        let mut task = Task::new(
            "Continuation verification".into(),
            "Reply exactly FIRST_OK. Do not use tools or spawn agents.".into(),
            "writing".into(),
        );
        for round in 0..2 {
            let resume_turn = if round == 1 {
                let original_thread = task.thread_id.clone();
                let original_run = task.run_id.clone();
                let (next, anchor) = store
                    .continue_task(
                        &task.id,
                        task.revision,
                        &task.run_id,
                        &task.turn_id,
                        "Reply exactly SECOND_OK. Do not use tools or spawn agents.",
                    )
                    .unwrap();
                assert_eq!(next.thread_id, original_thread);
                assert_ne!(next.run_id, original_run);
                task = next;
                Some(anchor)
            } else {
                store.save_task(task.clone()).unwrap();
                None
            };
            let mut child = OwnedChild::new(command().current_dir(&directory).spawn().unwrap());
            let input = child.child.stdin.take().unwrap();
            let output = child.child.stdout.take().unwrap();
            let mut stderr = child.child.stderr.take().unwrap();
            thread::spawn(move || {
                let _ = std::io::copy(&mut stderr, &mut std::io::sink());
            });
            let (tx, rx) = mpsc::sync_channel(128);
            thread::spawn(move || {
                let mut reader = BufReader::new(output);
                loop {
                    let result = read_message(&mut reader);
                    let done = !matches!(&result, Ok(Some(_)));
                    if tx.send(result).is_err() || done {
                        break;
                    }
                }
            });
            let mut run = Run {
                task,
                child,
                input,
                pending: HashMap::new(),
                controls: HashMap::new(),
                next_id: 100,
                model_cursors: Vec::new(),
                requested_model: None,
                fork_anchor: None,
                closed: false,
                last_emit: Instant::now(),
                resume_turn,
                startup: Startup::Connecting,
                buffered: Vec::new(),
                buffered_bytes: 0,
            };
            send(&mut run.input, &initialize()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(60);
            while !run.task.terminal() {
                let message = rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("real continuation timeout")
                    .unwrap()
                    .expect("CLI closed before completion");
                if message.get("error").is_some() {
                    let detail = error_message(&message["error"]);
                    let request = run
                        .recover_writer(message["id"].as_u64().unwrap(), &detail)
                        .unwrap_or_else(|| panic!("{}", message));
                    send(&mut run.input, &request).unwrap();
                    continue;
                }
                if run.defer_notification(&message).unwrap() {
                    continue;
                }
                if message.get("method").is_some() && message.get("id").is_some() {
                    send(&mut run.input, &json!({"id":message["id"],"error":{"code":-32601,"message":"No tools in continuation verification"}})).unwrap();
                } else if let Some(id) = message["id"].as_u64() {
                    let request = match id {
                        1 => {
                            send(&mut run.input, &json!({"method":"initialized"})).unwrap();
                            Some(json!({"id":4,"method":"model/list","params":{"limit":100}}))
                        }
                        4 => {
                            match select_model(&message["result"], &mut run.model_cursors).unwrap()
                            {
                                ModelChoice::Selected(model) => {
                                    Some(run.thread_request(&directory, &model))
                                }
                                ModelChoice::NextPage(cursor) => Some(
                                    json!({"id":4,"method":"model/list","params":{"limit":100,"cursor":cursor}}),
                                ),
                            }
                        }
                        2 => Some(run.start_turn(&message["result"]).unwrap()),
                        5 => Some(run.request_fork(&message["result"]).unwrap()),
                        6 => Some(run.start_forked_turn(&message["result"], &store).unwrap()),
                        3 => {
                            for deferred in run.accept_turn(&message["result"]).unwrap() {
                                project(&mut run.task, &deferred);
                            }
                            None
                        }
                        _ => None,
                    };
                    if let Some(request) = request {
                        send(&mut run.input, &request).unwrap();
                    }
                } else {
                    project(&mut run.task, &message);
                }
            }
            assert_eq!(run.task.status, "completed");
            let expected = if round == 0 { "FIRST_OK" } else { "SECOND_OK" };
            assert_eq!(run.task.artifacts.last().unwrap().content.trim(), expected);
            if occupied && round == 0 {
                retained_thread = run.task.thread_id.clone();
                retained_writer = Some((run.child, run.input));
            } else {
                run.child.stop().unwrap();
            }
            task = run.task;
            store.save_existing_task(task.clone()).unwrap().unwrap();
        }
        if occupied {
            assert_eq!(
                task.supplements.last().unwrap().source_thread_id,
                retained_thread
            );
            assert_ne!(task.thread_id, retained_thread);
            retained_writer.as_mut().unwrap().0.stop().unwrap();
        }
        assert_eq!(task.artifacts.len(), 2);
        assert_eq!(task.artifacts[0].content.trim(), "FIRST_OK");
        assert_ne!(task.artifacts[0].id, task.artifacts[1].id);
        assert_ne!(
            task.turn_id.as_deref(),
            Some(task.supplements[0].previous_turn_id.as_str())
        );
        std::fs::remove_dir_all(directory).unwrap();
        println!(
            "Confirmed actual CLI: {} completed; FIRST_OK and SECOND_OK retained.",
            if occupied {
                "occupied source forked through its pinned history"
            } else {
                "original thread resumed"
            }
        );
    }
    #[test]
    fn startup_buffer_overflow_is_explicit_and_pending_directions_recover_unknown() {
        let mut run = fake_run();
        run.startup = Startup::AwaitingTurn;
        for _ in 0..128 {
            assert!(run
                .defer_notification(&json!({"method":"turn/started","params":{}}))
                .unwrap());
        }
        assert!(run
            .defer_notification(&json!({"method":"turn/started","params":{}}))
            .is_err());
        run.startup = Startup::Running;
        let (mut candidate, _) = run.direction_request("pending direction").unwrap();
        candidate.recover();
        assert_eq!(candidate.directions[0].status, "unknown");
    }
    #[test]
    fn steering_retains_text_and_confirms_only_the_bound_turn() {
        let mut run = fake_run();
        let (candidate, request) = run.direction_request("more storage detail").unwrap();
        assert_eq!(request["method"], "turn/steer");
        assert_eq!(request["params"]["expectedTurnId"], "turn");
        assert_eq!(candidate.directions[0].text, "more storage detail");
        run.task = candidate;
        run.controls.insert(100, "补充方向".into());
        assert!(run.direction_request("more storage detail").is_err());
        run.confirm_control(100, &json!({"turnId":"foreign-turn"}));
        assert_eq!(run.task.directions[0].status, "unknown");
        assert!(!run.task.events.last().unwrap().text.contains("已采用"));
        run.next_id = 101;
        let (candidate, _) = run.direction_request("a different direction").unwrap();
        run.task = candidate;
        run.controls.insert(100, "补充方向".into());
        run.controls.remove(&100);
        run.controls.insert(101, "补充方向".into());
        run.confirm_control(101, &json!({"turnId":"turn"}));
        assert_eq!(run.task.directions.last().unwrap().status, "accepted");
    }
    #[test]
    fn resume_isolates_old_notifications_and_replays_early_new_turn() {
        let mut run = fake_run();
        run.task.status = "completed".into();
        run.task.artifacts.push(crate::model::Artifact {
            id: "first".into(),
            name: "v1.md".into(),
            kind: "markdown".into(),
            content: "edited first".into(),
            created_at: 0,
        });
        let (next, anchor) = run
            .task
            .continued(
                run.task.revision,
                &run.task.run_id,
                &run.task.turn_id,
                "next question",
            )
            .unwrap();
        run.task = next;
        run.resume_turn = Some(anchor);
        run.startup = Startup::Connecting;
        let request = run.thread_request(std::path::Path::new("/tmp/new"), "default");
        assert_eq!(request["method"], "thread/resume");
        assert_eq!(request["params"]["threadId"], "thread");
        assert!(request["params"].get("cwd").is_none());
        assert_eq!(request["params"]["sandbox"], "read-only");
        let old =
            json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"turn"}}});
        assert!(run.defer_notification(&old).unwrap());
        assert!(run.buffered.is_empty());
        assert!(run.start_turn(&json!({"thread":{"id":"thread","status":{"type":"active"},"turns":[{"id":"turn","status":"completed"}]}})).is_err());
        assert!(run.start_turn(&json!({"thread":{"id":"thread","status":{"type":"idle"},"turns":[{"id":"newer","status":"failed"}]}})).is_err());
        let request=run.start_turn(&json!({"model":"default","thread":{"id":"thread","status":{"type":"idle"},"turns":[{"id":"turn","status":"completed"}]}})).unwrap();
        assert_eq!(request["params"]["input"][0]["text"], "next question");
        assert_eq!(request["params"]["sandboxPolicy"]["type"], "readOnly");
        assert!(run.defer_notification(&old).unwrap());
        let early = json!({"method":"item/completed","params":{"threadId":"thread","turnId":"second","item":{"id":"answer","type":"agentMessage","phase":"final_answer","text":"second result"}}});
        assert!(run.defer_notification(&early).unwrap());
        let buffered = run.accept_turn(&json!({"turn":{"id":"second"}})).unwrap();
        for message in buffered {
            project(&mut run.task, &message);
        }
        assert_eq!(run.task.turn_id.as_deref(), Some("second"));
        project(
            &mut run.task,
            &json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"second","status":"completed"}}}),
        );
        assert_eq!(run.task.artifacts.len(), 2);
        assert_eq!(run.task.artifacts[0].content, "edited first");
        assert_eq!(run.task.artifacts[1].content, "second result");
        assert_eq!(run.task.artifacts[1].name, "codex-result-v2.md");
    }
    #[test]
    #[cfg(unix)]
    fn cleanup_after_leader_exit() {
        check_group_cleanup(true);
    }
    #[test]
    #[cfg(unix)]
    fn cleanup_descendant_ignoring_term() {
        check_group_cleanup(false);
    }
    #[test]
    #[cfg(unix)]
    fn saved_document_survives_shutdown_and_open_runs_reject_edits() {
        let directory =
            std::env::temp_dir().join(format!("orbit-document-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(directory.clone()).unwrap());
        let mut run = fake_run();
        run.task.status = "completed".into();
        run.task.artifacts.push(crate::model::Artifact {
            id: "doc".into(),
            name: "doc.md".into(),
            kind: "markdown".into(),
            content: "original".into(),
            created_at: 0,
        });
        runtime.store.save_task(run.task.clone()).unwrap();
        let run = Arc::new(Mutex::new(run));
        runtime
            .runs
            .lock()
            .unwrap()
            .insert("run".into(), run.clone());
        assert!(runtime
            .edit_artifact("doc".into(), "original".into(), "edited".into())
            .is_err());
        {
            let mut r = run.lock().unwrap();
            r.child.stop().unwrap();
            r.closed = true;
        }
        runtime
            .edit_artifact("doc".into(), "original".into(), "edited".into())
            .unwrap();
        assert_eq!(run.lock().unwrap().task.artifacts[0].content, "edited");
        runtime.shutdown();
        assert_eq!(
            Store::open(directory.clone()).unwrap().workspace().tasks[0].artifacts[0].content,
            "edited"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn archive_and_delete_require_idle_runs_and_shutdown_does_not_resurrect() {
        let directory =
            std::env::temp_dir().join(format!("orbit-archive-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(directory.clone()).unwrap());
        let mut run = fake_run();
        run.task.status = "completed".into();
        run.task.artifacts.push(crate::model::Artifact {
            id: "doc".into(),
            name: "doc.md".into(),
            kind: "markdown".into(),
            content: "delivery".into(),
            created_at: 0,
        });
        let id = run.task.id.clone();
        runtime.store.save_task(run.task.clone()).unwrap();
        let run = Arc::new(Mutex::new(run));
        runtime
            .runs
            .lock()
            .unwrap()
            .insert("run".into(), run.clone());
        assert!(runtime.archive_task(id.clone()).unwrap_err().contains("清理尚未确认"));
        {
            let mut r = run.lock().unwrap();
            r.closed = true;
        }
        assert!(runtime.archive_task(id.clone()).is_err());
        {
            let mut r = run.lock().unwrap();
            r.child.stop().unwrap();
        }
        let current=runtime.store.task(&id).unwrap();
        runtime.store.accept_task(&id,current.revision,&current.run_id,&current.turn_id).unwrap();
        std::fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(runtime.archive_task(id.clone()).is_err());
        assert!(!run.lock().unwrap().task.archived);
        std::fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        let saved = runtime.archive_task(id.clone()).unwrap();
        assert!(saved.archived);
        assert!(run.lock().unwrap().task.archived);
        std::fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(runtime.delete_task(id.clone()).is_err());
        assert!(runtime.runs.lock().unwrap().contains_key("run"));
        std::fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        let mut late = run.lock().unwrap().task.clone();
        late.archived = false;
        late.revision += 50;
        assert!(runtime
            .store
            .save_existing_task(late.clone())
            .unwrap()
            .is_none());
        runtime.delete_task(id).unwrap();
        assert!(runtime.runs.lock().unwrap().is_empty());
        assert!(runtime.store.save_existing_task(late).unwrap().is_none());
        runtime.shutdown();
        assert!(Store::open(directory.clone())
            .unwrap()
            .workspace()
            .tasks
            .is_empty());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn task_model_is_sent_only_after_initialization_and_reused_for_resume() {
        let mut run=fake_run();
        run.task.requested_model=Some("chosen-model".into());
        assert!(run.requested_model.is_none());
        let start=run.initialized_model_request(std::path::Path::new("/tmp"));
        assert_eq!(start["method"],"thread/start");
        assert_eq!(start["params"]["model"],"chosen-model");
        assert_eq!(start["params"]["sandbox"],"read-only");
        run.task.thread_id=Some("source".into());run.resume_turn=Some("last-turn".into());run.requested_model=None;
        let resume=run.initialized_model_request(std::path::Path::new("/tmp"));
        assert_eq!(resume["method"],"thread/resume");
        assert_eq!(resume["params"]["model"],"chosen-model");
        assert_eq!(resume["params"]["threadId"],"source");
        run.task.requested_model=None;run.requested_model=None;
        assert_eq!(run.initialized_model_request(std::path::Path::new("/tmp"))["method"],"model/list");
    }
    fn fake_run() -> Run {
        use std::os::unix::process::CommandExt;
        let mut child = OwnedChild::new(
            Command::new("/bin/cat")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        let input = child.child.stdin.take().unwrap();
        let mut task = Task::new("test".into(), "test".into(), "research".into());
        task.run_id = Some("run".into());
        task.thread_id = Some("thread".into());
        task.turn_id = Some("turn".into());
        task.status = "running".into();
        Run {
            task,
            child,
            input,
            pending: HashMap::new(),
            controls: HashMap::new(),
            next_id: 100,
            model_cursors: Vec::new(),
            requested_model: None,
            fork_anchor: None,
            closed: false,
            last_emit: Instant::now(),
            resume_turn: None,
            startup: Startup::Running,
            buffered: Vec::new(),
            buffered_bytes: 0,
        }
    }
    #[test]
    #[cfg(unix)]
    fn occupied_writer_creates_one_pinned_durable_branch() {
        let directory =
            std::env::temp_dir().join(format!("orbit-writer-state-{}", uuid::Uuid::new_v4()));
        let store = Store::open(directory.clone()).unwrap();
        let mut run = fake_run();
        run.task.status = "completed".into();
        run.task.artifacts.push(crate::model::Artifact {
            id: "old".into(),
            name: "old.md".into(),
            kind: "markdown".into(),
            content: "edited first".into(),
            created_at: 0,
        });
        let (task, anchor) = run
            .task
            .continued(
                run.task.revision,
                &run.task.run_id,
                &run.task.turn_id,
                "more information",
            )
            .unwrap();
        run.task = task;
        run.resume_turn = Some(anchor);
        run.startup = Startup::Connecting;
        store.save_task(run.task.clone()).unwrap();
        run.thread_request(std::path::Path::new("/tmp"), "default");
        assert!(run
            .recover_writer(3, "thread thread already has an active writer")
            .is_none());
        assert!(run
            .recover_writer(2, "thread other already has an active writer")
            .is_none());
        let read = run
            .recover_writer(2, "thread thread already has an active writer")
            .unwrap();
        assert_eq!(read["method"], "thread/read");
        assert!(run
            .recover_writer(2, "thread thread already has an active writer")
            .is_none());
        let source = json!({"thread":{"id":"thread","status":{"type":"notLoaded"},"turns":[{"id":"turn","status":"completed","items":[]}]}});
        for bad in [
            json!({"thread":{"id":"foreign","status":{"type":"notLoaded"},"turns":[{"id":"turn","status":"completed"}]}}),
            json!({"thread":{"id":"thread","status":{"type":"active"},"turns":[{"id":"turn","status":"completed"}]}}),
            json!({"thread":{"id":"thread","status":{"type":"notLoaded"},"turns":[{"id":"changed","status":"completed"}]}}),
            json!({"thread":{"id":"thread","status":{"type":"notLoaded"},"turns":[{"id":"turn","status":"inProgress"}]}}),
        ] {
            assert!(run.request_fork(&bad).is_err());
        }
        let fork = run.request_fork(&source).unwrap();
        assert_eq!(fork["method"], "thread/fork");
        assert_eq!(fork["params"]["lastTurnId"], "turn");
        assert_eq!(fork["params"]["deferGoalContinuation"], true);
        assert_eq!(fork["params"]["sandbox"], "read-only");
        assert!(run.request_fork(&source).is_err());
        let branch = json!({"model":"default","thread":{"id":"branch","forkedFromId":"thread","status":{"type":"idle"},"turns":[{"id":"turn","status":"completed","items":[]}]}});
        for field in ["id", "forkedFromId"] {
            let mut bad = branch.clone();
            bad["thread"][field] = json!("thread");
            if field == "forkedFromId" {
                bad["thread"][field] = json!("foreign");
            }
            assert!(run.start_forked_turn(&bad, &store).is_err());
        }
        let mut bad = branch.clone();
        bad["thread"]["status"]["type"] = json!("notLoaded");
        assert!(run.start_forked_turn(&bad, &store).is_err());
        let mut bad = branch.clone();
        bad["thread"]["turns"][0]["items"] = json!([{"text":"changed"}]);
        assert!(run.start_forked_turn(&bad, &store).is_err());
        let empty = Store::open(directory.join("empty")).unwrap();
        assert!(run.start_forked_turn(&branch, &empty).is_err());
        std::fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(run.start_forked_turn(&branch, &store).is_err());
        assert_eq!(run.task.thread_id.as_deref(), Some("thread"));
        assert_eq!(
            store.task(&run.task.id).unwrap().thread_id.as_deref(),
            Some("thread")
        );
        std::fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        let request = run.start_forked_turn(&branch, &store).unwrap();
        assert_eq!(request["method"], "turn/start");
        assert_eq!(request["params"]["threadId"], "branch");
        let saved = Store::open(directory.clone())
            .unwrap()
            .task(&run.task.id)
            .unwrap();
        assert_eq!(saved.thread_id.as_deref(), Some("branch"));
        assert_eq!(
            saved
                .supplements
                .last()
                .unwrap()
                .source_thread_id
                .as_deref(),
            Some("thread")
        );
        assert_eq!(saved.artifacts[0].content, "edited first");
        assert!(run.start_forked_turn(&branch, &store).is_err());
        run.task.status = "completed".into();
        run.task.turn_id = Some("second-turn".into());
        let (next, anchor) = run
            .task
            .continued(
                run.task.revision,
                &run.task.run_id,
                &run.task.turn_id,
                "second branch",
            )
            .unwrap();
        run.task = next;
        run.resume_turn = Some(anchor);
        run.startup = Startup::Connecting;
        store.save_task(run.task.clone()).unwrap();
        run.thread_request(std::path::Path::new("/tmp"), "default");
        run.recover_writer(2, "thread branch already has an active writer")
            .unwrap();
        run.request_fork(&json!({"thread":{"id":"branch","status":{"type":"notLoaded"},"turns":[{"id":"second-turn","status":"completed","items":[]}]}})).unwrap();
        run.start_forked_turn(&json!({"model":"default","thread":{"id":"branch-two","forkedFromId":"branch","status":{"type":"idle"},"turns":[{"id":"second-turn","status":"completed","items":[]}]}}), &store).unwrap();
        let saved = Store::open(directory.clone())
            .unwrap()
            .task(&run.task.id)
            .unwrap();
        assert_eq!(saved.thread_id.as_deref(), Some("branch-two"));
        assert_eq!(
            saved.supplements[0].source_thread_id.as_deref(),
            Some("thread")
        );
        assert_eq!(
            saved.supplements[1].source_thread_id.as_deref(),
            Some("branch")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn control_rejection_keeps_root_open_for_real_completion() {
        let mut run = fake_run();
        run.controls.insert(100, "补充方向".into());
        assert!(run.reject_control(100));
        assert!(!run.closed);
        assert_eq!(run.task.status, "running");
        assert!(!run.reject_control(1));
        assert!(project(
            &mut run.task,
            &json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"completed"}}})
        ));
        assert_eq!(run.task.status, "completed");
    }
    #[test]
    #[cfg(unix)]
    fn backend_answer_is_consumed_once_and_invalid_after_cancel() {
        let mut run = fake_run();
        let approval = Approval {
            id: "approval".into(),
            request_id: "42".into(),
            run_id: "run".into(),
            turn_id: "turn".into(),
            title: "input".into(),
            description: "".into(),
            kind: "input".into(),
            question_ids: vec!["scope".into()],
            questions: vec![],
            question_error: None,
        };
        run.task.status = "approval".into();
        run.task.approvals.push(approval.clone());
        run.pending.insert("approval".into(), json!({"id":42}));
        let answers = HashMap::from([("scope".into(), "yes".into())]);
        let response = run.answer("run", "approval", answers.clone()).unwrap();
        assert_eq!(response["id"], 42);
        assert_eq!(response["result"]["answers"]["scope"]["answers"][0], "yes");
        assert!(run.answer("run", "approval", answers.clone()).is_err());
        run.task.approvals.push(approval);
        run.pending.insert("approval".into(), json!({"id":42}));
        run.task.status = "cancelling".into();
        run.pending.clear();
        run.task.approvals.clear();
        assert!(run.answer("run", "approval", answers).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn structured_answer_keeps_selected_label_and_secret_and_blocks_parse_errors() {
        let mut run=fake_run();
        let questions=parse_input_questions(&json!([
            {"id":"scope","question":"Scope?","options":[{"label":" Local ","description":"Offline"}]},
            {"id":"secret","question":"Secret?","isSecret":true}
        ])).unwrap();
        let approval=Approval{id:"a".into(),request_id:"42".into(),run_id:"run".into(),turn_id:"turn".into(),title:"Ask".into(),description:"".into(),kind:"input".into(),question_ids:vec!["scope".into(),"secret".into()],questions,question_error:None};
        run.task.status="approval".into();
        run.task.approvals.push(approval.clone());
        run.pending.insert("a".into(),json!({"id":42}));
        let answers=HashMap::from([("scope".into()," Local ".into()),("secret".into()," keep spaces ".into())]);
        let response=run.answer("run","a",answers.clone()).unwrap();
        assert_eq!(response["result"]["answers"]["scope"]["answers"][0]," Local ");
        assert_eq!(response["result"]["answers"]["secret"]["answers"][0]," keep spaces ");
        let mut invalid=approval;invalid.question_error=Some("Malformed request".into());
        run.task.approvals.push(invalid);
        run.pending.insert("a".into(),json!({"id":42}));
        assert!(run.answer("run","a",answers).is_err());
        assert!(run.pending.contains_key("a"));
    }
    #[test]
    #[cfg(unix)]
    fn shutdown_waits_for_registration_and_rejects_later_starts() {
        let directory =
            std::env::temp_dir().join(format!("orbit-lifecycle-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(directory.clone()).unwrap());
        let guard = runtime.begin_start().unwrap();
        let other = runtime.clone();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let ready = barrier.clone();
        let (done, finished) = mpsc::channel();
        let shutdown = thread::spawn(move || {
            ready.wait();
            other.shutdown();
            done.send(()).unwrap();
        });
        barrier.wait();
        assert!(finished.recv_timeout(Duration::from_millis(30)).is_err());
        let run = Arc::new(Mutex::new(fake_run()));
        runtime
            .runs
            .lock()
            .unwrap()
            .insert("run".into(), run.clone());
        drop(guard);
        finished.recv_timeout(Duration::from_secs(3)).unwrap();
        shutdown.join().unwrap();
        assert!(run.lock().unwrap().closed);
        assert!(run.lock().unwrap().child.stopped);
        assert!(runtime.begin_start().is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn cleanup_error_after_leader_exit_never_signals_reused_group() {
        use std::os::unix::process::CommandExt;
        let child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .unwrap();
        let mut owned = OwnedChild::new(child);
        thread::sleep(Duration::from_millis(100));
        assert!(owned
            .stop_with(|_| Err(std::io::Error::from_raw_os_error(libc::EPERM)))
            .is_err());
        assert!(owned.reaped);
        assert!(!owned.stopped);
        assert!(owned
            .stop_with(|_| panic!("must not signal a reaped leader's group"))
            .is_err());
    }
    #[test]
    #[cfg(unix)]
    fn closed_run_with_live_uncleaned_leader_cannot_admit_new_root() {
        let mut run = fake_run();
        run.closed = true;
        assert!(run
            .child
            .stop_with(|_| Err(std::io::Error::from_raw_os_error(libc::EPERM)))
            .is_err());
        assert!(!run.child.reaped);
        assert!(!run.idle());
        run.child.stop().unwrap();
        assert!(run.idle());
    }
    #[test]
    #[cfg(unix)]
    fn retained_inspection_blocks_admission_until_cleanup_is_confirmed() {
        use std::os::unix::process::CommandExt;
        let directory =
            std::env::temp_dir().join(format!("orbit-inspection-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(directory.clone()).unwrap());
        let run = fake_run();
        runtime.inspections.lock().unwrap().push(run.child);
        assert!(runtime.ensure_idle().is_ok());
        assert!(runtime.inspections.lock().unwrap().is_empty());
        let mut uncertain = OwnedChild::new(
            Command::new("/bin/sh")
                .args(["-c", "exit 0"])
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        thread::sleep(Duration::from_millis(100));
        assert!(uncertain
            .stop_with(|_| Err(std::io::Error::from_raw_os_error(libc::EPERM)))
            .is_err());
        assert!(uncertain.reaped);
        runtime.inspections.lock().unwrap().push(uncertain);
        let lifecycle = runtime.begin_start().unwrap();
        assert!(runtime.ensure_idle().is_err());
        assert_eq!(runtime.inspections.lock().unwrap().len(), 1);
        drop(lifecycle);
        assert!(runtime
            .sync_agents("missing".into())
            .unwrap_err()
            .contains("清理未确认"));
        assert!(runtime.begin_start().is_ok()); // Failure released the lifecycle lock.
        runtime.shutdown();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn runtime_preserves_uncertain_cleanup_and_rejects_new_root() {
        use std::os::unix::process::CommandExt;
        let directory =
            std::env::temp_dir().join(format!("orbit-uncertain-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(directory.clone()).unwrap());
        let mut run = fake_run();
        run.child.stop().unwrap();
        run.child = OwnedChild::new(
            Command::new("/bin/sh")
                .args(["-c", "exit 0"])
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        thread::sleep(Duration::from_millis(100));
        assert!(run
            .child
            .stop_with(|_| Err(std::io::Error::from_raw_os_error(libc::EPERM)))
            .is_err());
        run.closed = true;
        runtime
            .runs
            .lock()
            .unwrap()
            .insert("run".into(), Arc::new(Mutex::new(run)));
        assert!(runtime.ensure_idle().is_err());
        assert_eq!(runtime.runs.lock().unwrap().len(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[ignore = "explicit real inference check; replies OK, no tools or user task"]
    fn local_minimal_real_turn_with_catalog_default() {
        let directory =
            std::env::temp_dir().join(format!("orbit-model-smoke-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut child = OwnedChild::new(command().current_dir(&directory).spawn().unwrap());
        let mut input = child.child.stdin.take().unwrap();
        let output = child.child.stdout.take().unwrap();
        let mut stderr = child.child.stderr.take().unwrap();
        thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                match read_message(&mut reader) {
                    Ok(Some(value)) => {
                        if tx.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = tx.send(Err("executor ended"));
                        break;
                    }
                }
            }
        });
        let mut task = Task::new(
            "Orbit connection verification".into(),
            "Reply exactly OK. Do not use tools, edit files, or spawn agents.".into(),
            "writing".into(),
        );
        let mut cursors = Vec::new();
        send(&mut input, &initialize()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        while !task.terminal() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = rx
                .recv_timeout(remaining)
                .expect("real turn timeout")
                .unwrap();
            if message.get("error").is_some() {
                panic!("{}", error_message(&message["error"]));
            }
            if message.get("method").is_some() && message.get("id").is_some() {
                send(&mut input, &json!({"id":message["id"],"error":{"code":-32601,"message":"No tool requests in this verification"}})).unwrap();
            } else if let Some(id) = message["id"].as_u64() {
                match id {
                    1 => {
                        send(&mut input, &json!({"method":"initialized"})).unwrap();
                        send(
                            &mut input,
                            &json!({"id":4,"method":"model/list","params":{"limit":100}}),
                        )
                        .unwrap();
                    }
                    4 => match select_model(&message["result"], &mut cursors).unwrap() {
                        ModelChoice::Selected(model) => {
                            println!("Requested catalog default: {model}");
                            send(&mut input, &thread_request(&directory, &model)).unwrap();
                        }
                        ModelChoice::NextPage(cursor) => {
                            send(&mut input, &json!({"id":4,"method":"model/list","params":{"limit":100,"cursor":cursor}})).unwrap();
                        }
                    },
                    2 => {
                        task.thread_id =
                            message["result"]["thread"]["id"].as_str().map(String::from);
                        assert!(task.thread_id.is_some());
                        task.root_node();
                        println!("Actual thread model: {}", message["result"]["model"]);
                        send(&mut input, &json!({"id":3,"method":"turn/start","params":{"threadId":task.thread_id,"input":[{"type":"text","text":task.prompt}]}})).unwrap();
                    }
                    3 => {
                        task.turn_id = message["result"]["turn"]["id"].as_str().map(String::from);
                    }
                    _ => (),
                }
            } else {
                project(&mut task, &message);
            }
        }
        assert_eq!(
            task.status,
            "completed",
            "{}",
            task.events
                .last()
                .map(|e| e.text.as_str())
                .unwrap_or("no event")
        );
        let output = &task.nodes[0].output;
        assert_eq!(output.trim(), "OK");
        child.stop().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
        println!("Confirmed turn/completed and text result: OK");
    }
    #[test]
    fn partial_child_transport_failure_retains_success_and_stops_later_reads() {
        let root: Value =
            serde_json::from_str(include_str!("../fixtures/subagent-history.json")).unwrap();
        let mut task = Task::new("test".into(), "test".into(), "research".into());
        task.status = "completed".into();
        task.thread_id = Some(root["id"].as_str().unwrap().into());
        task.turn_id = Some(root["turns"][0]["id"].as_str().unwrap().into());
        let nodes = observed_agent_nodes(&task, &root).unwrap();
        let broken = std::cell::Cell::new(false);
        let mut count = 0;
        let data = read_child_details(
            &nodes,
            Instant::now() + Duration::from_secs(20),
            &broken,
            0,
            |_, _| {
                count += 1;
                if count == 1 {
                    Ok(json!({"id":"first"}))
                } else {
                    broken.set(true);
                    Err("timeout".into())
                }
            },
        );
        assert_eq!(count, 2);
        assert!(data[0].result.is_ok());
        assert!(data[1].result.is_err());
        assert!(data[2].result.is_err());
        let mut calls = 0;
        let data = read_child_details(
            &nodes,
            Instant::now(),
            &std::cell::Cell::new(false),
            0,
            |_, _| {
                calls += 1;
                Ok(json!({}))
            },
        );
        assert_eq!(calls, 0);
        assert!(data.iter().all(|r| r.result.is_err()));
    }
    #[test]
    fn collaboration_sync_waits_for_the_current_lifecycle_section() {
        let directory =
            std::env::temp_dir().join(format!("orbit-sync-lock-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(directory.clone()).unwrap());
        let guard = runtime.begin_start().unwrap();
        let other = runtime.clone();
        let (started, ready) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let worker = thread::spawn(move || {
            started.send(()).unwrap();
            done.send(other.sync_agents("missing".into())).unwrap();
        });
        let began = ready.recv_timeout(Duration::from_secs(3));
        let blocked = finished.recv_timeout(Duration::from_millis(30));
        drop(guard);
        let result = finished.recv_timeout(Duration::from_secs(3));
        worker.join().unwrap();
        assert!(began.is_ok());
        assert!(matches!(blocked, Err(mpsc::RecvTimeoutError::Timeout)));
        assert_eq!(result.unwrap().unwrap_err(), "真实任务不存在");
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[ignore = "explicit local read-only history check; no inference"]
    fn local_collaboration_history_without_inference() {
        let real = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap()
            .join("Library/Application Support/local.orbit.agent-workbench/workspace.json");
        let disk: Value = serde_json::from_slice(&std::fs::read(real).unwrap()).unwrap();
        let task: Task = serde_json::from_value(
            disk["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["threadId"] == "01a101a0-bba0-7e00-9a71-07f250c731ef")
                .unwrap()
                .clone(),
        )
        .unwrap();
        let directory =
            std::env::temp_dir().join(format!("orbit-real-sync-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(directory.clone()).unwrap());
        runtime.store.save_task(task.clone()).unwrap();
        let saved = runtime.sync_agents(task.id.clone()).unwrap();
        assert_eq!(saved.nodes.len(), 4);
        assert!(saved.nodes.iter().all(|n| n.status == "completed"));
        assert!(
            saved
                .nodes
                .iter()
                .filter(|n| Some(n.id.as_str()) != saved.thread_id.as_deref())
                .all(|n| !n.output.trim().is_empty()),
            "completed child Agent answers were not synchronized"
        );
        assert!(saved
            .nodes
            .iter()
            .skip(1)
            .all(|n| n.detail_turn_count == 2 && n.detail_notice.is_none()));
        assert_eq!(
            runtime.sync_agents(task.id.clone()).unwrap().revision,
            saved.revision
        );
        assert_eq!(saved.artifacts[0].content, task.artifacts[0].content);
        assert_eq!(saved.status, task.status);
        assert_eq!(runtime.store.task(&task.id).unwrap().nodes.len(), 4);
        std::fs::write(
            directory.join("projected-task.json"),
            serde_json::to_vec_pretty(&saved).unwrap(),
        )
        .unwrap();
        println!("Actual CLI history restored 4 completed Agent nodes, preserving delivery; projection: {}", directory.join("projected-task.json").display());
        runtime.shutdown();
        assert!(runtime.begin_start().is_err());
    }
    #[test]
    #[ignore = "requires local Codex CLI; initialize only, no model calls"]
    fn local_initialize_without_model_calls() {
        let result = doctor();
        assert!(result.available, "{}", result.message);
        assert!(result.initialized, "{}", result.message);
        println!("{}", result.version);
    }
}
#[derive(Clone)]
pub struct Runtime {
    pub store: Arc<Store>,
    runs: Arc<Mutex<HashMap<String, Arc<Mutex<Run>>>>>,
    starting: Arc<Mutex<bool>>,
    inspections: Arc<Mutex<Vec<OwnedChild>>>,
}

fn cli_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    for path in [
        home.join(".local/bin/codex"),
        home.join(".cargo/bin/codex"),
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ] {
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from("codex")
}
fn command() -> Command {
    let mut c = Command::new(cli_path());
    c.arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    c
}
fn initialize() -> Value {
    json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"orbit-workbench","title":"Orbit","version":"0.1.0"},"capabilities":{"experimentalApi":true}}})
}

fn read_agent_history(
    task: &Task,
    inspections: &Mutex<Vec<OwnedChild>>,
) -> Result<AgentHistory, String> {
    let mut child = OwnedChild::new(command().spawn().map_err(|_| "无法启动 CLI 读取协作记录")?);
    let stdout = child.child.stdout.take().unwrap();
    let stderr = child.child.stderr.take().unwrap();
    thread::spawn(move || {
        let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
    });
    let (tx, rx) = mpsc::sync_channel(8);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let result = read_message(&mut reader);
            let done = !matches!(&result, Ok(Some(_)));
            if tx.send(result).is_err() || done {
                break;
            }
        }
    });
    let result = (|| {
        let deadline = Instant::now() + Duration::from_secs(20);
        let broken = std::cell::Cell::new(false);
        let receive = |id| -> Result<Value, String> {
            for _ in 0..128 {
                let message = rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| {
                        broken.set(true);
                        "读取协作记录超时"
                    })?
                    .map_err(|error| {
                        broken.set(true);
                        error
                    })?
                    .ok_or_else(|| {
                        broken.set(true);
                        "CLI 已关闭协作记录连接"
                    })?;
                if message["id"] == id {
                    if message.get("error").is_some() {
                        return Err(format!(
                            "协作记录读取失败：{}",
                            error_message(&message["error"])
                        ));
                    }
                    return Ok(message["result"].clone());
                }
            }
            broken.set(true);
            Err("协作记录通知超过限制，原图保留".into())
        };
        let input = child.child.stdin.as_mut().unwrap();
        send(input, &initialize())?;
        receive(1)?;
        send(input, &json!({"method":"initialized","params":{}}))?;
        send(
            input,
            &json!({"id":2,"method":"thread/read","params":{"threadId":task.thread_id,"includeTurns":true}}),
        )?;
        let root = receive(2)?["thread"].clone();
        let nodes = observed_agent_nodes(task, &root)?;
        let details = read_child_details(
            &nodes,
            deadline,
            &broken,
            root.to_string().len(),
            |node_id, id| {
                send(input, &json!({"id":id,"method":"thread/read","params":{"threadId":node_id,"includeTurns":true}}))
                .map_err(|error| { broken.set(true); error })?;
                Ok(receive(id)?["thread"].clone())
            },
        );
        Ok(AgentHistory { root, details })
    })();
    drop(rx);
    if child.stop().is_err() {
        inspections.lock().unwrap().push(child);
        return Err("协作记录进程清理未确认，原图保留；已阻止新运行".into());
    }
    result
}

fn read_child_details(
    nodes: &[crate::model::Node],
    deadline: Instant,
    broken: &std::cell::Cell<bool>,
    mut total_bytes: usize,
    mut read: impl FnMut(&str, u64) -> Result<Value, String>,
) -> Vec<AgentRead> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let result =
                if broken.get() || Instant::now() >= deadline || total_bytes >= 8 * 1024 * 1024 {
                    Err("读取连接或容量已到限制，尚未同步；已有输出保留".into())
                } else {
                    read(&node.id, i as u64 + 3).and_then(|value| {
                        total_bytes += value.to_string().len();
                        if total_bytes > 8 * 1024 * 1024 {
                            Err("协作详情超过 8 MiB，已有输出保留".into())
                        } else {
                            Ok(value)
                        }
                    })
                };
            AgentRead {
                id: node.id.clone(),
                result,
            }
        })
        .collect()
}

pub fn doctor() -> Doctor {
    let path = cli_path().display().to_string();
    let mut result = Doctor {
        provider: "codex".into(),
        capabilities: Capabilities::default(),
        available: false,
        initialized: false,
        path,
        version: String::new(),
        message: "未找到可运行的 Codex CLI".into(),
    };
    // Check initialize only; no thread or inference is started.
    let mut child = match command().spawn() {
        Ok(c) => OwnedChild::new(c),
        Err(_) => return result,
    };
    result.available = true;
    let stderr = child.child.stderr.take().unwrap();
    let capture = Arc::new(Mutex::new(String::new()));
    let captured = capture.clone();
    thread::spawn(move || {
        let mut reader = stderr;
        let mut bytes = [0; 1024];
        while let Ok(n) = reader.read(&mut bytes) {
            if n == 0 {
                break;
            }
            let mut s = captured.lock().unwrap();
            s.push_str(&String::from_utf8_lossy(&bytes[..n]));
            if s.len() > 4096 {
                *s = s
                    .chars()
                    .rev()
                    .take(2048)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
            }
        }
    });
    let output = child.child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(read_message(&mut BufReader::new(output)));
    });
    let sent = send(child.child.stdin.as_mut().unwrap(), &initialize());
    let response = if sent.is_ok() {
        rx.recv_timeout(Duration::from_secs(8)).ok()
    } else {
        None
    };
    match response {
        Some(Ok(Some(value))) if value["id"] == 1 && value.get("result").is_some() => {
            result.initialized = true;
            result.capabilities = Capabilities::codex();
            result.version = value["result"]["userAgent"]
                .as_str()
                .unwrap_or("版本未返回")
                .to_string();
            result.message = "初始化握手成功 · 未调用模型".into();
        }
        Some(Ok(Some(_))) => result.message = "执行器返回协议错误或不兼容响应".into(),
        Some(Ok(None)) => result.message = "CLI 在初始化前退出，请检查本机状态目录与配置".into(),
        Some(Err(error)) => result.message = error,
        None => {
            result.message = if sent.is_err() {
                "无法向 CLI 发送初始化请求"
            } else {
                "初始化检查超时，已结束检查进程"
            }
            .into()
        }
    }
    stop_child(&mut child);
    if !result.initialized
        && capture
            .lock()
            .unwrap()
            .contains("failed to initialize sqlite state runtime")
    {
        result.message = "Codex 本地状态目录不可写，请在正常桌面环境运行".into();
    }
    result
}

impl Runtime {
    #[cfg(test)]
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(store),
            runs: Arc::new(Mutex::new(HashMap::new())),
            starting: Arc::new(Mutex::new(false)),
            inspections: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn from_shared(store: Arc<Store>) -> Self {
        Self {
            store,
            runs: Arc::new(Mutex::new(HashMap::new())),
            starting: Arc::new(Mutex::new(false)),
            inspections: Arc::new(Mutex::new(Vec::new())),
        }
    }
    fn begin_start(&self) -> Result<std::sync::MutexGuard<'_, bool>, String> {
        let guard = self.starting.lock().unwrap();
        if *guard {
            Err("工作台正在关闭，不能启动新任务".into())
        } else {
            Ok(guard)
        }
    }
    pub fn sync_agents(&self, task_id: String) -> Result<Task, String> {
        let _lifecycle = self.begin_start()?;
        self.ensure_idle()?;
        let task = self.store.task(&task_id).ok_or("真实任务不存在")?;
        if task.archived {
            return Err("归档任务只读，不能同步协作".into());
        }
        if !task.terminal() {
            return Err("请等任务结束后再同步协作记录".into());
        }
        task.thread_id.as_deref().ok_or("任务没有 CLI 会话记录")?;
        let run_id = task.run_id.as_deref().ok_or("任务没有运行标识")?;
        let turn_id = task.turn_id.as_deref().ok_or("任务没有本轮标识")?;
        let history = read_agent_history(&task, &self.inspections)?;
        let matching = self.runs.lock().unwrap().get(run_id).cloned();
        if let Some(run) = matching {
            let mut r = run.lock().unwrap();
            if !r.closed {
                return Err("执行器仍在结束中，请稍后同步".into());
            }
            let saved = self
                .store
                .sync_agents(&task_id, run_id, turn_id, &history)?;
            r.task = saved.clone().actor_snapshot();
            Ok(saved)
        } else {
            self.store.sync_agents(&task_id, run_id, turn_id, &history)
        }
    }
    #[cfg(test)]
    pub fn edit_artifact(
        &self,
        id: String,
        expected: String,
        content: String,
    ) -> Result<Task, String> {
        let _lifecycle = self.begin_start()?;
        let matching = self
            .runs
            .lock()
            .unwrap()
            .values()
            .find(|run| {
                run.lock()
                    .unwrap()
                    .task
                    .artifacts
                    .iter()
                    .any(|a| a.id == id)
            })
            .cloned();
        if let Some(run) = matching {
            let mut r = run.lock().unwrap();
            if !r.closed {
                return Err("执行器仍在结束中，请稍后保存".into());
            }
            let saved = self.store.edit_artifact(&id, &expected, &content)?;
            r.task = saved.clone().actor_snapshot();
            Ok(saved)
        } else {
            self.store.edit_artifact(&id, &expected, &content)
        }
    }
    #[cfg(test)]
    pub fn archive_task(&self, task_id: String) -> Result<Task, String> {
        let _lifecycle = self.begin_start()?;
        let matching: Vec<_> = self
            .runs
            .lock()
            .unwrap()
            .values()
            .filter(|r| r.lock().unwrap().task.id == task_id)
            .cloned()
            .collect();
        for run in &matching {
            if !run.lock().unwrap().idle() {
                return Err("执行器清理尚未确认，请稍后归档".into());
            }
        }
        let saved = self.store.archive_task(&task_id)?;
        for run in matching {
            run.lock().unwrap().task = saved.clone();
        }
        Ok(saved)
    }
    #[cfg(test)]
    pub fn delete_task(&self, task_id: String) -> Result<(), String> {
        let _lifecycle = self.begin_start()?;
        let mut runs = self.runs.lock().unwrap();
        for run in runs.values() {
            let r = run.lock().unwrap();
            if r.task.id == task_id && !r.idle() {
                return Err("执行器清理尚未确认，不能删除任务".into());
            }
        }
        self.store.delete_task(&task_id)?;
        runs.retain(|_, r| r.lock().unwrap().task.id != task_id);
        Ok(())
    }
    fn ensure_idle(&self) -> Result<(), String> {
        {
            let mut inspections = self.inspections.lock().unwrap();
            for child in inspections.iter_mut() {
                let _ = child.stop();
            }
            inspections.retain(|child| !child.stopped);
            if !inspections.is_empty() {
                return Err("协作记录进程清理未确认，请核对本机进程后重启工作台".into());
            }
        }
        for run in self.runs.lock().unwrap().values() {
            let mut r = run.lock().unwrap();
            if !r.closed {
                return Err("当前已有真实任务运行，请先结束或中断它".into());
            }
            let _ = r.child.stop();
            if !r.idle() {
                return Err("旧执行器清理未确认，请核对本机进程后重启工作台".into());
            }
        }
        Ok(())
    }
    fn publish(&self, app: &AppHandle, task: Task) {
        match self.store.save_existing_task(task) {
            Ok(Some(saved)) => {
                let _ = app.emit("runtime-task", saved);
            }
            Ok(None) => {}
            Err(message) => {
                let _ = app.emit("runtime-warning", message);
            }
        }
    }
    fn fail_unlaunched(&self, app: &AppHandle, mut task: Task, error: &str) -> Task {
        task.status = "failed".into();
        task.finished_at = Some(now());
        task.event(error, "error", "工作台");
        self.publish(app, task.clone());
        task
    }
    fn launch(
        &self,
        app: AppHandle,
        task: Task,
        resume_turn: Option<String>,
    ) -> Result<Task, String> {
        let run_id = task.run_id.clone().unwrap();
        let directory = self.store.directory.join("runs").join(&run_id);
        if std::fs::create_dir_all(&directory).is_err() {
            return Ok(self.fail_unlaunched(&app, task, "无法创建任务目录；已有交付保留"));
        }
        let child = match command().current_dir(&directory).spawn() {
            Ok(child) => child,
            Err(_) => {
                return Ok(self.fail_unlaunched(
                    &app,
                    task,
                    "无法启动 Codex，请先在执行器设置中检查连接；已有交付保留",
                ))
            }
        };
        let mut child = OwnedChild::new(child);
        let input = child.child.stdin.take().unwrap();
        let output = child.child.stdout.take().unwrap();
        let mut stderr = child.child.stderr.take().unwrap();
        thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        let run = Arc::new(Mutex::new(Run {
            task: task.clone(),
            child,
            input,
            pending: HashMap::new(),
            controls: HashMap::new(),
            next_id: 100,
            model_cursors: Vec::new(),
            requested_model: None,
            fork_anchor: None,
            closed: false,
            last_emit: Instant::now(),
            resume_turn,
            startup: Startup::Connecting,
            buffered: Vec::new(),
            buffered_bytes: 0,
        }));
        {
            let mut runs = self.runs.lock().unwrap();
            runs.retain(|_, r| !r.lock().unwrap().closed);
            runs.insert(run_id.clone(), run.clone());
        }
        let _ = app.emit("runtime-task", task.clone());
        let sent = {
            let mut r = run.lock().unwrap();
            send(&mut r.input, &initialize())
        };
        if let Err(error) = sent {
            self.fail(&app, &run, &error, "failed");
            return Ok(run.lock().unwrap().task.clone());
        }
        let runtime = self.clone();
        let reader_run = run.clone();
        let reader_app = app.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                match read_message(&mut reader) {
                    Ok(Some(message)) => {
                        if !runtime.receive(&reader_app, &reader_run, message, &directory) {
                            break;
                        }
                    }
                    Ok(None) => {
                        runtime.fail(
                            &reader_app,
                            &reader_run,
                            "执行器连接结束，在途结果需核对",
                            "unknown",
                        );
                        break;
                    }
                    Err(error) => {
                        runtime.fail(&reader_app, &reader_run, &error, "unknown");
                        break;
                    }
                }
            }
            let mut run = reader_run.lock().unwrap();
            stop_child(&mut run.child);
        });
        let timeout_runtime = self.clone();
        let timeout_run = run.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(25));
            timeout_runtime.fail_if(
                &app,
                &timeout_run,
                "启动超时，任务结果需核对",
                "unknown",
                |r| r.task.turn_id.is_none(),
            );
        });
        Ok(task)
    }
    fn fail(&self, app: &AppHandle, run: &Arc<Mutex<Run>>, message: &str, status: &str) {
        self.fail_if(app, run, message, status, |_| true);
    }
    fn fail_if(
        &self,
        app: &AppHandle,
        run: &Arc<Mutex<Run>>,
        message: &str,
        status: &str,
        condition: impl FnOnce(&Run) -> bool,
    ) {
        let snapshot = {
            let mut r = run.lock().unwrap();
            if r.closed || r.task.terminal() || !condition(&r) {
                return;
            }
            r.closed = true;
            r.task.status = status.into();
            r.task.finished_at = Some(now());
            r.task.approvals.clear();
            r.task.unconfirm_directions();
            r.pending.clear();
            for n in &mut r.task.nodes {
                if matches!(n.status.as_str(), "running" | "approval" | "queued") {
                    n.status = "unknown".into();
                }
            }
            r.task.event(message, "error", "工作台");
            if r.child.stop().is_err() {
                r.task.event(
                    "拥有的进程组清理未确认；暂停新任务，请核对本机执行器",
                    "error",
                    "工作台",
                );
            }
            r.task.clone()
        };
        self.publish(app, snapshot);
    }
    fn receive(
        &self,
        app: &AppHandle,
        run: &Arc<Mutex<Run>>,
        message: Value,
        directory: &std::path::Path,
    ) -> bool {
        let mut r = run.lock().unwrap();
        if r.closed {
            return false;
        }
        match r.defer_notification(&message) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(error) => {
                drop(r);
                self.fail(app, run, &error, "unknown");
                return false;
            }
        }
        let mut replay = Vec::new();
        let mut changed = false;
        if message.get("method").is_some() && message.get("id").is_some() {
            let method = message["method"].as_str().unwrap_or("");
            let id = message["id"].clone();
            let params = &message["params"];
            if method == "item/tool/requestUserInput"
                && params["threadId"].as_str() == r.task.thread_id.as_deref()
                && params["turnId"].as_str() == r.task.turn_id.as_deref()
                && r.task.status != "cancelling"
                && r.pending.len() < 16
            {
                let key = id.to_string();
                let approval_id = format!("{}:{}", r.task.run_id.as_deref().unwrap(), key);
                if r.pending.contains_key(&approval_id) {
                    return true;
                }
                let questions = params["questions"].as_array().cloned().unwrap_or_default();
                let (input_questions, question_error) = match parse_input_questions(&params["questions"]) {
                    Ok(value) => (value, None),
                    Err(error) => (vec![], Some(error)),
                };
                let description = questions
                    .iter()
                    .filter_map(|q| q["question"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let approval = Approval {
                    id: approval_id.clone(),
                    request_id: key,
                    run_id: r.task.run_id.clone().unwrap(),
                    turn_id: r.task.turn_id.clone().unwrap(),
                    title: "Codex 需要补充信息".into(),
                    description: description.chars().take(2000).collect(),
                    kind: "input".into(),
                    question_ids: questions
                        .iter()
                        .filter_map(|q| q["id"].as_str().map(String::from))
                        .collect(),
                    questions: input_questions,
                    question_error,
                };
                r.pending.insert(approval_id, message.clone());
                r.task.approvals.push(approval);
                r.task.status = "approval".into();
                r.task.event("收到补充信息请求", "approval", "Codex");
                changed = true;
            } else {
                let response = match method {
                    "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                        json!({"id":id,"result":{"decision":"decline"}})
                    }
                    _ => {
                        json!({"id":id,"error":{"code":-32601,"message":"Unsupported request in Orbit read-only prototype"}})
                    }
                };
                if send(&mut r.input, &response).is_err() {
                    drop(r);
                    self.fail(app, run, "权限响应发送失败，结果需核对", "unknown");
                    return false;
                }
                r.task.event(
                    "只读原型已拒绝权限提升或不支持的工具请求",
                    "approval",
                    "工作台",
                );
                changed = true;
            }
        } else if message.get("id").is_some() {
            let id = message["id"].as_u64().unwrap_or(0);
            let expected = match id {
                1 | 4 => r.startup == Startup::Connecting && r.requested_model.is_none(),
                2 => r.startup == Startup::Connecting && r.requested_model.is_some(),
                5 => r.startup == Startup::ReadingSource,
                6 => r.startup == Startup::Forking,
                3 => r.startup == Startup::AwaitingTurn,
                _ => true,
            };
            if !expected {
                return true;
            }
            if message.get("error").is_some() {
                if r.reject_control(id) {
                    let snapshot = r.task.clone();
                    drop(r);
                    self.publish(app, snapshot);
                    return true;
                }
                let detail = error_message(&message["error"]);
                if let Some(request) = r.recover_writer(id, &detail) {
                    if send(&mut r.input, &request).is_err() {
                        drop(r);
                        self.fail(app, run, "无法核对占用会话，未发送补充", "failed");
                        return false;
                    }
                    let snapshot = r.task.clone();
                    drop(r);
                    self.publish(app, snapshot);
                    return true;
                }
                let label = "初始化或启动";
                drop(r);
                self.fail(
                    app,
                    run,
                    &format!("{}请求被执行器拒绝：{}", label, detail),
                    "failed",
                );
                return false;
            }
            let request = match id {
                1 if r.startup == Startup::Connecting && r.requested_model.is_none() => {
                    if send(&mut r.input, &json!({"method":"initialized"})).is_err() {
                        drop(r);
                        self.fail(app, run, "初始化通知发送失败", "failed");
                        return false;
                    }
                    Some(r.initialized_model_request(directory))
                }
                4 if r.startup == Startup::Connecting && r.requested_model.is_none() => {
                    match select_model(&message["result"], &mut r.model_cursors) {
                        Ok(ModelChoice::Selected(model)) => {
                            r.task.event(
                                &format!("本次请求 CLI 目录默认模型：{}", model),
                                "system",
                                "工作台",
                            );
                            changed = true;
                            Some(r.thread_request(directory, &model))
                        }
                        Ok(ModelChoice::NextPage(cursor)) => Some(
                            json!({"id":4,"method":"model/list","params":{"limit":100,"cursor":cursor}}),
                        ),
                        Err(error) => {
                            drop(r);
                            self.fail(app, run, &error, "failed");
                            return false;
                        }
                    }
                }
                2 if r.startup == Startup::Connecting && r.requested_model.is_some() => {
                    match r.start_turn(&message["result"]) {
                        Ok(request) => {
                            changed = true;
                            Some(request)
                        }
                        Err(error) => {
                            drop(r);
                            self.fail(app, run, &error, "failed");
                            return false;
                        }
                    }
                }
                5 if r.startup == Startup::ReadingSource => {
                    match r.request_fork(&message["result"]) {
                        Ok(request) => Some(request),
                        Err(error) => {
                            drop(r);
                            self.fail(app, run, &error, "failed");
                            return false;
                        }
                    }
                }
                6 if r.startup == Startup::Forking => {
                    match r.start_forked_turn(&message["result"], &self.store) {
                        Ok(request) => {
                            changed = true;
                            Some(request)
                        }
                        Err(error) => {
                            drop(r);
                            self.fail(app, run, &error, "failed");
                            return false;
                        }
                    }
                }
                3 if r.startup == Startup::AwaitingTurn => {
                    match r.accept_turn(&message["result"]) {
                        Ok(buffered) => {
                            changed = true;
                            replay = buffered;
                            None
                        }
                        Err(error) => {
                            drop(r);
                            self.fail(app, run, &error, "unknown");
                            return false;
                        }
                    }
                }
                _ => {
                    changed = r.confirm_control(id, &message["result"]);
                    None
                }
            };
            if let Some(request) = request {
                if send(&mut r.input, &request).is_err() {
                    drop(r);
                    self.fail(
                        app,
                        run,
                        "无法发送启动请求，执行结果需核对",
                        if request["method"] == "turn/start" {
                            "unknown"
                        } else {
                            "failed"
                        },
                    );
                    return false;
                }
            }
        } else {
            changed = project(&mut r.task, &message);
            if message["method"] == "serverRequest/resolved" {
                let request_id = message["params"]["requestId"].to_string();
                r.pending.retain(|_, m| m["id"].to_string() != request_id);
            }
        }
        let terminal = r.task.terminal();
        if terminal {
            r.closed = true;
            r.pending.clear();
            if r.child.stop().is_err() {
                r.task.event(
                    "执行结果已确认；拥有的进程组清理未确认，请核对本机执行器",
                    "error",
                    "工作台",
                );
            }
        }
        let delta = message["method"] == "item/agentMessage/delta";
        let should_emit = changed && (!delta || r.last_emit.elapsed() > Duration::from_millis(180));
        let snapshot = should_emit.then(|| {
            r.last_emit = Instant::now();
            r.task.clone()
        });
        drop(r);
        if let Some(snapshot) = snapshot {
            self.publish(app, snapshot);
        }
        for deferred in replay {
            if !self.receive(app, run, deferred, directory) {
                return false;
            }
        }
        !terminal
    }
    fn run(&self, id: &str) -> Result<Arc<Mutex<Run>>, String> {
        self.runs
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "运行已失效，请核对状态".into())
    }
    pub fn steer(&self, app: &AppHandle, id: String, text: String) -> Result<(), String> {
        let run = self.run(&id)?;
        let mut r = run.lock().unwrap();
        let (candidate, request) = r.direction_request(&text)?;
        let request_id = r.next_id;
        let saved = self
            .store
            .save_existing_task(candidate)?
            .ok_or("任务状态已变化，补充未发送")?;
        r.task = saved.clone().actor_snapshot();
        r.next_id += 1;
        r.controls.insert(request_id, "补充方向".into());
        if send(&mut r.input, &request).is_err() {
            drop(r);
            self.fail(
                app,
                &run,
                "补充发送结果未知，请核对交付，勿重复发送",
                "unknown",
            );
            return Err("补充发送结果未知，请勿重复发送".into());
        }
        drop(r);
        let _ = app.emit("runtime-task", saved);
        let runtime = self.clone();
        let app = app.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(20));
            let mut r = run.lock().unwrap();
            let key = format!("{}:{}", id, request_id);
            if let Some(direction) = r
                .task
                .directions
                .iter_mut()
                .find(|d| d.id == key && d.status == "pending")
            {
                direction.status = "unknown".into();
                r.task.event(
                    "补充确认超时，请在交付中核对结果，勿重复发送",
                    "error",
                    "工作台",
                );
                let task = r.task.clone();
                drop(r);
                runtime.publish(&app, task);
            }
        });
        Ok(())
    }
    pub fn interrupt(&self, app: AppHandle, id: String) -> Result<(), String> {
        let run = self.run(&id)?;
        let mut r = run.lock().unwrap();
        if r.closed {
            return Err("运行已结束".into());
        }
        if r.task.status == "cancelling" {
            return Ok(());
        }
        let Some(turn) = r.task.turn_id.clone() else {
            drop(r);
            self.fail(
                &app,
                &run,
                "已结束启动进程；尚未确认是否存在外部执行",
                "unknown",
            );
            return Ok(());
        };
        let thread = r.task.thread_id.clone().ok_or("会话未创建")?;
        let request_id = r.next_id;
        r.next_id += 1;
        send(
            &mut r.input,
            &json!({"id":request_id,"method":"turn/interrupt","params":{"threadId":thread,"turnId":turn}}),
        )?;
        r.controls.insert(request_id, "中断".into());
        r.task.status = "cancelling".into();
        r.task.approvals.clear();
        r.pending.clear();
        r.task
            .event("已请求中断，等待执行器确认", "system", "工作台");
        let task = r.task.clone();
        drop(r);
        self.publish(&app, task);
        let runtime = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(10));
            runtime.fail_if(
                &app,
                &run,
                "中断确认超时；已尝试停止拥有的执行器，结果与清理范围需核对",
                "unknown",
                |r| r.task.status == "cancelling",
            );
        });
        Ok(())
    }
    pub fn reply(
        &self,
        app: &AppHandle,
        id: String,
        approval_id: String,
        answers: HashMap<String, String>,
    ) -> Result<(), String> {
        let run = self.run(&id)?;
        let mut r = run.lock().unwrap();
        let response = r.answer(&id, &approval_id, answers)?;
        if send(&mut r.input, &response).is_err() {
            drop(r);
            self.fail(app, &run, "答复发送结果未知，请勿重复批准", "unknown");
            return Err("答复发送结果未知".into());
        }
        if r.task.approvals.is_empty() {
            r.task.status = "running".into();
        }
        r.task.event("信息答复已发送", "approval", "你");
        let task = r.task.clone();
        drop(r);
        self.publish(app, task);
        Ok(())
    }
    pub fn shutdown(&self) {
        let mut lifecycle = self.starting.lock().unwrap();
        *lifecycle = true;
        for child in self.inspections.lock().unwrap().iter_mut() {
            let _ = child.stop();
        }
        for run in self.runs.lock().unwrap().values() {
            let mut r = run.lock().unwrap();
            if !r.closed {
                r.closed = true;
                r.task.status = "unknown".into();
                r.task.unconfirm_directions();
                r.task.approvals.clear();
                r.pending.clear();
                r.task
                    .event("App 已关闭，在途结果需核对", "system", "工作台");
            }
            if r.child.stop().is_err() {
                r.task.event(
                    "拥有的进程组清理未确认，请核对本机执行器",
                    "error",
                    "工作台",
                );
            }
            let _ = self.store.save_existing_task(r.task.clone());
        }
    }
}

impl Executor for Runtime {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "codex".into(),
            name: "Codex".into(),
            protocol: "codex-app-server".into(),
            description: "本机 CLI · 支持续交付、运行中补充与子 Agent 历史".into(),
            permission_note: "文件沙箱只读，拒绝权限提升；MCP 沿用本机设置".into(),
            capabilities: Capabilities::codex(),
        }
    }
    fn doctor(&self) -> Doctor {
        doctor()
    }
    fn models(&self) -> Result<Vec<crate::executor::ExecutorModel>, String> { model_catalog() }
    fn ensure_idle(&self) -> Result<(), String> {
        Runtime::ensure_idle(self)
    }
    fn ensure_task_idle(&self, task_id: &str) -> Result<(), String> {
        for run in self.runs.lock().unwrap().values() {
            let r = run.lock().unwrap();
            if r.task.id == task_id && !r.idle() {
                return Err("该任务执行器仍在运行或清理尚未确认".into());
            }
        }
        Ok(())
    }
    fn launch(
        &self,
        app: AppHandle,
        task: Task,
        resume_anchor: Option<String>,
    ) -> Result<Task, String> {
        Runtime::launch(self, app, task, resume_anchor)
    }
    fn steer(&self, app: &AppHandle, run_id: String, text: String) -> Result<(), String> {
        Runtime::steer(self, app, run_id, text)
    }
    fn interrupt(&self, app: AppHandle, run_id: String) -> Result<(), String> {
        Runtime::interrupt(self, app, run_id)
    }
    fn reply(
        &self,
        app: &AppHandle,
        run_id: String,
        approval_id: String,
        answers: HashMap<String, String>,
    ) -> Result<(), String> {
        Runtime::reply(self, app, run_id, approval_id, answers)
    }
    fn sync_agents(&self, task_id: String) -> Result<Task, String> {
        Runtime::sync_agents(self, task_id)
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
            .retain(|_, r| r.lock().unwrap().task.id != task_id);
    }
    fn shutdown(&self) {
        Runtime::shutdown(self)
    }
}


pub(crate) fn model_catalog() -> Result<Vec<crate::executor::ExecutorModel>, String> {
    let mut child = OwnedChild::new(command().spawn().map_err(|_| "无法启动 Codex CLI，请先检查连接")?);
    let stdout = child.child.stdout.take().unwrap();
    let mut stderr = child.child.stderr.take().unwrap();
    thread::spawn(move || { let _ = std::io::copy(&mut stderr, &mut std::io::sink()); });
    let (tx, rx) = mpsc::sync_channel(8);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let result = read_message(&mut reader);
            let done = !matches!(&result, Ok(Some(_)));
            if tx.send(result).is_err() || done { break; }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut bytes = 0usize;
    let mut messages = 0usize;
    let mut response = |id: u64| -> Result<Value, String> {
        loop {
            let value = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "Codex 模型目录查询超时")??.ok_or("Codex 模型目录连接已关闭")?;
            messages += 1;
            bytes += value.to_string().len();
            if messages > 128 || bytes > 1024 * 1024 { return Err("模型目录响应超过大小限制".into()); }
            if value["id"] == id {
                if value.get("error").is_some() { return Err(error_message(&value["error"])); }
                return value.get("result").cloned().ok_or("模型目录响应缺少 result".into());
            }
        }
    };
    let result = (|| {
        let input = child.child.stdin.as_mut().unwrap();
        send(input, &initialize())?;
        response(1)?;
        send(input, &json!({"method":"initialized"}))?;
        let mut cursor: Option<String> = None;
        let mut cursors = Vec::new();
        let mut models: Vec<crate::executor::ExecutorModel> = Vec::new();
        for _ in 0..8 {
            send(input, &json!({"id":2,"method":"model/list","params":{"limit":100,"includeHidden":false,"cursor":cursor}}))?;
            let page = response(2)?;
            for model in crate::executor::codex_model_page(&page)? {
                if !models.iter().any(|m| m.id == model.id) { models.push(model); }
            }
            if models.len() > 512 { return Err("模型目录超过数量限制".into()); }
            match page.get("nextCursor") {
                None | Some(Value::Null) => return if models.is_empty() { Err("Codex 未返回可选模型".into()) } else { Ok(models) },
                Some(Value::String(next)) if !next.is_empty() && next.len() <= 1024 && !cursors.contains(next) => {
                    cursors.push(next.clone()); cursor = Some(next.clone());
                }
                _ => return Err("模型目录分页格式无效或重复".into()),
            }
        }
        Err("模型目录分页超过 8 页".into())
    })();
    child.stop()?;
    result
}
