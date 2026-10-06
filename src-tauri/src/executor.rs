use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Capabilities {
    pub resume: bool,
    pub steer: bool,
    pub interrupt: bool,
    pub input: bool,
    pub agent_history: bool,
}
impl Capabilities {
    pub fn codex() -> Self {
        Self {
            resume: true,
            steer: true,
            interrupt: true,
            input: true,
            agent_history: true,
        }
    }
    pub fn acp(load_session: bool) -> Self {
        Self {
            resume: load_session,
            interrupt: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub provider: String,
    pub protocol: String,
    pub id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub description: String,
    pub permission_note: String,
    pub capabilities: Capabilities,
}

pub fn name(provider: &str) -> &str {
    match provider {
        "codex" => "Codex",
        "qoder" => "Qoder",
        other => other,
    }
}
pub fn valid_provider(provider: &str) -> bool {
    !provider.is_empty()
        && provider.len() <= 64
        && provider != "demo"
        && provider
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}
pub fn default_provider() -> String {
    "codex".into()
}

#[cfg(feature = "desktop")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doctor {
    pub provider: String,
    pub available: bool,
    pub initialized: bool,
    pub path: String,
    pub version: String,
    pub message: String,
    pub capabilities: Capabilities,
}

#[cfg(feature = "desktop")]
pub trait Executor: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    fn doctor(&self) -> Doctor;
    fn ensure_idle(&self) -> Result<(), String>;
    fn ensure_task_idle(&self, task_id: &str) -> Result<(), String>;
    fn launch(
        &self,
        app: tauri::AppHandle,
        task: crate::model::Task,
        resume_anchor: Option<String>,
    ) -> Result<crate::model::Task, String>;
    fn interrupt(&self, app: tauri::AppHandle, run_id: String) -> Result<(), String>;
    fn steer(&self, _app: &tauri::AppHandle, _run_id: String, _text: String) -> Result<(), String> {
        Err(format!(
            "{} 当前接入方式不支持运行中补充；请等待交付结束",
            self.descriptor().name
        ))
    }
    fn reply(
        &self,
        _app: &tauri::AppHandle,
        _run_id: String,
        _approval_id: String,
        _answers: std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        Err(format!(
            "{} 当前接入方式不支持此审批答复",
            self.descriptor().name
        ))
    }
    fn sync_agents(&self, _task_id: String) -> Result<crate::model::Task, String> {
        Err(format!(
            "{} 当前接入方式不支持子 Agent 历史同步",
            self.descriptor().name
        ))
    }
    fn refresh(&self, task: &crate::model::Task);
    fn forget(&self, task_id: &str);
    fn shutdown(&self);
}
