use crate::model::{now, Task};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ITEM_LIMIT: usize = 256;
pub const TEXT_LIMIT: usize = 256000;
pub const ITEM_TEXT_LIMIT: usize = 16000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatItem {
    pub run_id: String,
    pub thread_id: String,
    pub item_id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub text: String,
    pub final_answer: bool,
    pub truncated: bool,
    pub exit_code: Option<i64>,
    pub at: u64,
    #[serde(default)] pub source_input_ids: Vec<String>,
}

impl Task {
    pub fn settle_conversation(&mut self) {
        for item in &mut self.conversation {
            if item.status == "running" { item.status = "unknown".into(); }
        }
    }
}

// Only public root-agent messages and fixed tool metadata enter storage.
// Commands, arguments, output, patches, URLs and reasoning are never persisted here.
pub fn project(task: &mut Task, method: &str, params: &Value) -> bool {
    let thread = params["threadId"].as_str().unwrap_or("");
    if Some(thread) != task.thread_id.as_deref() { return false; }
    let delta = method == "item/agentMessage/delta";
    let completed = method == "item/completed";
    if !delta && !completed && method != "item/started" { return false; }
    let item = &params["item"];
    let (kind, title) = match if delta { "agentMessage" } else { item["type"].as_str().unwrap_or("") } {
        "agentMessage" => ("assistant", ""),
        "commandExecution" => ("tool", "执行命令"),
        "fileChange" => ("tool", "更改文件"),
        "webSearch" => ("tool", "搜索网页"),
        "mcpToolCall" => ("tool", "调用工具"),
        "collabAgentToolCall" => ("tool", match item["tool"].as_str() {
            Some("spawnAgent") => "创建子 Agent",
            Some("sendMessage") => "发送协作消息",
            Some("wait") => "等待子 Agent",
            Some("closeAgent") => "结束子 Agent",
            _ => "Agent 协作",
        }),
        _ => return false,
    };
    let id = if delta { &params["itemId"] } else { &item["id"] }.as_str().unwrap_or("");
    let run = task.run_id.as_deref().unwrap_or("");
    if id.is_empty() || id.len() > 128 || run.is_empty() || run.len() > 100 || thread.len() > 128 { return false; }
    let index = task.conversation.iter().position(|x| x.run_id == run && x.thread_id == thread && x.item_id == id);
    if index.is_some() && method == "item/started" { return false; }
    if index.is_none() {
        let body = if delta { params["delta"].as_str() } else { item["text"].as_str() };
        let full = kind == "assistant" && body.is_some_and(|s| !s.is_empty()) && task.conversation.iter().map(|x| x.text.chars().count()).sum::<usize>() >= TEXT_LIMIT;
        if task.conversation.len() >= ITEM_LIMIT || full {
            task.conversation_truncated = true;
            task.revision += 1;
            return true;
        }
        task.conversation.push(ChatItem { run_id: run.into(), thread_id: thread.into(), item_id: id.into(), kind: kind.into(), title: title.into(), status: "running".into(), text: String::new(), final_answer: false, truncated: false, exit_code: None, at: now(), source_input_ids: vec![] });
    }
    let index = index.unwrap_or(task.conversation.len() - 1);
    let previous = &task.conversation[index];
    if previous.kind != kind || (previous.status != "running" && !completed) { return false; }
    let used: usize = task.conversation.iter().enumerate().filter(|(i, _)| *i != index).map(|(_, x)| x.text.chars().count()).sum();
    let available = ITEM_TEXT_LIMIT.min(TEXT_LIMIT.saturating_sub(used));
    let entry = &mut task.conversation[index];
    if kind == "assistant" {
        let incoming = if delta { params["delta"].as_str() } else { item["text"].as_str() };
        if let Some(value) = incoming {
            if !delta { entry.text.clear(); entry.truncated = false; }
            let remaining = available.saturating_sub(entry.text.chars().count());
            entry.text.extend(value.chars().take(remaining));
            if value.chars().count() > remaining { entry.truncated = true; task.conversation_truncated = true; }
        }
        if item["phase"].as_str() == Some("final_answer") { entry.final_answer = true; }
    }
    if completed {
        entry.exit_code = item["exitCode"].as_i64();
        entry.status = if matches!(item["status"].as_str(), Some("failed" | "declined" | "interrupted" | "cancelled")) || entry.exit_code.is_some_and(|code| code != 0) { "failed" } else { "completed" }.into();
    }
    task.revision += 1;
    true
}

