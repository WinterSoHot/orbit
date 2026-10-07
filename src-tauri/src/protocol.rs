use crate::model::{now, Node, Task};
#[cfg(test)] use crate::model::Artifact;
use serde_json::Value;
use std::io::{BufRead, Read};

#[derive(Debug, PartialEq)]
pub enum ModelChoice {
    Selected(String),
    NextPage(String),
}
pub fn select_model(result: &Value, cursors: &mut Vec<String>) -> Result<ModelChoice, String> {
    let rows = result["data"].as_array().ok_or("模型目录响应无效")?;
    if let Some(model) = rows
        .iter()
        .find(|m| m["isDefault"] == true && m["hidden"] == false)
        .and_then(|m| m["model"].as_str())
        .filter(|id| !id.trim().is_empty() && id.len() <= 256)
    {
        return Ok(ModelChoice::Selected(model.into()));
    }
    let Some(cursor) = result["nextCursor"].as_str().filter(|s| !s.is_empty()) else {
        return Err("CLI 模型目录没有可见的默认模型，请检查 CLI 版本与配置".into());
    };
    if cursors.len() >= 7 || cursors.iter().any(|seen| seen == cursor) {
        return Err("模型目录分页重复或超过 8 页，已停止启动".into());
    }
    cursors.push(cursor.into());
    Ok(ModelChoice::NextPage(cursor.into()))
}
pub fn thread_request(directory: &std::path::Path, model: &str) -> Value {
    serde_json::json!({"id":2,"method":"thread/start","params":{"model":model,"cwd":directory.to_string_lossy(),"approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":"read-only","developerInstructions":"本次任务仅允许只读操作。不要修改文件，不要调用会产生外部写入的 MCP 工具。普通回复留在对话；明确交付须按本轮提交契约输出，未完成或需要澄清时不得提交。","ephemeral":false}})
}
pub fn resume_request(thread: &str, model: &str) -> Value {
    let mut request = thread_request(std::path::Path::new("/"), model);
    request["method"] = serde_json::json!("thread/resume");
    let params = request["params"].as_object_mut().unwrap();
    params.remove("cwd");
    params.remove("ephemeral");
    params.insert("threadId".into(), serde_json::json!(thread));
    request
}
pub fn validate_resume(thread: &Value, expected: &str, previous_turn: &str) -> Result<(), String> {
    let last = thread["turns"].as_array().and_then(|turns| turns.last());
    if thread["id"] != expected
        || thread["status"]["type"] != "idle"
        || last.is_none_or(|turn| {
            turn["id"] != previous_turn
                || !matches!(
                    turn["status"].as_str(),
                    Some("completed" | "failed" | "interrupted")
                )
        })
    {
        return Err("原会话状态或最后一轮已变化，已停止续接；请核对 Codex 会话后新建任务".into());
    }
    Ok(())
}
pub fn turn_request(thread: &str, input: &str) -> Value {
    let input=crate::delivery::instruction(input);
    serde_json::json!({"id":3,"method":"turn/start","params":{"threadId":thread,"input":[{"type":"text","text":input}],"sandboxPolicy":{"type":"readOnly"}}})
}
// Only error.message is read; never persist raw protocol payloads or stderr.
pub fn error_message(error: &Value) -> String {
    let mut message = error["message"]
        .as_str()
        .unwrap_or("执行器未提供错误详情")
        .to_string();
    for _ in 0..2 {
        let Ok(nested) = serde_json::from_str::<Value>(&message) else {
            break;
        };
        let Some(inner) = nested["error"]["message"].as_str() else {
            break;
        };
        message = inner.to_string();
    }
    // Conservative: once a credential label appears, hide the remainder rather
    // than guessing where an opaque value ends. ASCII case folding preserves
    // the byte positions used to truncate the original UTF-8 message.
    let lower = message.to_ascii_lowercase();
    let labels = [
        "api_key",
        "api-key",
        "apikey",
        "access_token",
        "accesstoken",
        "refresh_token",
        "token",
        "secret",
        "password",
        "passwd",
        "authorization",
        "cookie",
        "bearer",
        "key",
    ];
    let cutoff = labels
        .iter()
        .flat_map(|label| {
            lower.match_indices(label).filter_map(|(at, _)| {
                let boundary = |c: char| !c.is_alphanumeric() && c != '_';
                let left = lower[..at].chars().next_back().is_none_or(boundary);
                let right = lower[at + label.len()..]
                    .chars()
                    .next()
                    .is_none_or(boundary);
                (left && right).then_some(at)
            })
        })
        .min();
    if let Some(at) = cutoff {
        message.truncate(at);
        message.push_str(" [已隐藏]");
    }
    let safe = message
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            if lower.contains("sk-") || lower.contains("eyj") || lower.contains("://") {
                "[已隐藏]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let safe: String = safe.chars().take(1600).collect();
    if safe.is_empty() {
        "执行器未提供错误详情".into()
    } else {
        safe
    }
}

pub const MAX_MESSAGE: usize = 1024 * 1024;
pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>, String> {
    loop {
        let mut bytes = Vec::new();
        Read::take(&mut *reader, (MAX_MESSAGE + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| "无法读取执行器消息".to_string())?;
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.len() > MAX_MESSAGE {
            return Err("执行器消息超过 1 MiB 限制".into());
        }
        if bytes.last() != Some(&b'\n') {
            return Err("执行器消息被截断".into());
        }
        if bytes.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        return serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "执行器返回了非法 JSON".into());
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn node_status(value: &str) -> String {
    match value {
        "running" | "inProgress" => "running",
        "completed" => "completed",
        "interrupted" | "shutdown" => "interrupted",
        "errored" | "failed" => "failed",
        "pendingInit" => "queued",
        _ => "unknown",
    }
    .into()
}
fn append_bounded(target: &mut String, value: &str) {
    let remaining = 64000usize.saturating_sub(target.chars().count());
    target.extend(value.chars().take(remaining));
}

pub const MAX_AGENT_ACTIVITIES: usize = 2048;

fn valid_agent_path(path: &str) -> bool {
    path.starts_with("/root/")
        && path.len() <= 256
        && path.split('/').skip(1).all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
        })
}
fn resolve_agent_parents(task: &mut Task) {
    let mut paths: Vec<_> = task
        .nodes
        .iter()
        .filter_map(|n| n.agent_path.as_ref().map(|p| (n.id.clone(), p.clone())))
        .collect();
    if let Some(root) = &task.thread_id {
        paths.retain(|(id, _)| id != root);
        paths.push((root.clone(), "/root".into()));
    }
    for node in &mut task.nodes {
        if let Some(path) = &node.agent_path {
            if let Some((parent, _)) = path.rsplit_once('/') {
                let matches: Vec<_> = paths
                    .iter()
                    .filter(|(id, p)| p == parent && id != &node.id)
                    .collect();
                node.parent_id = if matches.len() == 1 {
                    Some(matches[0].0.clone())
                } else {
                    None
                };
            }
        }
    }
}
fn project_activity(task: &mut Task, item: &Value) -> bool {
    let id = text(item, "agentThreadId");
    let path = text(item, "agentPath");
    let kind = text(item, "kind");
    let item_id = text(item, "id");
    if id.is_empty()
        || id.len() > 128
        || Some(id) == task.thread_id.as_deref()
        || item_id.is_empty()
        || item_id.len() > 256
        || !valid_agent_path(path)
        || !matches!(kind, "started" | "interacted" | "completed" | "interrupted")
    {
        return false;
    }
    let event_id = format!(
        "{}:{item_id}:{kind}",
        task.run_id.as_deref().unwrap_or("run")
    );
    if task.agent_activity_ids.contains(&event_id) {
        return false;
    }
    if task.agent_activity_ids.len() >= MAX_AGENT_ACTIVITIES {
        const WARNING: &str = "Agent 活动超过 2048 条，新活动暂停投影，关系图不完整";
        if task.events.iter().any(|e| e.text == WARNING) {
            return false;
        }
        task.event(WARNING, "error", "工作台");
        return true;
    }
    if !task.nodes.iter().any(|n| n.id == id) {
        if task.nodes.len() >= 64 {
            if !task
                .events
                .iter()
                .any(|e| e.text == "Agent 超过 64 个，关系图展示不完整")
            {
                task.event("Agent 超过 64 个，关系图展示不完整", "error", "工作台");
                return true;
            }
            return false;
        }
        task.nodes.push(Node {
            id: id.into(),
            name: path.rsplit('/').next().unwrap().into(),
            role: "子 Agent".into(),
            model: "实际模型未返回".into(),
            status: "unknown".into(),
            summary: String::new(),
            output: String::new(),
            parent_id: None,
            agent_path: Some(path.into()),
            detail_notice: None,
            detail_turn_count: 0,
            output_truncated: false,
        });
    }
    let node = task.nodes.iter_mut().find(|n| n.id == id).unwrap();
    node.agent_path = Some(path.into());
    match kind {
        "started" => node.status = "running".into(),
        "completed" => node.status = "completed".into(),
        "interrupted" => node.status = "interrupted".into(),
        _ => (),
    }
    node.summary = match kind {
        "started" => "子 Agent 已开始工作",
        "completed" => "子 Agent 本轮已结束",
        "interrupted" => "已观测到中断活动",
        _ => "已观测到消息交互",
    }
    .into();
    let name = node.name.clone();
    resolve_agent_parents(task);
    task.event(&format!("{path}：{kind}"), "agent", &name);
    task.agent_activity_ids.push(event_id.clone());
    task.events.last_mut().unwrap().id = event_id;
    true
}

pub fn project(task: &mut Task, message: &Value) -> bool {
    let method = text(message, "method");
    let p = &message["params"];
    let root = task.thread_id.clone().unwrap_or_default();
    let thread = text(p, "threadId");
    if method == "thread/started" {
        let t = &p["thread"];
        let id = text(t, "id");
        let parent = text(t, "parentThreadId");
        if id == root
            || id.is_empty()
            || parent.is_empty()
            || (parent != root && !task.nodes.iter().any(|n| n.id == parent))
            || task.nodes.len() >= 64
        {
            return false;
        }
        if !task.nodes.iter().any(|n| n.id == id) {
            task.nodes.push(Node {
                id: id.into(),
                name: text(t, "agentNickname").to_string(),
                role: text(t, "agentRole").to_string(),
                model: "CLI 默认模型".into(),
                status: "running".into(),
                summary: "已观测到子 Agent".into(),
                output: String::new(),
                parent_id: Some(parent.into()),
                agent_path: None,
                detail_notice: None,
                detail_turn_count: 0,
                output_truncated: false,
            });
        }
        task.revision += 1;
        return true;
    }
    if thread.is_empty() || (thread != root && !task.nodes.iter().any(|n| n.id == thread)) {
        return false;
    }
    if let Some(turn) = p.get("turnId").and_then(Value::as_str) {
        if thread == root && task.turn_id.as_deref() != Some(turn) {
            return false;
        }
    }
    if task.terminal() {
        return false;
    }
    task.root_node();
    let conversation_changed = crate::conversation::project(task, method, p);
    match method {
        "turn/started" if thread == root => {
            let id = text(&p["turn"], "id");
            if id.is_empty() || task.turn_id.as_deref().is_some_and(|known| known != id) {
                return false;
            }
            task.turn_id = Some(id.into());
            task.event("执行器已开始本轮任务", "agent", "Codex");
        }
        "turn/completed" if thread == root => {
            let turn = &p["turn"];
            if task.turn_id.as_deref() != Some(text(turn, "id")) {
                return false;
            }
            let status = text(turn, "status");
            let failure = format!("执行失败：{}", error_message(&turn["error"]));
            task.status = match status {
                "completed" => "completed",
                "interrupted" => "interrupted",
                _ => "failed",
            }
            .into();
            task.finished_at = Some(now());
            task.approvals.clear();
            task.unconfirm_directions();
            task.refresh_answer();
            if let Some(n) = task.nodes.iter_mut().find(|n| n.id == root) {
                n.status = task.status.clone();
                n.summary = match status {
                    "completed" => "本轮执行结束".into(),
                    "interrupted" => "本轮执行已终止".into(),
                    _ => failure.clone(),
                };
            }
            for node in &mut task.nodes {
                if node.id != root
                    && matches!(node.status.as_str(), "running" | "approval" | "queued")
                {
                    node.status = "unknown".into();
                }
            }
            task.answer_items.clear();
            task.event(
                if status == "completed" {
                    "本轮执行结束，回复已保留"
                } else if status == "interrupted" {
                    "执行器确认本轮已中断"
                } else {
                    &failure
                },
                "agent",
                "Codex",
            );
        }
        "item/agentMessage/delta" => {
            if let Some(n) = task.nodes.iter_mut().find(|n| n.id == thread) {
                append_bounded(&mut n.output, text(p, "delta"));
            }
            task.revision += 1;
        }
        "item/started" | "item/completed" => {
            let item = &p["item"];
            match text(item, "type") {
                "agentMessage" if method == "item/completed" => {
                    if thread == root
                        && matches!(item["phase"].as_str(), None | Some("final_answer"))
                    {
                        task.collect_answer(text(item, "id"), text(item, "text"));
                        if item["phase"]=="final_answer" && p["turnId"].as_str().is_some_and(|id|!id.is_empty()&&Some(id)==task.turn_id.as_deref()) && matches!(item["status"].as_str(),None|Some("completed")) {task.delivery_candidate.capture(text(item,"id"),text(item,"text"));}
                    } else if thread != root || task.answer_items.is_empty() {
                        if let Some(n) = task.nodes.iter_mut().find(|n| n.id == thread) {
                            n.output = text(item, "text").chars().take(64000).collect();
                        }
                    }
                    task.revision += 1;
                }
                "subAgentActivity" if method == "item/completed" => {
                    return project_activity(task, item);
                }
                "collabAgentToolCall" => {
                    let sender = text(item, "senderThreadId");
                    let spawn = text(item, "tool") == "spawnAgent";
                    if let Some(states) = item["agentsStates"].as_object() {
                        for (id, state) in states {
                            if id.is_empty() || id.len() > 128 || id == &root {
                                continue;
                            }
                            let parent = if spawn
                                && id != sender
                                && (sender == root || task.nodes.iter().any(|n| n.id == sender))
                            {
                                Some(sender.to_string())
                            } else {
                                None
                            };
                            if let Some(n) = task.nodes.iter_mut().find(|n| &n.id == id) {
                                n.status = node_status(text(state, "status"));
                                if parent.is_some() && n.parent_id.is_none() {
                                    n.parent_id = parent;
                                }
                                let output = text(state, "message");
                                if !output.is_empty() {
                                    n.output = output.chars().take(64000).collect();
                                }
                            } else if task.nodes.len() < 64 {
                                task.nodes.push(Node {
                                    id: id.clone(),
                                    name: format!(
                                        "Agent {}",
                                        &id.chars().take(6).collect::<String>()
                                    ),
                                    role: "子 Agent".into(),
                                    model: text(item, "model").to_string(),
                                    status: node_status(text(state, "status")),
                                    summary: "状态来自协作调用".into(),
                                    output: text(state, "message").chars().take(64000).collect(),
                                    parent_id: parent,
                                    agent_path: None,
                                    detail_notice: None,
                                    detail_turn_count: 0,
                                    output_truncated: false,
                                });
                            }
                        }
                    }
                    task.event(
                        &format!("协作调用：{}", text(item, "tool")),
                        "agent",
                        "Codex",
                    );
                }
                "commandExecution" | "fileChange" => {
                    task.event(
                        if method == "item/started" {
                            "工具开始执行"
                        } else {
                            "工具执行结束"
                        },
                        "tool",
                        "Codex",
                    );
                }
                _ => return conversation_changed,
            }
        }
        "thread/tokenUsage/updated" if thread == root => {
            task.tokens = p["tokenUsage"]["total"]["totalTokens"].as_u64();
            task.revision += 1;
        }
        "serverRequest/resolved" => {
            let id = p["requestId"].to_string();
            task.approvals.retain(|a| a.request_id != id);
            if task.approvals.is_empty() && task.status == "approval" {
                task.status = "running".into();
            }
            task.revision += 1;
        }
        _ => return false,
    }
    true
}

pub fn reconcile_agents(task: &Task, history: &Value) -> Result<Task, String> {
    if task.provider != "codex"
        || !task.terminal()
        || Some(text(history, "id")) != task.thread_id.as_deref()
    {
        return Err("记录与当前任务不匹配，或任务尚未结束".into());
    }
    let turn = history["turns"]
        .as_array()
        .and_then(|turns| {
            turns
                .iter()
                .find(|t| Some(text(t, "id")) == task.turn_id.as_deref())
        })
        .ok_or("历史中未找到本轮任务")?;
    let items = turn["items"].as_array().ok_or("协作记录格式不受支持")?;
    let mut projected = task.clone();
    projected.status = "running".into();
    projected.events.clear();
    projected.agent_activity_ids.clear();
    projected.root_node();
    for item in items {
        let kind = text(item, "type");
        if !matches!(kind, "subAgentActivity" | "collabAgentToolCall") {
            continue;
        }
        if kind == "subAgentActivity" {
            let event_id = format!(
                "{}:{}:{}",
                task.run_id.as_deref().unwrap_or("run"),
                text(item, "id"),
                text(item, "kind")
            );
            if projected.agent_activity_ids.len() >= MAX_AGENT_ACTIVITIES
                && !projected.agent_activity_ids.contains(&event_id)
            {
                return Err("Agent 活动超过 2048 条，无法完整同步，原图保留".into());
            }
            if !valid_agent_path(text(item, "agentPath"))
                || text(item, "agentThreadId").is_empty()
                || text(item, "agentThreadId").len() > 128
                || Some(text(item, "agentThreadId")) == task.thread_id.as_deref()
                || text(item, "id").is_empty()
                || text(item, "id").len() > 256
                || !matches!(
                    text(item, "kind"),
                    "started" | "interacted" | "completed" | "interrupted"
                )
            {
                return Err("协作记录包含不支持的 Agent 活动，原图保留".into());
            }
            if projected.nodes.len() >= 64
                && !projected
                    .nodes
                    .iter()
                    .any(|n| n.id == text(item, "agentThreadId"))
            {
                return Err("Agent 超过 64 个，无法完整同步，原图保留".into());
            }
        }
        if kind == "collabAgentToolCall" {
            let states = item["agentsStates"]
                .as_object()
                .ok_or("旧协作记录格式不受支持，原图保留")?;
            let new = states
                .keys()
                .filter(|id| {
                    Some(id.as_str()) != task.thread_id.as_deref()
                        && !projected.nodes.iter().any(|n| &n.id == *id)
                })
                .count();
            if projected.nodes.len() + new > 64 {
                return Err("Agent 超过 64 个，无法完整同步，原图保留".into());
            }
        }
        project(
            &mut projected,
            &serde_json::json!({"method":"item/completed","params":{"threadId":task.thread_id,"turnId":task.turn_id,"item":item}}),
        );
    }
    resolve_agent_parents(&mut projected);
    for node in &mut projected.nodes {
        if Some(node.id.as_str()) == task.thread_id.as_deref() {
            if let Some(original) = task.nodes.iter().find(|n| n.id == node.id) {
                *node = original.clone();
            } else {
                node.status = task.status.clone();
            }
        } else {
            if let Some(original) = task
                .nodes
                .iter()
                .find(|n| n.id == node.id && !n.output.is_empty())
            {
                node.output = original.output.clone();
            }
            if matches!(node.status.as_str(), "running" | "approval" | "queued") {
                node.status = "unknown".into();
            }
        }
    }
    let mut ids = task.agent_activity_ids.clone();
    for id in projected.agent_activity_ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.len() > MAX_AGENT_ACTIVITIES {
        return Err("Agent 活动超过去重容量，原图保留".into());
    }
    if projected.nodes == task.nodes && ids == task.agent_activity_ids {
        return Ok(task.clone());
    }
    let mut saved = task.clone();
    saved.nodes = projected.nodes;
    saved.agent_activity_ids = ids;
    saved.event(
        &format!("协作记录已同步：{} 个 Agent", saved.nodes.len()),
        "agent",
        "工作台",
    );
    Ok(saved)
}

pub struct AgentRead {
    pub id: String,
    pub result: Result<Value, String>,
}
pub struct AgentHistory {
    pub root: Value,
    pub details: Vec<AgentRead>,
}
pub fn observed_agent_nodes(task: &Task, root: &Value) -> Result<Vec<Node>, String> {
    let mut fresh = task.clone();
    fresh
        .nodes
        .retain(|n| Some(n.id.as_str()) == task.thread_id.as_deref());
    Ok(reconcile_agents(&fresh, root)?
        .nodes
        .into_iter()
        .filter(|n| Some(n.id.as_str()) != task.thread_id.as_deref())
        .collect())
}
pub fn reconcile_agent_history(task: &Task, history: &AgentHistory) -> Result<Task, String> {
    let observed = observed_agent_nodes(task, &history.root)?;
    let mut saved = reconcile_agents(task, &history.root)?;
    for read in &history.details {
        let Some(identity) = observed.iter().find(|n| n.id == read.id) else {
            continue;
        };
        let Some(node) = saved.nodes.iter_mut().find(|n| n.id == read.id) else {
            continue;
        };
        let merged = (|| -> Result<(String, usize, bool, Option<String>), String> {
            let thread = read.result.as_ref().map_err(Clone::clone)?;
            if text(thread, "id") != identity.id
                || identity.parent_id.is_none()
                || Some(text(thread, "parentThreadId")) != identity.parent_id.as_deref()
            {
                return Err("子会话身份或父节点不匹配，原输出保留".into());
            }
            if let Some(expected) = identity.agent_path.as_deref() {
                let path = &thread["source"]["subAgent"]["thread_spawn"]["agent_path"];
                if path.as_str() != Some(expected) {
                    return Err("子会话路径不匹配，原输出保留".into());
                }
            }
            let turns = thread["turns"].as_array().ok_or("子会话没有可读取的轮次")?;
            let mut output = String::new();
            let mut count = 0;
            let mut truncated = false;
            let mut message_ids = std::collections::HashSet::new();
            // These are this child's historical rounds, not an asserted root-turn association.
            for (i, turn) in turns.iter().enumerate() {
                let items = turn["items"]
                    .as_array()
                    .ok_or("子会话轮次内容不完整，原输出保留")?;
                if items
                    .iter()
                    .any(|m| text(m, "type") == "agentMessage" && !m["text"].is_string())
                {
                    return Err("子会话消息正文不完整，原输出保留".into());
                }
                let has_final = items.iter().any(|m| {
                    text(m, "type") == "agentMessage"
                        && text(m, "phase") == "final_answer"
                        && !text(m, "text").trim().is_empty()
                });
                let mut round = false;
                for message in items {
                    if text(message, "type") != "agentMessage"
                        || text(message, "text").trim().is_empty()
                        || (has_final && text(message, "phase") != "final_answer")
                    {
                        continue;
                    }
                    let id = (text(turn, "id"), text(message, "id"));
                    if id.0.trim().is_empty() || id.1.trim().is_empty() {
                        return Err("子会话消息标识不完整，原输出保留".into());
                    }
                    if !message_ids.insert(id) {
                        continue;
                    }
                    if !round {
                        count += 1;
                        round = true;
                        let heading = format!(
                            "{}## 第 {} 轮\n\n",
                            if output.is_empty() { "" } else { "\n\n" },
                            i + 1
                        );
                        append_bounded(&mut output, &heading);
                    } else {
                        append_bounded(&mut output, "\n\n");
                    }
                    let text = text(message, "text");
                    if text.chars().count() > 64000usize.saturating_sub(output.chars().count()) {
                        truncated = true;
                    }
                    append_bounded(&mut output, text);
                }
            }
            if output.trim().is_empty() {
                return Err("子会话尚无文本输出，已有输出保留".into());
            }
            let model = thread["model"]
                .as_str()
                .filter(|m| !m.trim().is_empty() && m.len() <= 256)
                .map(str::to_owned);
            Ok((output, count, truncated, model))
        })();
        match merged {
            Ok((output, count, truncated, model)) => {
                node.output = output;
                node.detail_turn_count = count;
                node.output_truncated = truncated;
                node.detail_notice =
                    truncated.then(|| "输出超过 64,000 字符，仅显示前 64,000 字符".into());
                if let Some(model) = model {
                    node.model = model;
                }
            }
            Err(error) => node.detail_notice = Some(error.chars().take(300).collect()),
        }
    }
    // Ignore the intermediate graph event. Commit one revision only when the final projection changes.
    saved.revision = task.revision;
    saved.events = task.events.clone();
    if saved.nodes != task.nodes || saved.agent_activity_ids != task.agent_activity_ids {
        saved.event(
            &format!("协作详情已同步：{} 个 Agent", saved.nodes.len()),
            "agent",
            "工作台",
        );
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_final_reply_does_not_become_a_delivery() {
        let mut t=crate::model::Task::new("test".into(),"goal".into(),"research".into());t.thread_id=Some("root".into());t.turn_id=Some("turn".into());
        super::project(&mut t,&serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"reply","type":"agentMessage","phase":"final_answer","text":"Which option should I choose?"}}}));
        super::project(&mut t,&serde_json::json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn","status":"completed"}}}));
        assert!(t.artifacts.is_empty());assert_eq!(t.conversation[0].text,"Which option should I choose?");
    }
    use super::*;
    use crate::model::Approval;
    use serde_json::json;

    fn fixture() -> Task {
        let mut task = Task::new("Test".into(), "Test".into(), "research".into());
        task.thread_id = Some("root".into());
        task.turn_id = Some("turn-1".into());
        task
    }
    #[test]
    fn final_answer_aggregation_is_idempotent_bounded_and_not_polluted_by_deltas() {
        let mut task = fixture();
        let answer = serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"same","type":"agentMessage","text":"legacy answer"}}});
        project(&mut task, &answer);
        project(&mut task, &answer);
        project(
            &mut task,
            &serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"old","item":{"id":"old","type":"agentMessage","text":"old poison"}}}),
        );
        project(
            &mut task,
            &serde_json::json!({"method":"item/agentMessage/delta","params":{"threadId":"root","turnId":"turn-1","delta":"stream poison"}}),
        );
        project(
            &mut task,
            &serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"comment","type":"agentMessage","phase":"commentary","text":"commentary poison"}}}),
        );
        project(
            &mut task,
            &serde_json::json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn-1","status":"completed"}}}),
        );
        assert!(task.artifacts.is_empty());
        assert_eq!(task.conversation.iter().filter(|m|m.item_id=="same").count(),1);
        assert_eq!(task.nodes[0].output,"legacy answer");
        assert!(task.answer_items.is_empty());
        let mut task = fixture();
        task.root_node();
        task.collect_answer("large", &"中".repeat(65000));
        task.collect_answer("large", &"中".repeat(65000));
        for i in 0..25 {
            task.collect_answer(&format!("item-{i}"), "extra");
        }
        task.refresh_answer();
        assert!(task.nodes[0].output.chars().count() <= 64000);
        assert!(task.nodes[0].output_truncated);
        assert!(task.nodes[0].output.ends_with("内容已截断。"));
        task.begin_run(true);
        assert!(task.answer_items.is_empty());
        assert!(task.nodes.is_empty());
    }
    #[test]
    fn two_final_answers_stay_in_chat_without_a_delivery() {
        let mut task = fixture();
        project(
            &mut task,
            &serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"answer-1","type":"agentMessage","phase":"final_answer","text":"original complete plan"}}}),
        );
        project(
            &mut task,
            &serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"answer-2","type":"agentMessage","phase":"final_answer","text":"additional storage design"}}}),
        );
        project(
            &mut task,
            &serde_json::json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn-1","status":"completed"}}}),
        );
        assert!(task.artifacts.is_empty());
        assert!(task.nodes[0].output.contains("original complete plan"));
        assert!(task.nodes[0].output.contains("additional storage design"));
        assert_eq!(task.conversation.len(),2);
        assert!(!crate::delivery::commit(&mut task).unwrap());

    }
    #[test]
    fn catalog_default_is_explicit_and_hidden_models_are_not_selected() {
        let mut cursors = Vec::new();
        let choice = select_model(&json!({"data":[{"model":"hidden","isDefault":true,"hidden":true},{"model":"catalog-default","isDefault":true,"hidden":false}]}), &mut cursors).unwrap();
        assert_eq!(choice, ModelChoice::Selected("catalog-default".into()));
        let request = thread_request(std::path::Path::new("/tmp/run"), "catalog-default");
        assert_eq!(request["method"], "thread/start");
        assert_eq!(request["params"]["model"], "catalog-default");
        assert_eq!(request["params"]["sandbox"], "read-only");
    }
    #[test]
    fn model_pagination_handles_later_default_and_rejects_loops_or_missing_default() {
        let mut cursors = Vec::new();
        assert_eq!(
            select_model(&json!({"data":[],"nextCursor":"page-2"}), &mut cursors).unwrap(),
            ModelChoice::NextPage("page-2".into())
        );
        assert_eq!(
            select_model(
                &json!({"data":[{"model":"later","isDefault":true,"hidden":false}]}),
                &mut cursors
            )
            .unwrap(),
            ModelChoice::Selected("later".into())
        );
        assert!(select_model(&json!({"data":[],"nextCursor":"page-2"}), &mut cursors).is_err());
        assert!(select_model(&json!({"data":[],"nextCursor":null}), &mut Vec::new()).is_err());
        assert!(select_model(
            &json!({"data":[],"nextCursor":"overflow"}),
            &mut vec!["seen".into(); 7]
        )
        .is_err());
    }
    #[test]
    fn failure_details_are_visible_but_stale_errors_are_ignored() {
        let mut task = fixture();
        let error = json!({"message":"{\"error\":{\"message\":\"The 'gpt-6.1-sol' model is not supported when using Codex with a ChatGPT account.\"}}"});
        assert!(!project(
            &mut task,
            &json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"old","status":"failed","error":error}}})
        ));
        assert!(task.events.is_empty());
        assert!(project(
            &mut task,
            &json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn-1","status":"failed","error":error}}})
        ));
        assert_eq!(task.status, "failed");
        assert!(task.events.last().unwrap().text.contains("gpt-6.1-sol"));
        assert!(task.nodes[0].summary.contains("not supported"));
    }
    #[test]
    fn provider_error_redacts_common_credentials_before_truncation() {
        let message = error_message(
            &json!({"message":"Model gpt-example rejected Bearer abcsecret sk-private eyJprivate.jwt token=private secret=private https://host/path?key=private"}),
        );
        assert!(message.contains("Model gpt-example rejected"));
        for secret in [
            "abcsecret",
            "sk-private",
            "eyJprivate",
            "token=private",
            "secret=private",
            "https://host",
        ] {
            assert!(!message.contains(secret));
        }
        assert!(
            error_message(&json!({"message":"x".repeat(4000)}))
                .chars()
                .count()
                <= 1600
        );
    }
    #[test]
    fn opaque_credentials_with_spaces_and_quotes_are_not_persisted() {
        for detail in [
            "token= abcsecret",
            "api_key: abcsecret",
            "password: abcsecret",
            "\"api_key\" = \"abcsecret\"",
            "{\"error\":\"bad\",\"api_key\":\"abcsecret\"}",
        ] {
            let message = error_message(&json!({"message":format!("Model rejected: {detail}")}));
            assert!(message.contains("Model rejected"));
            assert!(
                !message.contains("abcsecret"),
                "credential leaked: {detail}"
            );
        }
    }
    #[test]
    fn completion_ignores_old_turn_and_is_terminal() {
        let mut task = fixture();
        assert!(!project(
            &mut task,
            &json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"old","status":"completed"}}})
        ));
        assert!(project(
            &mut task,
            &json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"turn-1","status":"completed"}}})
        ));
        assert_eq!(task.status, "completed");
        assert!(!project(
            &mut task,
            &json!({"method":"turn/started","params":{"threadId":"root","turn":{"id":"new","status":"inProgress"}}})
        ));
    }
    #[test]
    fn framing_handles_multiple_messages_eof_and_invalid_json() {
        let mut bytes = std::io::Cursor::new(b"{\"id\":1}\n{\"id\":2}\n".to_vec());
        assert_eq!(read_message(&mut bytes).unwrap().unwrap()["id"], 1);
        assert_eq!(read_message(&mut bytes).unwrap().unwrap()["id"], 2);
        assert!(read_message(&mut bytes).unwrap().is_none());
        assert!(read_message(&mut std::io::Cursor::new(b"oops\n")).is_err());
    }
    #[test]
    fn spawn_links_child_but_send_message_does_not_invent_parent() {
        let mut task = fixture();
        project(
            &mut task,
            &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"item-1","type":"collabAgentToolCall","tool":"spawnAgent","senderThreadId":"root","receiverThreadIds":["child"],"agentsStates":{"child":{"status":"running","message":"working"}}}}}),
        );
        assert_eq!(
            task.nodes
                .iter()
                .find(|n| n.id == "child")
                .unwrap()
                .parent_id
                .as_deref(),
            Some("root")
        );
        project(
            &mut task,
            &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"item-2","type":"collabAgentToolCall","tool":"sendMessage","senderThreadId":"root","receiverThreadIds":["other"],"agentsStates":{"other":{"status":"running"}}}}}),
        );
        assert!(task
            .nodes
            .iter()
            .find(|n| n.id == "other")
            .unwrap()
            .parent_id
            .is_none());
    }
    #[test]
    fn real_subagent_activity_creates_and_completes_three_children() {
        let mut task = fixture();
        for name in ["product", "technical", "challenger"] {
            assert!(project(
                &mut task,
                &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":format!("spawn-{name}"),"type":"subAgentActivity","agentThreadId":name,"agentPath":format!("/root/{name}"),"kind":"started"}}})
            ));
        }
        assert_eq!(task.nodes.len(), 4);
        assert!(task
            .nodes
            .iter()
            .filter(|n| n.id != "root")
            .all(|n| n.parent_id.as_deref() == Some("root")));
        let complete = json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"completed-product","type":"subAgentActivity","agentThreadId":"product","agentPath":"/root/product","kind":"completed"}}});
        project(&mut task, &complete);
        let revision = task.revision;
        assert!(!project(&mut task, &complete));
        assert_eq!(task.revision, revision);
        project(
            &mut task,
            &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"input-product","type":"subAgentActivity","agentThreadId":"product","agentPath":"/root/product","kind":"interacted"}}}),
        );
        assert_eq!(
            task.nodes
                .iter()
                .find(|n| n.id == "product")
                .unwrap()
                .status,
            "completed"
        );
    }
    #[test]
    fn actual_history_recovers_three_children_and_preserves_delivery() {
        let mut history: Value =
            serde_json::from_str(include_str!("../fixtures/subagent-history.json")).unwrap();
        let mut task = fixture();
        task.thread_id = Some(text(&history, "id").into());
        task.turn_id = Some(text(&history["turns"][0], "id").into());
        task.root_node();
        task.status = "completed".into();
        task.nodes[0].status = "completed".into();
        task.nodes[0].agent_path = None; // Old snapshots have no path metadata.
        task.nodes[0].output = "root delivery".into();
        task.artifacts.push(Artifact {
            id: "doc".into(),
            name: "doc.md".into(),
            kind: "markdown".into(),
            content: "user edited delivery".into(),
            source_input_ids: vec![], created_at: 0,
        });
        history["turns"].as_array_mut().unwrap().push(serde_json::json!({"id":"other-turn","items":[{"type":"subAgentActivity","id":"other","agentThreadId":"outsider","agentPath":"/root/outsider","kind":"started"}]}));
        let saved = reconcile_agents(&task, &history).unwrap();
        assert_eq!(saved.nodes.len(), 4);
        assert!(saved.nodes.iter().all(|n| n.status == "completed"));
        assert!(saved
            .nodes
            .iter()
            .skip(1)
            .all(|n| n.parent_id == task.thread_id));
        assert_eq!(saved.nodes[0].output, "root delivery");
        assert_eq!(saved.artifacts[0].content, "user edited delivery");
        assert_eq!(saved.status, task.status);
        let again = reconcile_agents(&saved, &history).unwrap();
        assert_eq!(
            serde_json::to_value(again).unwrap(),
            serde_json::to_value(&saved).unwrap()
        );
        assert!(reconcile_agents(&task, &serde_json::json!({"id":"wrong","turns":[]})).is_err());
        history["turns"][0]["items"][0]["kind"] = Value::String("unknown-kind".into());
        assert!(reconcile_agents(&task, &history).is_err());
    }
    #[test]
    fn path_parents_resolve_when_arriving_late_and_legacy_ids_merge() {
        let mut task = fixture();
        let event = |id: &str, path: &str, kind: &str| serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":format!("{id}-{kind}"),"type":"subAgentActivity","agentThreadId":id,"agentPath":path,"kind":kind}}});
        project(
            &mut task,
            &event("reviewer", "/root/product/reviewer", "started"),
        );
        assert!(task
            .nodes
            .iter()
            .find(|n| n.id == "reviewer")
            .unwrap()
            .parent_id
            .is_none());
        project(
            &mut task,
            &serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"legacy","type":"collabAgentToolCall","tool":"spawnAgent","senderThreadId":"root","agentsStates":{"product":{"status":"running"}}}}}),
        );
        project(&mut task, &event("product", "/root/product", "started"));
        assert_eq!(task.nodes.len(), 3);
        assert_eq!(
            task.nodes
                .iter()
                .find(|n| n.id == "reviewer")
                .unwrap()
                .parent_id
                .as_deref(),
            Some("product")
        );
        assert!(!project(&mut task, &event("root", "/root/root", "started")));
        assert!(!project(
            &mut task,
            &event("bad", "/root/product/../bad", "started")
        ));
        let mut stale = event("stale", "/root/stale", "started");
        stale["params"]["turnId"] = Value::String("old-turn".into());
        assert!(!project(&mut task, &stale));
    }
    #[test]
    fn old_started_cannot_regress_completion_after_display_log_rolls_over() {
        let mut task = fixture();
        let start = json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"start-child","type":"subAgentActivity","agentThreadId":"child","agentPath":"/root/child","kind":"started"}}});
        project(&mut task, &start);
        project(
            &mut task,
            &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"complete-child","type":"subAgentActivity","agentThreadId":"child","agentPath":"/root/child","kind":"completed"}}}),
        );
        for _ in 0..110 {
            task.event("other activity", "system", "工作台");
        }
        assert!(!project(&mut task, &start));
        assert_eq!(
            task.nodes.iter().find(|n| n.id == "child").unwrap().status,
            "completed"
        );
    }
    #[test]
    fn activity_capacity_reports_incomplete_projection_without_evicting_ids() {
        let mut task = fixture();
        let start = json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn-1","item":{"id":"start-child","type":"subAgentActivity","agentThreadId":"child","agentPath":"/root/child","kind":"started"}}});
        project(&mut task, &start);
        task.agent_activity_ids
            .extend((0..MAX_AGENT_ACTIVITIES - 1).map(|i| format!("seen-{i}")));
        let ids = task.agent_activity_ids.clone();
        assert!(!project(&mut task, &start));
        let mut new = start.clone();
        new["params"]["item"]["id"] = Value::String("new-activity".into());
        assert!(project(&mut task, &new));
        assert_eq!(task.agent_activity_ids, ids);
        assert!(task.events.last().unwrap().text.contains("不完整"));
        let history: Value =
            serde_json::from_str(include_str!("../fixtures/subagent-history.json")).unwrap();
        task.status = "completed".into();
        task.thread_id = Some(text(&history, "id").into());
        task.turn_id = Some(text(&history["turns"][0], "id").into());
        assert!(reconcile_agents(&task, &history).is_err());
    }
    #[test]
    fn recovery_invalidates_pending_approvals() {
        let mut task = fixture();
        task.status = "approval".into();
        task.approvals.push(Approval {
            id: "a".into(),
            request_id: "1".into(),
            run_id: task.run_id.clone().unwrap(),
            turn_id: "turn-1".into(),
            title: "Ask".into(),
            description: "".into(),
            kind: "input".into(),
            question_ids: vec![],
            questions: vec![],
            question_error: None,
        });
        task.recover();
        assert_eq!(task.status, "unknown");
        assert!(task.approvals.is_empty());
    }
    fn details_fixture() -> (Task, AgentHistory) {
        let root: Value =
            serde_json::from_str(include_str!("../fixtures/subagent-history.json")).unwrap();
        let mut task = fixture();
        task.status = "completed".into();
        task.thread_id = Some(text(&root, "id").into());
        task.turn_id = Some(text(&root["turns"][0], "id").into());
        task.root_node();
        task.nodes[0].output = "root delivery".into();
        let mut details = Vec::new();
        for n in observed_agent_nodes(&task, &root).unwrap() {
            let child = json!({"id":n.id,"parentThreadId":n.parent_id,"source":{"subAgent":{"thread_spawn":{"agent_path":n.agent_path}}},"turns":[
                {"id":"round-one","items":[{"type":"agentMessage","id":"comment","phase":"commentary","text":"working"},{"type":"agentMessage","id":"final-one","phase":"final_answer","text":"第一轮回答"}]},
                {"id":"round-two","items":[{"type":"agentMessage","id":"final-two","phase":"final_answer","text":"第二轮回答"},{"type":"agentMessage","id":"final-two","phase":"final_answer","text":"第二轮回答"}]}]});
            details.push(AgentRead {
                id: n.id,
                result: Ok(child),
            });
        }
        (task, AgentHistory { root, details })
    }
    #[test]
    fn child_history_merges_rounds_once_preserves_root_and_is_idempotent() {
        let (task, history) = details_fixture();
        let saved = reconcile_agent_history(&task, &history).unwrap();
        assert_eq!(saved.status, task.status);
        assert_eq!(saved.nodes[0], task.nodes[0]);
        for n in saved.nodes.iter().skip(1) {
            assert_eq!(n.detail_turn_count, 2);
            assert_eq!(n.output.matches("第二轮回答").count(), 1);
            assert!(!n.output.contains("working"));
            assert!(!n.output_truncated);
            assert_eq!(n.status, "completed");
            assert_eq!(n.model, "实际模型未返回");
        }
        assert_eq!(
            reconcile_agent_history(&saved, &history).unwrap().revision,
            saved.revision
        );
    }
    #[test]
    fn failed_empty_and_foreign_child_reads_preserve_latest_output() {
        let (task, mut history) = details_fixture();
        let mut saved = reconcile_agent_history(&task, &history).unwrap();
        saved.nodes[1].output = "newest child answer".into();
        history.details[0].result = Err("读取超时，尚未同步".into());
        history.details[1].result.as_mut().unwrap()["turns"] = json!([]);
        history.details[2].result.as_mut().unwrap()["parentThreadId"] = json!("foreign");
        let next = reconcile_agent_history(&saved, &history).unwrap();
        for (a, b) in saved.nodes.iter().zip(&next.nodes).skip(1) {
            assert_eq!(a.output, b.output);
            assert!(b.detail_notice.is_some());
        }
        history.details[0].result = Ok(json!({"id":"foreign"}));
        let next = reconcile_agent_history(&next, &history).unwrap();
        assert_eq!(next.nodes[1].output, "newest child answer");
        assert!(next.nodes[1]
            .detail_notice
            .as_deref()
            .unwrap()
            .contains("不匹配"));
        for field in ["items", "id"] {
            let (_, mut malformed) = details_fixture();
            malformed.details[0].result.as_mut().unwrap()["turns"][0][field] = Value::Null;
            let next = reconcile_agent_history(&saved, &malformed).unwrap();
            assert_eq!(next.nodes[1].output, "newest child answer");
            assert!(next.nodes[1]
                .detail_notice
                .as_deref()
                .unwrap()
                .contains("不完整"));
        }
        for field in ["text", "id"] {
            for invalid in [None, Some(Value::Null), Some(json!(42))] {
                let (_, mut malformed) = details_fixture();
                let message = malformed.details[0].result.as_mut().unwrap()["turns"][1]["items"][0]
                    .as_object_mut()
                    .unwrap();
                if let Some(value) = invalid {
                    message.insert(field.into(), value);
                } else {
                    message.remove(field);
                }
                let next = reconcile_agent_history(&saved, &malformed).unwrap();
                assert_eq!(next.nodes[1].output, "newest child answer");
                assert_eq!(
                    next.nodes[1].detail_turn_count,
                    saved.nodes[1].detail_turn_count
                );
                assert!(next.nodes[1]
                    .detail_notice
                    .as_deref()
                    .unwrap()
                    .contains("不完整"));
            }
        }
    }
    #[test]
    fn child_output_unicode_cap_is_visible_and_path_must_match_exactly() {
        let (task, mut history) = details_fixture();
        history.details[0].result.as_mut().unwrap()["turns"][0]["items"][1]["text"] =
            json!("中".repeat(70000));
        history.details[1].result.as_mut().unwrap()["source"]["subAgent"]["thread_spawn"]
            ["agent_path"] = json!("/root/technical-other");
        let saved = reconcile_agent_history(&task, &history).unwrap();
        assert_eq!(saved.nodes[1].output.chars().count(), 64000);
        assert!(saved.nodes[1].output_truncated);
        assert!(saved.nodes[1].detail_notice.is_some());
        assert!(saved.nodes[2].output.is_empty());
        assert!(saved.nodes[2]
            .detail_notice
            .as_deref()
            .unwrap()
            .contains("路径"));
    }
    #[test]
    fn reader_rejects_truncated_and_oversized_messages() {
        assert!(read_message(&mut std::io::Cursor::new(b"{\"id\":1}")).is_err());
        assert!(read_message(&mut std::io::Cursor::new(vec![b'x'; MAX_MESSAGE + 2])).is_err());
    }
}

