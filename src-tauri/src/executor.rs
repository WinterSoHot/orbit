use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutorModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_default: bool,
}
pub fn valid_model_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && !id.starts_with('-')
        && !id.chars().any(|c| c.is_control() || c.is_whitespace())
}
pub fn codex_model_page(result: &serde_json::Value) -> Result<Vec<ExecutorModel>, String> {
    let rows = result["data"].as_array().ok_or("模型目录响应无效")?;
    if rows.len() > 100 { return Err("模型目录超过分页大小限制".into()); }
    let mut models = Vec::new();
    for row in rows {
        if row["hidden"] == true { continue; }
        let id = row["model"].as_str().filter(|id| valid_model_id(id)).ok_or("模型标识无效")?;
        if models.iter().any(|m: &ExecutorModel| m.id == id) { continue; }
        models.push(ExecutorModel {
            id: id.into(),
            name: row["displayName"].as_str().unwrap_or(id).chars().take(100).collect(),
            description: row["description"].as_str().unwrap_or("").chars().take(500).collect(),
            is_default: row["isDefault"] == true,
        });
    }
    Ok(models)
}
pub fn qoder_model_catalog(output: &str) -> Result<Vec<ExecutorModel>, String> {
    if output.len() > 65536 { return Err("Qoder 模型目录超过大小限制".into()); }
    let mut lines = output.lines().filter(|line| !line.trim().is_empty());
    if lines.next().map(str::trim) != Some("MODEL") { return Err("Qoder 未返回可识别的模型目录，请核对 CLI 版本与登录状态".into()); }
    let mut models = Vec::new();
    for line in lines {
        let id = line.trim();
        if !valid_model_id(id) || models.len() >= 512 { return Err("Qoder 模型目录格式无效或超过限制".into()); }
        if models.iter().any(|m: &ExecutorModel| m.id == id) { continue; }
        models.push(ExecutorModel { id: id.into(), name: id.into(), description: String::new(), is_default: false });
    }
    if models.is_empty() { return Err("Qoder 未返回可选模型".into()); }
    Ok(models)
}

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

#[derive(Clone, Debug)]
pub struct Ownership {pub task_id:String,pub run_id:String,pub session_id:Option<String>}

#[cfg(feature = "desktop")]
pub trait Executor: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    fn doctor(&self) -> Doctor;
    fn models(&self) -> Result<Vec<ExecutorModel>, String> {
        Err(format!("{} 尚未开放模型目录", self.descriptor().name))
    }
    fn ensure_idle(&self) -> Result<(), String>;
    fn ownership(&self) -> Vec<Ownership> {vec![]}
    fn abort_start(&self, app:tauri::AppHandle,run_id:String)->Result<(),String>{self.interrupt(app,run_id)}
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
    fn steer_sources(&self,app:&tauri::AppHandle,run_id:String,text:String,sources:Vec<crate::sources::SourceSnapshot>)->Result<(),String>{
        if sources.is_empty(){self.steer(app,run_id,text)}else{Err("当前执行器不支持运行中引用资料，请等待本轮结束".into())}
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

#[cfg(test)]
mod model_catalog_tests {
    use super::*;
    #[test]
    fn visible_models_preserve_cli_ids_and_reject_invalid_catalogs() {
        let rows = serde_json::json!({"data":[
            {"model":"gpt-test","displayName":"GPT Test","isDefault":true,"hidden":false},
            {"model":"secret","hidden":true},
            {"model":"gpt-test","displayName":"duplicate"}
        ],"nextCursor":null});
        let models = codex_model_page(&rows).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-test");
        assert_eq!(models[0].name, "GPT Test");
        assert!(models[0].is_default);
        assert!(codex_model_page(&serde_json::json!({"data":[{"model":"bad\nvalue"}]})).is_err());
        assert!(codex_model_page(&serde_json::json!({"data":null})).is_err());
        let qoder = qoder_model_catalog("MODEL\nQwen3.8-Max\nQwen3.8-Flash\nQwen3.8-Max\n").unwrap();
        assert_eq!(qoder.len(),2);
        assert_eq!(qoder[1].id,"Qwen3.8-Flash");
        assert!(qoder_model_catalog("Login required\n").is_err());
        assert!(qoder_model_catalog("MODEL\nwarning: login required\n").is_err());
        assert!(!valid_model_id(""));
        assert!(!valid_model_id(" model "));
        assert!(!valid_model_id(&"m".repeat(257)));
    }
}
