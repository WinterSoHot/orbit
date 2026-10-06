use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub const OUTPUT_LIMIT: usize = 64000;
pub const ANSWER_LIMIT: usize = 20;
const ANSWER_SEPARATOR: &str = "\n\n---\n\n";
const TRUNCATED_NOTICE: &str = "\n\n> 输出超过大小或条数上限，内容已截断。";
pub fn answer_budget() -> usize {
    OUTPUT_LIMIT
        - (ANSWER_LIMIT - 1) * ANSWER_SEPARATOR.chars().count()
        - TRUNCATED_NOTICE.chars().count()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub name: String,
    pub role: String,
    pub model: String,
    pub status: String,
    pub summary: String,
    pub output: String,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub agent_path: Option<String>,
    #[serde(default)]
    pub detail_notice: Option<String>,
    #[serde(default)]
    pub detail_turn_count: usize,
    #[serde(default)]
    pub output_truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub id: String,
    pub at: u64,
    pub kind: String,
    pub agent: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputQuestion {
    pub id: String,
    #[serde(default)]
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Option<Vec<InputOption>>,
    #[serde(default)]
    pub is_other: bool,
    #[serde(default)]
    pub is_secret: bool,
}
pub fn parse_input_questions(raw: &serde_json::Value) -> Result<Vec<InputQuestion>, String> {
    let questions: Vec<InputQuestion> = serde_json::from_value(raw.clone())
        .map_err(|_| "澄清问题数据不完整，请重新发起请求".to_owned())?;
    let mut ids = std::collections::HashSet::new();
    if questions.is_empty() || questions.iter().any(|q| {
        q.id.trim().is_empty() || q.question.trim().is_empty() || !ids.insert(q.id.clone())
            || q.options.as_ref().is_some_and(|options| options.iter().any(|o| o.label.trim().is_empty() || o.label.chars().count() > 2000))
    }) {
        return Err("澄清问题存在缺失、重复编号或无效选项，请重新发起请求".into());
    }
    Ok(questions)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub request_id: String,
    pub run_id: String,
    pub turn_id: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub question_ids: Vec<String>,
    #[serde(default)]
    pub questions: Vec<InputQuestion>,
    #[serde(default)]
    pub question_error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub content: String,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerItem {
    pub id: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Direction {
    pub id: String,
    pub run_id: String,
    pub turn_id: String,
    pub text: String,
    pub status: String,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supplement {
    #[serde(default)]
    pub source_thread_id: Option<String>,
    pub run_id: String,
    pub previous_turn_id: String,
    pub text: String,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QueueAction {
    Start,
    Continue { text: String, #[serde(rename="runId")] run_id: Option<String>, #[serde(rename="turnId")] turn_id: Option<String> },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRequest {
    pub request_id: String, pub order: u64, pub next_run_id: String,
    pub state: String, pub action: QueueAction, pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Acceptance { pub run_id: Option<String>, pub turn_id: Option<String>, pub artifact_ids: Vec<String> }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    #[serde(default)]
    pub queue: Option<QueueRequest>,
    #[serde(default)]
    pub conversation: Vec<crate::conversation::ChatItem>,
    #[serde(default)]
    pub conversation_truncated: bool,
    #[serde(default)]
    pub acceptance: Option<Acceptance>,
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub scene: String,
    #[serde(default = "crate::executor::default_provider")]
    pub provider: String,
    #[serde(default)]
    pub session_ref: Option<crate::executor::SessionRef>,
    #[serde(default)]
    pub capabilities: Option<crate::executor::Capabilities>,
    pub status: String,
    #[serde(default)]
    pub archived: bool,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub run_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub revision: u64,
    #[serde(default)]
    pub executor_revision: Option<u64>,
    #[serde(default)]
    pub agent_activity_ids: Vec<String>,
    pub phase: u32,
    pub nodes: Vec<Node>,
    pub events: Vec<Activity>,
    pub approvals: Vec<Approval>,
    pub artifacts: Vec<Artifact>,
    pub tokens: Option<u64>,
    #[serde(default)]
    pub supplements: Vec<Supplement>,
    #[serde(default)]
    pub directions: Vec<Direction>,
    #[serde(default)]
    pub answer_items: Vec<AnswerItem>,
}
impl Task {
    pub fn new(title: String, prompt: String, scene: String) -> Self {
        Self {
            queue: None, acceptance: None,
            conversation: vec![], conversation_truncated: false,
            id: uuid::Uuid::new_v4().to_string(),
            title,
            prompt,
            scene,
            provider: "codex".into(),
            session_ref: None,
            capabilities: None,
            status: "running".into(),
            archived: false,
            created_at: now(),
            started_at: Some(now()),
            finished_at: None,
            run_id: Some(uuid::Uuid::new_v4().to_string()),
            thread_id: None,
            turn_id: None,
            revision: 1,
            executor_revision: None,
            agent_activity_ids: vec![],
            phase: 0,
            nodes: vec![],
            events: vec![],
            approvals: vec![],
            artifacts: vec![],
            tokens: None,
            supplements: vec![],
            directions: vec![],
            answer_items: vec![],
        }
    }
    pub fn begin_run(&mut self, preserve: bool) {
        self.acceptance = None;
        self.status = "running".into();
        self.started_at = Some(now());
        self.finished_at = None;
        self.run_id = Some(uuid::Uuid::new_v4().to_string());
        self.turn_id = None;
        self.nodes.clear();
        self.answer_items.clear();
        self.agent_activity_ids.clear();
        self.approvals.clear();
        self.tokens = None;
        self.settle_conversation();
        if !preserve {
            self.conversation.clear();
            self.conversation_truncated = false;
            self.thread_id = None;
            self.session_ref = None;
            self.artifacts.clear();
            self.supplements.clear();
            self.directions.clear();
        }
        self.event(
            &format!(
                "正在连接本机 {} 执行器",
                crate::executor::name(&self.provider)
            ),
            "system",
            "工作台",
        );
    }
    pub fn continued(
        &self,
        revision: u64,
        run: &Option<String>,
        turn: &Option<String>,
        text: &str,
    ) -> Result<(Self, String), String> {
        if self.revision != revision || &self.run_id != run || &self.turn_id != turn {
            return Err("任务已更新，请核对当前交付后重新发送".into());
        }
        if self.archived
            || !self.terminal()
            || !self.can_resume()
            || self.run_id.is_none()
        {
            return Err("仅未归档且已结束、具有可恢复会话的任务可以继续；未知状态需先核对".into());
        }
        if text.trim().is_empty() || text.chars().count() > 2000 {
            return Err("补充信息应为 1–2000 字".into());
        }
        if self.artifacts.len() >= 10 || self.supplements.len() >= 10 {
            return Err("已达到 10 份交付或 10 次补充上限；请新建任务，历史将保留".into());
        }
        let anchor = self
            .turn_id
            .as_ref()
            .or_else(|| self.supplements.last().map(|s| &s.previous_turn_id))
            .filter(|id| !id.is_empty())
            .ok_or("缺少可核对的上一轮标识，请新建任务")?
            .clone();
        let mut next = self.clone();
        next.begin_run(true);
        next.supplements.push(Supplement {
            source_thread_id: None,
            run_id: next.run_id.clone().unwrap(),
            previous_turn_id: anchor.clone(),
            text: text.trim().into(),
            created_at: now(),
        });
        next.event("补充信息已保存，正在续接原会话", "system", "工作台");
        Ok((next, anchor))
    }
    // Platform bookkeeping must not import the UI clock into the actor's execution clock.
    pub fn merge_platform(&mut self, saved: &Self) {
        if self.id != saved.id || self.run_id != saved.run_id || self.provider != saved.provider { return; }
        self.queue=saved.queue.clone();self.acceptance=saved.acceptance.clone();self.archived=saved.archived;
        if self.terminal() && saved.terminal() && self.turn_id==saved.turn_id {
            self.artifacts=saved.artifacts.clone();
        }
    }
    pub fn actor_snapshot(mut self) -> Self {
        self.revision=self.executor_revision.unwrap_or(self.revision);self
    }
    pub fn accepted(&self) -> bool {
        self.status == "completed" && !self.artifacts.is_empty() && self.acceptance.as_ref().is_some_and(|a|
            a.run_id == self.run_id && a.turn_id == self.turn_id &&
            a.artifact_ids == self.artifacts.iter().map(|a| a.id.clone()).collect::<Vec<_>>())
    }
    pub fn terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed" | "interrupted")
    }
    pub fn can_resume(&self) -> bool {
        if let Some(session) = &self.session_ref {
            if session.provider != self.provider || session.id.is_empty() {
                return false;
            }
        }
        if self.provider == "codex" {
            return self.thread_id.as_deref().is_some_and(|id| !id.is_empty())
                && self.capabilities.as_ref().is_none_or(|c| c.resume);
        }
        self.session_ref.is_some() && self.capabilities.as_ref().is_some_and(|c| c.resume)
    }
    pub fn recover(&mut self) {
        if matches!(self.status.as_str(), "running" | "approval" | "cancelling") {
            self.status = "unknown".into();
            self.unconfirm_directions();
            self.approvals.clear();
            self.revision += 1;
            for node in &mut self.nodes {
                if matches!(node.status.as_str(), "running" | "approval" | "cancelling") {
                    node.status = "unknown".into();
                }
            }
        }
    }
    pub fn unconfirm_directions(&mut self) {
        self.settle_conversation();
        for direction in &mut self.directions {
            if direction.status == "pending" {
                direction.status = "unknown".into();
            }
        }
    }
    pub fn collect_answer(&mut self, id: &str, value: &str) {
        use std::hash::{Hash, Hasher};
        let id = if id.is_empty() {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            value.hash(&mut hash);
            format!("legacy-{:x}", hash.finish())
        } else {
            id.chars().take(200).collect()
        };
        let existing = self.answer_items.iter().position(|item| item.id == id);
        if existing.is_none() && self.answer_items.len() >= ANSWER_LIMIT {
            if let Some(root) = self.nodes.first_mut() {
                root.output_truncated = true;
            }
            return;
        }
        let used: usize = self
            .answer_items
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != existing)
            .map(|(_, item)| item.text.chars().count())
            .sum();
        let remaining = answer_budget().saturating_sub(used);
        if existing.is_none() && remaining == 0 {
            if let Some(root) = self.nodes.first_mut() {
                root.output_truncated = true;
            }
            self.refresh_answer();
            return;
        }
        let text: String = value.chars().take(remaining).collect();
        if value.chars().count() > remaining {
            if let Some(root) = self.nodes.first_mut() {
                root.output_truncated = true;
            }
        }
        let item = AnswerItem { id, text };
        if let Some(i) = existing {
            self.answer_items[i] = item;
        } else {
            self.answer_items.push(item);
        }
        self.refresh_answer();
    }
    pub fn refresh_answer(&mut self) {
        if self.answer_items.is_empty() {
            return;
        }
        if let Some(root) = self.nodes.first_mut() {
            root.output = self
                .answer_items
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>()
                .join(ANSWER_SEPARATOR);
            if root.output.chars().count() > OUTPUT_LIMIT {
                root.output_truncated = true;
            }
            if root.output_truncated {
                root.output = root
                    .output
                    .chars()
                    .take(OUTPUT_LIMIT - TRUNCATED_NOTICE.chars().count())
                    .collect();
                root.output.push_str(TRUNCATED_NOTICE);
            }
        }
    }
    pub fn event(&mut self, text: &str, kind: &str, agent: &str) {
        self.revision += 1;
        self.events.push(Activity {
            id: format!("{}-{}", self.id, self.revision),
            at: now(),
            kind: kind.into(),
            agent: agent.into(),
            text: text.chars().take(1500).collect(),
        });
        if self.events.len() > 100 {
            self.events.remove(0);
        }
    }
    pub fn root_node(&mut self) {
        if self.provider == "codex" && self.thread_id.is_some() {
            self.session_ref = Some(crate::executor::SessionRef {
                provider: self.provider.clone(),
                protocol: "codex-app-server".into(),
                id: self.thread_id.clone().unwrap(),
                cwd: None,
                metadata: serde_json::Value::Null,
                extra: Default::default(),
            });
            self.capabilities = Some(crate::executor::Capabilities::codex());
        }
        if let Some(id) = self
            .thread_id
            .as_ref()
            .or_else(|| self.session_ref.as_ref().map(|s| &s.id))
        {
            if !self.nodes.iter().any(|n| &n.id == id) {
                self.nodes.insert(
                    0,
                    Node {
                        id: id.clone(),
                        name: crate::executor::name(&self.provider).into(),
                        role: "主 Agent".into(),
                        model: "CLI 默认模型".into(),
                        status: "running".into(),
                        summary: "正在执行你的任务".into(),
                        output: String::new(),
                        parent_id: None,
                        agent_path: Some("/root".into()),
                        detail_notice: None,
                        detail_turn_count: 0,
                        output_truncated: false,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod continuation_tests {
    use super::*;
    #[test]
    fn clarification_preserves_questions_and_options() {
        let raw=serde_json::json!({"id":"a","requestId":"1","runId":"r","turnId":"t","title":"Ask","description":"Scope?","kind":"input","questionIds":["scope"],"questions":[{"id":"scope","header":"Scope","question":"Local or cloud?","isOther":true,"isSecret":false,"options":[{"label":"Local","description":"Offline"}]}]});
        let approval:Approval=serde_json::from_value(raw).unwrap();
        let saved=serde_json::to_value(approval).unwrap();
        assert_eq!(saved["questions"][0]["options"][0]["label"],"Local");
        assert_eq!(saved["questions"][0]["isOther"],true);
        let mut legacy=saved;
        legacy.as_object_mut().unwrap().remove("questions");
        legacy.as_object_mut().unwrap().remove("questionError");
        let legacy:Approval=serde_json::from_value(legacy).unwrap();
        assert!(legacy.questions.is_empty()&&legacy.question_error.is_none());
        let free=parse_input_questions(&serde_json::json!([{"id":"goal","question":"Goal?","options":null,"isSecret":true}])).unwrap();
        assert!(free[0].is_secret&&free[0].options.is_none()&&!free[0].is_other);
        assert!(parse_input_questions(&serde_json::json!([{"id":"x","question":"One?"},{"id":"x","question":"Two?"}])).is_err());
        assert!(parse_input_questions(&serde_json::json!([{"id":"x","question":"One?"},{"id":"y","options":[]}])).is_err());
        assert!(parse_input_questions(&serde_json::json!([])).is_err());
    }
    #[test]
    fn continuation_preserves_edited_versions_and_rejects_stale_or_full_tasks() {
        let mut task = Task::new("test".into(), "original question".into(), "research".into());
        task.status = "completed".into();
        task.thread_id = Some("thread".into());
        task.turn_id = Some("first-turn".into());
        task.artifacts.push(Artifact {
            id: "first-result".into(),
            name: "first.md".into(),
            kind: "markdown".into(),
            content: "edited delivery".into(),
            created_at: 0,
        });
        let run = task.run_id.clone();
        assert!(task
            .continued(task.revision + 1, &run, &task.turn_id, "more")
            .is_err());
        let (next, anchor) = task
            .continued(task.revision, &run, &task.turn_id, "  more information  ")
            .unwrap();
        assert_eq!(anchor, "first-turn");
        assert_eq!(next.thread_id.as_deref(), Some("thread"));
        assert_ne!(next.run_id, task.run_id);
        assert_eq!(next.turn_id, None);
        assert_eq!(next.prompt, "original question");
        assert_eq!(next.artifacts[0].content, "edited delivery");
        assert_eq!(next.supplements[0].text, "more information");
        assert!(next
            .continued(next.revision, &next.run_id, &next.turn_id, "duplicate")
            .is_err());
        let mut failed = next;
        failed.status = "failed".into();
        let (_, retry_anchor) = failed
            .continued(
                failed.revision,
                &failed.run_id,
                &failed.turn_id,
                "retry explicitly",
            )
            .unwrap();
        assert_eq!(retry_anchor, "first-turn");
        task.archived = true;
        assert!(task
            .continued(task.revision, &run, &task.turn_id, "more")
            .is_err());
        task.archived = false;
        task.artifacts = vec![task.artifacts[0].clone(); 10];
        assert!(task
            .continued(task.revision, &run, &task.turn_id, "more")
            .is_err());
    }
}

#[cfg(test)]
mod platform_merge_tests {
    use super::*;
    #[test]
    fn platform_refresh_preserves_actor_clock_and_completed_output() {
        let mut actor=Task::new("x".into(),"goal".into(),"research".into());actor.turn_id=Some("turn".into());
        let mut platform=actor.clone();platform.revision+=10;
        actor.status="completed".into();actor.artifacts.push(Artifact{id:"fresh".into(),name:"new.md".into(),kind:"markdown".into(),content:"fresh".into(),created_at:0});actor.event("completed","system","agent");
        let clock=actor.revision;actor.merge_platform(&platform);assert_eq!(actor.status,"completed");assert_eq!(actor.revision,clock);assert_eq!(actor.artifacts[0].content,"fresh");
        platform.status="completed".into();platform.turn_id=Some("other".into());actor.merge_platform(&platform);assert_eq!(actor.artifacts[0].content,"fresh");
        platform.turn_id=actor.turn_id.clone();platform.artifacts=actor.artifacts.clone();platform.artifacts[0].content="user edit".into();actor.merge_platform(&platform);assert_eq!(actor.artifacts[0].content,"user edit");assert_eq!(actor.revision,clock);
    }
}