// ACP has a session identity but no public message ID. One stable root item per run.
pub fn qoder_output(task:&mut Task,text:&str,completed:bool,truncated:bool) {
    let Some(session)=task.session_ref.as_ref() else{return};let Some(run)=task.run_id.as_ref() else{return};
    let key=(run.clone(),session.id.clone());let index=task.conversation.iter().position(|x|x.run_id==key.0&&x.thread_id==key.1&&x.item_id=="qoder-response");
    if index.is_none(){if task.conversation.len()>=ITEM_LIMIT{task.conversation_truncated=true;return}task.conversation.push(ChatItem{run_id:key.0,thread_id:key.1,item_id:"qoder-response".into(),kind:"assistant".into(),title:"".into(),status:"running".into(),text:String::new(),final_answer:false,truncated:false,exit_code:None,at:now(),source_input_ids:vec![]});}
    let index=index.unwrap_or(task.conversation.len()-1);let used=task.conversation.iter().enumerate().filter(|(i,_)|*i!=index).map(|(_,x)|x.text.chars().count()).sum::<usize>();
    let limit=ITEM_TEXT_LIMIT.min(TEXT_LIMIT.saturating_sub(used));let item=&mut task.conversation[index];item.text=text.chars().take(limit).collect();item.truncated=truncated||text.chars().count()>limit;item.status=if completed{"completed"}else{"running"}.into();item.final_answer=completed;
    if item.truncated{task.conversation_truncated=true;}
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn task() -> Task { let mut t = Task::new("test".into(), "goal".into(), "research".into()); t.thread_id = Some("root".into()); t.turn_id = Some("turn".into()); t }
    fn delta(id: &str, text: &str) -> Value { json!({"method":"item/agentMessage/delta","params":{"threadId":"root","turnId":"turn","itemId":id,"delta":text}}) }
    fn complete(id: &str, text: &str) -> Value { json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"type":"agentMessage","id":id,"phase":"final_answer","text":text}}}) }
    #[test]
    fn lifecycle_keeps_messages_and_replaces_snapshots_without_repeated_deltas() {
        let mut t = task();
        crate::protocol::project(&mut t, &delta("a", "progress"));
        crate::protocol::project(&mut t, &json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"type":"agentMessage","id":"a","text":""}}}));
        assert_eq!(t.conversation[0].text, "progress");
        crate::protocol::project(&mut t, &complete("a", "first answer"));
        crate::protocol::project(&mut t, &complete("a", "first answer"));
        crate::protocol::project(&mut t, &delta("a", "late"));
        crate::protocol::project(&mut t, &complete("b", "second answer"));
        assert_eq!(t.conversation.len(), 2);
        assert_eq!(t.conversation[0].text, "first answer");
        let mut stale = delta("b", "stale"); stale["params"]["turnId"] = json!("old");
        assert!(!crate::protocol::project(&mut t, &stale));
        t.status = "completed".into();
        assert!(!crate::protocol::project(&mut t, &delta("b", "after turn")));
        assert_eq!(t.conversation[1].text, "second answer");
    }
    #[test]
    fn tool_metadata_does_not_leak_payloads() {
        let mut t = task();
        crate::protocol::project(&mut t, &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"type":"commandExecution","id":"cmd","command":"secret value","aggregatedOutput":"secret output","exitCode":2}}}));
        assert_eq!(t.conversation[0].status, "failed");
        assert_eq!(t.conversation[0].exit_code, Some(2));
        assert!(!serde_json::to_string(&t.conversation).unwrap().contains("secret"));
        crate::protocol::project(&mut t, &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"type":"commandExecution","id":"declined","status":"declined"}}}));
        assert_eq!(t.conversation[1].status, "failed");
        let count = t.conversation.len();
        crate::protocol::project(&mut t, &json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"type":"reasoning","id":"r","text":"private"}}}));
        assert_eq!(t.conversation.len(), count);
    }
    #[test]
    fn budgets_unicode_recovery_and_continuation_are_bounded() {
        let mut t = task();
        crate::protocol::project(&mut t, &delta("big", &"🪐".repeat(ITEM_TEXT_LIMIT + 1)));
        assert_eq!(t.conversation[0].text.chars().count(), ITEM_TEXT_LIMIT);
        assert!(t.conversation[0].truncated && t.conversation_truncated);
        for i in 1..ITEM_LIMIT { crate::protocol::project(&mut t, &delta(&i.to_string(), &"中".repeat(1200))); }
        assert_eq!(t.conversation.len(), 201);
        for i in 0..(ITEM_LIMIT - 201) {
            crate::protocol::project(&mut t, &json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"type":"commandExecution","id":format!("metadata-{i}")}}}));
        }
        assert_eq!(t.conversation.len(), ITEM_LIMIT);
        assert!(t.conversation.iter().map(|x| x.text.chars().count()).sum::<usize>() <= TEXT_LIMIT);
        crate::protocol::project(&mut t, &complete("overflow", "not added"));
        assert_eq!(t.conversation.len(), ITEM_LIMIT);
        crate::protocol::project(&mut t, &complete("big", "done"));
        assert_eq!(t.conversation[0].status, "completed");
        t.recover();
        assert_eq!(t.status, "unknown");
        assert_eq!(t.conversation[1].status, "unknown");
        t.status = "interrupted".into();
        let (mut next, _) = t.continued(t.revision, &t.run_id, &t.turn_id, "continue").unwrap();
        assert_eq!(next.conversation.len(), ITEM_LIMIT);
        next.begin_run(false);
        assert!(next.conversation.is_empty());
        let mut legacy = serde_json::to_value(t).unwrap(); legacy.as_object_mut().unwrap().remove("conversation"); legacy.as_object_mut().unwrap().remove("conversationTruncated");
        assert!(serde_json::from_value::<Task>(legacy).unwrap().conversation.is_empty());
    }
}