pub fn writing_thread(task:&Task,mut request:Value)->Value{
 if let Some(w)=&task.code_workspace{
  request["params"]["cwd"]=serde_json::json!(w.directory);
  request["params"]["sandbox"]=serde_json::json!("workspace-write");
  request["params"]["config"]=serde_json::json!({"sandbox_workspace_write.writable_roots":[w.directory],"sandbox_workspace_write.network_access":false,"sandbox_workspace_write.exclude_slash_tmp":true,"sandbox_workspace_write.exclude_tmpdir_env_var":true,"mcp_servers":{}});
  request["params"]["developerInstructions"]=serde_json::json!("只允许修改本任务分配的独立工作区。不得修改共享 Git 元数据、原项目目录、访问网络或调用外部写服务；Git 快照、集成与合并由平台管理。所有分工由工作台组织，不另行创建写入子会话。普通说明留在对话，成果按任务提交协议。权限提升必须拒绝。");
 }request
}
pub fn writing_turn(task:&Task,mut request:Value)->Value{if let Some(w)=&task.code_workspace{request["params"]["cwd"]=serde_json::json!(w.directory);request["params"]["sandboxPolicy"]=serde_json::json!({"type":"workspaceWrite","writableRoots":[w.directory],"networkAccess":false,"excludeSlashTmp":true,"excludeTmpdirEnvVar":true});}request}
pub fn validate_writing_response(task:&Task,result:&Value)->Result<(),String>{if let Some(w)=&task.code_workspace{let s=&result["sandbox"];if result["cwd"]!=w.directory||s["type"]!="workspaceWrite"||s["writableRoots"]!=serde_json::json!([w.directory])||s["networkAccess"]!=false||s["excludeSlashTmp"]!=true||s["excludeTmpdirEnvVar"]!=true{return Err("CLI 未确认受限工作区沙箱，已拒绝写任务启动".into())}}Ok(())}
pub fn empty_mcp(result:&Value)->bool{result["data"].as_array().is_some_and(|a|a.is_empty())&&result.get("nextCursor").is_some_and(Value::is_null)}
