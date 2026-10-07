use crate::model::{Task, QueueAction, QueueRequest, Acceptance};
use crate::knowledge::{LibraryStore,LibraryExport,NewDocument,Kind};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
};

const MAX_STORE: u64 = 8 * 1024 * 1024;
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub tasks: Vec<Task>,
    #[serde(default="crate::team::default_profiles")] pub agents:Vec<crate::team::AgentProfile>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, rename = "exportDirectory")]
    pub export_directory: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Disk {
    version: u32,
    tasks: Vec<Task>,
    #[serde(default="crate::team::default_profiles")] agents:Vec<crate::team::AgentProfile>,
    #[serde(default, rename = "exportDirectory")]
    export_directory: Option<PathBuf>,
}
#[derive(Serialize)]
pub struct ExportSettings {
    pub directory: String,
    pub custom: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceExport {
    format: &'static str,
    version: u32,
    exported_at: u64,
    #[serde(flatten)]
    workspace: Workspace,
    library: LibraryExport,
    code_bundles:Vec<crate::coding::BundleExport>,
}
pub struct Store {
    pub directory: PathBuf,
    pub(crate) data: Mutex<Workspace>,
    pub library: LibraryStore,
}

impl Store {
    pub fn open(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&directory).map_err(|_| "无法创建本地记录目录".to_string())?;
        let file = directory.join("workspace.json");
        let data = if file.exists() {
            let read = (|| -> Result<Workspace, String> {
                let mut bytes = Vec::new();
                File::open(&file)
                    .map_err(|_| "无法读取记录")?
                    .take(MAX_STORE + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "无法读取记录")?;
                if bytes.len() as u64 > MAX_STORE {
                    return Err("记录超过大小限制".into());
                }
                let disk: Disk = serde_json::from_slice(&bytes).map_err(|_| "记录格式损坏")?;
                if !matches!(disk.version, 1 | 2 | 3 | 4 | 5) || disk.tasks.len() > 50 {
                    return Err("记录版本或任务数量不受支持".into());
                }
                if disk.agents.len()>12{return Err("Agent 数量超限".into())}
                let mut agent_ids=std::collections::HashSet::new();for a in &disk.agents{a.validate()?;if !agent_ids.insert(&a.id){return Err("Agent 标识重复".into())}}
                let mut tasks: Vec<_> = disk
                    .tasks
                    .into_iter()
                    .filter(|t| t.provider != "demo")
                    .collect();
                crate::team::validate_links(&tasks)?;
                for task in &mut tasks {
                    validate(task)?;
                    task.recover();
                }
                Ok(Workspace {
                    tasks,
                    agents:disk.agents,
                    error: None,
                    export_directory: disk.export_directory,
                })
            })();
            match read {
                Ok(workspace) => workspace,
                Err(reason) => Workspace {
                    tasks: vec![],
                    agents:crate::team::default_profiles(),
                    export_directory: None,
                    error: Some(format!("{}，原文件已保留。当前操作暂存内存。", reason)),
                },
            }
        } else {
            Workspace{agents:crate::team::default_profiles(),..Default::default()}
        };
        let library = LibraryStore::open(directory.clone())?;
        Ok(Self {
            library,
            directory,
            data: Mutex::new(data),
        })
    }
    pub fn workspace(&self) -> Workspace {
        self.data.lock().unwrap().clone()
    }
    pub fn task(&self, id: &str) -> Option<Task> {
        self.data
            .lock()
            .unwrap()
            .tasks
            .iter()
            .find(|t| t.id == id)
            .cloned()
    }
    pub fn save_task(&self, mut task: Task) -> Result<(), String> {
        validate(&task)?;
        let mut data = self.data.lock().unwrap();
        let mut candidate = data.clone();
        if let Some(old) = candidate.tasks.iter_mut().find(|t| t.id == task.id) {
            if old.source_inputs.iter().any(|i| !task.source_inputs.iter().any(|n|i.same_content(n))){return Err("资料快照不可改写".into())}
            if old.provider != task.provider || old.requested_model != task.requested_model {
                return Err("任务执行器与模型不可变，请新建任务调整选择".into());
            }
            if task.revision < old.revision {
                return Ok(());
            }
            task.executor_revision = if old.run_id == task.run_id { old.executor_revision } else { Some(task.revision) };
            *old = task;
        } else {
        task.executor_revision=Some(task.revision);
            if candidate.tasks.len() >= 50 {
                return Err("已达到 50 个任务，请先删除不需要的归档任务".into());
            }
            candidate.tasks.insert(0, task);
        }
        self.persist(&candidate)?;
        *data = candidate;
        Ok(())
    }
    pub fn continue_task(
        &self,
        id: &str,
        revision: u64,
        run: &Option<String>,
        turn: &Option<String>,
        text: &str,
    ) -> Result<(Task, String), String> {
        let mut data = self.data.lock().unwrap();
        let index = data
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or("任务不存在")?;
        let (next, anchor) = data.tasks[index].continued(revision, run, turn, text)?;
        validate(&next)?;
        let mut candidate = data.clone();
        candidate.tasks[index] = next.clone();
        self.persist(&candidate)?;
        *data = candidate;
        Ok((next, anchor))
    }
    pub fn enqueue(&self, id: &str, revision: u64, action: QueueAction) -> Result<Task,String> {self.enqueue_with_sources(id,revision,action,vec![])}
    pub fn enqueue_with_sources(&self, id:&str, revision:u64, action:QueueAction, sources:Vec<crate::sources::SourceSnapshot>)->Result<Task,String>{
        crate::sources::validate_sources(&sources)?;
        let mut data = self.data.lock().unwrap();
        let index = data.tasks.iter().position(|t| t.id == id).ok_or("任务不存在")?;
        let t = &data.tasks[index];
        crate::team_store::can_enqueue(&data,t)?;
        if t.revision != revision || t.archived || t.queue.is_some() || matches!(t.status.as_str(), "running"|"approval"|"cancelling") {
            return Err("任务已更新、正在执行或已在队列中，请重新核对".into());
        }
        if let QueueAction::Continue {text,run_id,turn_id} = &action { t.continued(revision,run_id,turn_id,text)?; }
        let order = data.tasks.iter().filter_map(|t|t.queue.as_ref().map(|q|q.order)).max().unwrap_or(0).checked_add(1).filter(|n|*n<=9_007_199_254_740_991).ok_or("队列序号超过限制")?;
        let mut candidate = data.clone();
        let t=&mut candidate.tasks[index];
        t.queue=Some(QueueRequest {cancel_requested:false,request_id:uuid::Uuid::new_v4().to_string(),order,next_run_id:uuid::Uuid::new_v4().to_string(),state:"pending".into(),action,error:None});
        if !sources.is_empty() {
            let q=t.queue.as_ref().unwrap();
            let QueueAction::Continue{text,..}=&q.action else {return Err("新资料请在补充对话中发送".into())};
            t.source_inputs.push(crate::sources::SourceInput::new(q.next_run_id.clone(),"continue",text.clone(),sources,Some(q.next_run_id.clone()),None));
        }
        t.event("请求已加入等待队列", "system", "工作台");
        validate(t)?;
        let saved=t.clone();self.persist(&candidate)?;*data=candidate;Ok(saved)
    }
    pub fn claim_next(&self) -> Result<Option<(Task, Option<String>)>,String> {self.claim_next_reserved(&[])}
    pub fn claim_next_reserved(&self,live_requests:&[String]) -> Result<Option<(Task, Option<String>)>,String> {
        let mut data=self.data.lock().unwrap();
        if data.tasks.iter().any(|t|t.queue.as_ref().is_some_and(|q|q.state=="claimed"&&!live_requests.contains(&q.request_id))) {return Err("存在未确认的启动请求，请先核对并撤销该请求".into());}
        let Some(index)=data.tasks.iter().enumerate().filter(|(_,t)|t.queue.as_ref().is_some_and(|q|q.state=="pending")).min_by_key(|(_,t)|t.queue.as_ref().unwrap().order).map(|(i,_)|i) else {return Ok(None)};
        let t=&data.tasks[index];let q=t.queue.as_ref().unwrap();
        if q.error.is_some() || t.archived || matches!(t.status.as_str(),"running"|"approval"|"cancelling") {return Err("队首任务需要核对，未启动".into());}
        let (mut next,anchor)=match &q.action {
            QueueAction::Start=>{let mut next=t.clone();next.begin_run(t.team.is_some()||t.parent_link.is_some());if t.team.is_some()||t.parent_link.is_some(){next.thread_id=None;next.session_ref=None;}(next,None)},
            QueueAction::Continue{text,run_id,turn_id}=>{let (next,anchor)=t.continued(t.revision,run_id,turn_id,text)?;(next,Some(anchor))}
        };
        next.run_id=Some(q.next_run_id.clone());
        if matches!(q.action,QueueAction::Start) {
            if !next.source_inputs.is_empty(){
                let sources=next.source_inputs.iter().find(|i|i.kind=="template").map(|i|i.sources.clone()).unwrap_or_default();
                next.source_inputs.push(crate::sources::SourceInput::new(q.next_run_id.clone(),"initial",next.prompt.clone(),sources,Some(q.next_run_id.clone()),None));
            }
        }
        next.executor_revision=Some(next.revision);
        if anchor.is_some() {if let Some(last)=next.supplements.last_mut(){last.run_id=q.next_run_id.clone();}}
        next.queue.as_mut().unwrap().state="claimed".into();
        validate(&next)?;let mut candidate=data.clone();candidate.tasks[index]=next.clone();
        self.persist(&candidate)?;*data=candidate;Ok(Some((next,anchor)))
    }
    pub fn finish_claim(&self,id:&str,request_id:&str,run_id:&str,error:Option<String>) -> Result<Task,String> {
        let mut data=self.data.lock().unwrap();let index=data.tasks.iter().position(|t|t.id==id).ok_or("任务不存在")?;
        let t=&data.tasks[index];
        if !t.queue.as_ref().is_some_and(|q|q.request_id==request_id && q.next_run_id==run_id && q.state=="claimed") || t.run_id.as_deref()!=Some(run_id) {return Err("启动请求已失效").map_err(String::from);}
        if t.queue.as_ref().is_some_and(|q|q.cancel_requested){return Err("启动取消意图尚未清理确认，不能完成启动交接".into())}
        let mut candidate=data.clone();let t=&mut candidate.tasks[index];
        if let Some(error)=error {if matches!(t.status.as_str(),"running"|"approval"|"cancelling"){t.status="unknown".into();}for input in &mut t.source_inputs {if input.run_id.as_deref()==Some(run_id)&&input.status=="pending"{input.status="unknown".into();}}t.queue.as_mut().unwrap().error=Some(error.chars().take(2000).collect());t.event("启动未确认，请求已保留，请核对后撤销", "error", "工作台");}
        else {t.queue=None;t.event("任务已离开等待队列", "system", "工作台");}
        let saved=t.clone();self.persist(&candidate)?;*data=candidate;Ok(saved)
    }
    pub fn cancel_queued(&self,id:&str,revision:u64) -> Result<Task,String> {
        let mut data=self.data.lock().unwrap();let index=data.tasks.iter().position(|t|t.id==id).ok_or("任务不存在")?;let t=&data.tasks[index];
        if t.revision!=revision || !t.queue.as_ref().is_some_and(|q|q.state=="pending") || matches!(t.status.as_str(),"running"|"approval"|"cancelling") {return Err("任务已更新或已经启动，不能撤销排队".into());}
        let mut candidate=data.clone();let next=&mut candidate.tasks[index];
        let run=&next.queue.as_ref().unwrap().next_run_id;
        for input in &mut next.source_inputs {if input.run_id.as_ref()==Some(run)&&input.status=="pending"{input.status="cancelled".into();}}
        next.queue=None;next.event("已撤销等待请求，历史保留", "system", "你");
        let saved=candidate.tasks[index].clone();self.persist(&candidate)?;*data=candidate;Ok(saved)
    }
    pub fn request_claim_cancel(&self,id:&str,revision:u64)->Result<Task,String>{
        let mut data=self.data.lock().unwrap();let mut next=data.clone();let t=next.tasks.iter_mut().find(|t|t.id==id&&t.revision==revision).ok_or("任务已更新")?;
        let q=t.queue.as_mut().filter(|q|q.state=="claimed").ok_or("启动请求不存在")?;q.cancel_requested=true;q.error=Some("正在取消启动，等待进程清理确认".into());t.event("启动取消意图已保存","system","你");let saved=t.clone();self.persist(&next)?;*data=next;Ok(saved)
    }
    // Caller must hold lifecycle admission and verify actual executor ownership is idle.
    pub fn cancel_claimed(&self,id:&str,revision:u64,request_id:&str,run_id:&str)->Result<Task,String> {
        let mut data=self.data.lock().unwrap();let index=data.tasks.iter().position(|t|t.id==id).ok_or("任务不存在")?;let t=&data.tasks[index];
        if t.revision!=revision || t.run_id.as_deref()!=Some(run_id) || !t.queue.as_ref().is_some_and(|q|q.state=="claimed"&&q.request_id==request_id&&q.next_run_id==run_id){return Err("启动请求已更新，请重新核对".into());}
        let mut candidate=data.clone();let t=&mut candidate.tasks[index];
        if matches!(t.status.as_str(),"running"|"approval"|"cancelling"){t.status="unknown".into();}
        for input in &mut t.source_inputs {if input.run_id.as_deref()==Some(run_id)&&input.status=="pending"{input.status="unknown".into();}}
        t.queue=None;t.event("执行器已空闲，已撤销未确认启动请求", "system", "你");
        let saved=t.clone();self.persist(&candidate)?;*data=candidate;Ok(saved)
    }
    pub fn accept_task(&self,id:&str,revision:u64,run:&Option<String>,turn:&Option<String>) -> Result<Task,String> {
        let mut data=self.data.lock().unwrap();let index=data.tasks.iter().position(|t|t.id==id).ok_or("任务不存在")?;let t=&data.tasks[index];
        crate::team_store::can_accept(&data,t)?;
        if t.revision!=revision || &t.run_id!=run || &t.turn_id!=turn || t.archived || t.queue.is_some() || t.status!="completed" || t.current_delivery_ids().is_empty() {return Err("请核对最新已完成的交付后再验收".into());}
        let mut candidate=data.clone();let t=&mut candidate.tasks[index];
        t.acceptance=Some(Acceptance{run_id:t.run_id.clone(),turn_id:t.turn_id.clone(),artifact_ids:t.current_delivery_ids()});
        t.event("最新交付已验收", "system", "你");let saved=t.clone();self.persist(&candidate)?;*data=candidate;Ok(saved)
    }
    pub fn save_existing_task(&self, mut task: Task) -> Result<Option<Task>, String> {
        let actor_revision=task.revision;
        let mut data = self.data.lock().unwrap();
        let Some(index) = data.tasks.iter().position(|t| t.id == task.id) else {
            return Ok(None);
        };
        let current = &data.tasks[index];
        if current.archived
            || current.provider != task.provider
            || current.requested_model != task.requested_model
            || current.run_id != task.run_id
            || task.revision <= current.executor_revision.unwrap_or(current.revision)
            || (current.terminal() && (current.status != task.status || current.turn_id != task.turn_id))
        {
            return Ok(None);
        }
        let mut inputs=current.source_inputs.clone();
        for input in &task.source_inputs {
            if let Some(old)=inputs.iter_mut().find(|i|i.id==input.id){
                if !old.same_content(input){return Err("资料快照不可改写".into())}
                // Qoder confirms only from a matching remote prompt response/update, not its local request token.
                if task.provider=="qoder" && input.status=="accepted" && input.run_id==task.run_id && input.turn_id==task.turn_id {old.status="accepted".into();old.turn_id=input.turn_id.clone();}
            }
            else {
                if input.kind!="direction" || input.run_id!=current.run_id || input.turn_id!=current.turn_id || !task.directions.iter().any(|d|d.id==input.id && d.text==input.text && d.status=="pending"){return Err("新增资料与当前输入不匹配".into())}
                inputs.push(input.clone());
            }
        }
        task.source_inputs=inputs;
        for input in &mut task.source_inputs {
            if input.kind=="direction" {if let Some(d)=task.directions.iter().find(|d|d.id==input.id){input.status=d.status.clone();}}
            else if input.run_id.is_some() && input.run_id==task.run_id {
                if task.provider=="codex" && task.turn_id.is_some(){input.turn_id=task.turn_id.clone();input.status="accepted".into();}
                else if input.status=="pending" && matches!(task.status.as_str(),"failed"|"unknown"|"interrupted"){input.status="unknown".into();}
            }
        }
        for event in current.events.iter().filter(|e|e.id.starts_with("delivery:")||e.id.starts_with("team:")) {if !task.events.iter().any(|e|e.id==event.id){task.events.push(event.clone());}}
        if task.events.len()>100{task.events.drain(..task.events.len()-100);}
        // The store owns delivery receipts, user edits and message-time source binding.
        task.assignment=current.assignment.clone();task.team=current.team.clone();task.parent_link=current.parent_link.clone();task.team_input=current.team_input.clone();task.review_submission=current.review_submission.clone();task.code_workspace=current.code_workspace.clone();
        task.artifacts=current.artifacts.clone();
        task.delivery_submissions=current.delivery_submissions.clone();
        task.explicit_delivery=current.explicit_delivery;task.delivery_error=current.delivery_error.clone();
        let provided=task.provided_input_ids();
        for message in &mut task.conversation {
            if let Some(old)=current.conversation.iter().find(|m|m.run_id==message.run_id&&m.thread_id==message.thread_id&&m.item_id==message.item_id&&m.status=="completed") {*message=old.clone();}
            else if message.run_id==task.run_id.as_deref().unwrap_or("")&&message.status=="completed" {message.source_input_ids=provided.clone();}
            else {message.source_input_ids.clear();}
        }
        if task.status=="completed" {
            let outcome=if task.parent_link.as_ref().is_some_and(|p|p.role=="review")||task.delivery_candidate.text.trim_start().starts_with("```orbit-review") {crate::team::commit_review(&mut task)}else{crate::delivery::commit(&mut task)};
            match outcome {
                Ok(true)=>{if task.delivery_submissions.len()>current.delivery_submissions.len(){task.delivery_error=None;task.event("成果已通过格式校验并保存，等待验收", "artifact", "工作台");task.events.last_mut().unwrap().id=format!("delivery:{}",uuid::Uuid::new_v4());}},
                Ok(false)=>{},
                Err(error)=>{let notice=format!("交付未保存：{error}");task.delivery_error=Some(notice.clone());if !current.events.iter().any(|e|e.text==notice){task.event(&notice,"error","交付");task.events.last_mut().unwrap().id=format!("delivery:{}",uuid::Uuid::new_v4());}}
            }
        }
        task.executor_revision=Some(actor_revision);
        task.revision=task.revision.max(current.revision.checked_add(1).ok_or("任务版本超过限制")?);
        task.queue = current.queue.clone();
        task.acceptance = current.acceptance.clone();
        validate(&task)?;
        let mut candidate = data.clone();
        candidate.tasks[index] = task.clone();
        crate::team_store::advance(&mut candidate);
        let task=candidate.tasks[index].clone();
        self.persist(&candidate)?;
        *data = candidate;
        Ok(Some(task))
    }
    pub fn archive_task(&self, id: &str) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let index = data
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or("任务不存在")?;
        let task = &data.tasks[index];
        crate::team_store::protect(&data,&task.id)?;
        if task.archived {
            return Ok(task.clone());
        }
        crate::team_store::can_accept(&data,task)?;
        if !task.accepted() || task.queue.is_some() {
            return Err("请先验收最新交付并取消排队，再归档任务".into());
        }
        let mut candidate = data.clone();
        if candidate.tasks[index].team.is_some(){if candidate.tasks.iter().any(|t|t.parent_link.as_ref().is_some_and(|l|l.parent_id==id)&&(!t.terminal()||t.queue.is_some())){return Err("请先核对所有子任务的结束状态".into())}for child in candidate.tasks.iter_mut().filter(|t|t.parent_link.as_ref().is_some_and(|l|l.parent_id==id)){child.archived=true;child.event("随父任务归档","system","工作台");}}
        candidate.tasks[index].archived = true;
        candidate.tasks[index].event("任务已归档", "system", "你");
        crate::team_store::advance(&mut candidate);
        let saved = candidate.tasks[index].clone();
        self.persist(&candidate)?;
        *data = candidate;
        Ok(saved)
    }
    pub fn delete_task(&self, id: &str) -> Result<(), String> {
        let mut data = self.data.lock().unwrap();
        let task = data.tasks.iter().find(|t| t.id == id).ok_or("任务不存在")?;
        crate::team_store::protect(&data,id)?;
        if data.tasks.iter().any(|t|t.parent_link.as_ref().is_some_and(|p|p.parent_id==id)&&(!t.terminal()||t.queue.is_some())){return Err("子任务尚未结束或仍在队列中".into())}
        if !task.archived&&(!task.terminal()||task.queue.is_some()||!task.current_delivery_ids().is_empty()) {
            return Err("正式交付请先验收并归档；无交付任务请等执行结束且无排队后再删除".into());
        }
        let mut candidate = data.clone();
        candidate.tasks.retain(|t| t.id != id&&t.parent_link.as_ref().is_none_or(|p|p.parent_id!=id));
        self.persist(&candidate)?;
        *data = candidate;
        Ok(())
    }
    pub fn sync_agents(
        &self,
        id: &str,
        run_id: &str,
        turn_id: &str,
        history: &crate::protocol::AgentHistory,
    ) -> Result<Task, String> {
        let mut data = self.data.lock().unwrap();
        let ti = data
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or("任务已不存在")?;
        let task = &data.tasks[ti];
        if task.archived {
            return Err("归档任务只读，不能同步协作".into());
        }
        if task.run_id.as_deref() != Some(run_id) || task.turn_id.as_deref() != Some(turn_id) {
            return Err("任务已进入新一轮运行，请重新同步".into());
        }
        let saved = crate::protocol::reconcile_agent_history(task, history)?;
        if saved.revision == task.revision {
            return Ok(saved);
        }
        let mut candidate = data.clone();
        candidate.tasks[ti] = saved.clone();
        self.persist(&candidate)?;
        *data = candidate;
        Ok(saved)
    }
    pub fn edit_artifact(&self, id: &str, expected: &str, content: &str) -> Result<Task, String> {
        if content.len() > 256000 {
            return Err("文档超过 256 KB，请缩短后保存".into());
        }
        let mut data = self.data.lock().unwrap();
        let matches: Vec<_> = data
            .tasks
            .iter()
            .enumerate()
            .flat_map(|(ti, t)| {
                t.artifacts
                    .iter()
                    .enumerate()
                    .filter(move |(_, a)| a.id == id)
                    .map(move |(ai, _)| (ti, ai))
            })
            .collect();
        if matches.len() != 1 {
            return Err("文档不存在或标识重复，请重新打开".into());
        }
        let (ti, ai) = matches[0];
        let task = &data.tasks[ti];
        if task.archived||task.parent_link.as_ref().is_some_and(|l|data.tasks.iter().any(|p|p.id==l.parent_id&&p.archived)) {
            return Err("归档交付只读，请导出后编辑".into());
        }
        if !task.terminal() || task.queue.is_some() {
            return Err("请等任务结束且无排队请求后再编辑交付物".into());
        }
        if task.artifacts[ai].kind!="markdown" {return Err("仅 Markdown 成果支持编辑".into())}
        if task.artifacts[ai].content != expected {
            return Err("文档已更新，请保留草稿并重新打开".into());
        }
        if content == expected {
            return Ok(task.clone());
        }
        let mut candidate = data.clone();
        candidate.tasks[ti].artifacts[ai].content = content.into();
        candidate.tasks[ti].acceptance = None;
        candidate.tasks[ti].event("已保存 Markdown 修改", "artifact", "你");
        crate::team_store::advance(&mut candidate);
        let saved = candidate.tasks[ti].clone();
        self.persist(&candidate)?;
        *data = candidate;
        Ok(saved)
    }
    pub fn save_message(&self,id:&str,revision:u64,run:&str,thread:&str,item:&str)->Result<Task,String>{
        let mut data=self.data.lock().unwrap();let index=data.tasks.iter().position(|t|t.id==id).ok_or("任务不存在")?;
        let task=&data.tasks[index];
        if task.revision!=revision||task.archived||task.queue.is_some()||!task.terminal(){return Err("请等任务结束且无排队请求后再保存回复".into())}
        let message=task.conversation.iter().find(|m|m.run_id==run&&m.thread_id==thread&&m.item_id==item&&m.kind=="assistant"&&m.status=="completed"&&!m.truncated&&!m.text.trim().is_empty()).ok_or("消息不完整或不存在，无法保存")?;
        if task.delivery_submissions.iter().any(|r|r.origin=="manual"&&r.run_id==run&&r.thread_id==thread&&r.item_id==item){return Ok(task.clone())}
        if task.artifacts.len()>=10||task.delivery_submissions.len()>=10{return Err("已达到 10 份成果上限".into())}
        let artifact=crate::model::Artifact{id:uuid::Uuid::new_v4().to_string(),name:format!("回复-{}.md",task.artifacts.len()+1),kind:"markdown".into(),content:message.text.clone(),created_at:crate::model::now(),source_input_ids:message.source_input_ids.clone()};
        let receipt=crate::delivery::Receipt{id:uuid::Uuid::new_v4().to_string(),run_id:run.into(),turn_id:None,thread_id:thread.into(),item_id:item.into(),origin:"manual".into(),canonical:message.text.clone(),artifact_ids:vec![artifact.id.clone()]};
        let mut candidate=data.clone();let next=&mut candidate.tasks[index];next.artifacts.push(artifact);next.delivery_submissions.push(receipt);next.event("回复已手动保存为 Markdown，不计为执行器交付","artifact","你");validate(next)?;let saved=next.clone();self.persist(&candidate)?;*data=candidate;Ok(saved)
    }
    pub fn export_settings(&self) -> ExportSettings {
        let data = self.data.lock().unwrap();
        self.settings_for(&data)
    }
    fn settings_for(&self, data: &Workspace) -> ExportSettings {
        ExportSettings {
            directory: data
                .export_directory
                .as_ref()
                .unwrap_or(&self.directory.join("exports"))
                .display()
                .to_string(),
            custom: data.export_directory.is_some(),
        }
    }
    pub fn set_export_directory(
        &self,
        selected: Option<PathBuf>,
    ) -> Result<ExportSettings, String> {
        let selected = match selected {
            Some(path) => {
                if !path.is_absolute() {
                    return Err("请选择绝对路径的文件夹".into());
                }
                let path = path
                    .canonicalize()
                    .map_err(|_| "所选文件夹不存在或无法访问")?;
                if !path.is_dir() || path.to_str().is_none_or(|s| s.len() > 4096) {
                    return Err("所选路径不是有效的文件夹".into());
                }
                let probe = path.join(format!(".orbit-write-check-{}", uuid::Uuid::new_v4()));
                let file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&probe)
                    .map_err(|_| "所选文件夹不可写，请选择其他文件夹")?;
                drop(file);
                fs::remove_file(&probe).map_err(|_| "无法清理文件夹写入检查，请选择其他文件夹")?;
                Some(path)
            }
            None => None,
        };
        let mut data = self.data.lock().unwrap();
        let mut candidate = data.clone();
        candidate.export_directory = selected;
        self.persist(&candidate)?;
        *data = candidate;
        Ok(self.settings_for(&data))
    }
    pub fn export_artifact(&self, id: &str) -> Result<String, String> {
        let data = self.data.lock().unwrap();
        let artifact = data
            .tasks
            .iter()
            .flat_map(|t| &t.artifacts)
            .find(|a| a.id == id)
            .ok_or("交付物未保存，请稍后再试")?;
        if artifact.content.len() > 256000 {
            return Err("交付物超出导出大小限制".into());
        }
        let name: String = artifact
            .name
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .take(80)
            .collect();
        let name = if name.ends_with(".md") || name.ends_with(".txt") {
            name
        } else {
            format!("{}.md",if name.is_empty(){"artifact"}else{&name})
        };
        let task=data.tasks.iter().find(|t|t.artifacts.iter().any(|a|a.id==id)).unwrap();
        let content=format!("{}{}",crate::delivery::export_body(artifact)?,crate::sources::appendix(&task.source_inputs,&artifact.source_input_ids));
        self.write_export(&data, &name, content.as_bytes())
    }
    pub fn export_workspace(&self) -> Result<String, String> {
        let (workspace,library,exported_at) = {
            let data = self.data.lock().unwrap();
            if data.error.is_some() {return Err("原记录无法读取，不能导出完整数据；原文件已保留".into());}
            let library = self.library.snapshot()?;
            (data.clone(),library,crate::model::now())
        };
        // Immutable PDF attachments are assembled after both snapshot locks are released.
        let library=self.library.export_snapshot(&library)?;
        let code_bundles=crate::coding::export_bundles(&workspace.tasks)?;
        let snapshot=WorkspaceExport{code_bundles,format:"orbit-workspace",version:5,exported_at,workspace,library};
        let bytes=serde_json::to_vec_pretty(&snapshot).map_err(|_|"无法编码工作台数据")?;
        self.write_export(&snapshot.workspace,&format!("orbit-workspace-{}.json",snapshot.exported_at),&bytes)
    }
    pub fn export_document(&self,id:&str)->Result<String,String>{
        let (name,bytes)=self.library.document_bytes(id)?;
        let safe:String=name.chars().map(|c|if c.is_alphanumeric()||"._-".contains(c){c}else{'_'}).take(140).collect();
        self.write_export(&self.workspace(),&safe,&bytes)
    }
    pub fn collect_artifact(&self,id:&str)->Result<crate::knowledge::Document,String>{
        let data=self.workspace();let artifact=data.tasks.iter().flat_map(|t|&t.artifacts).find(|a|a.id==id).ok_or("交付文档不存在")?;
        self.library.create(NewDocument{title:artifact.name.chars().take(128).collect(),kind:Kind::Markdown,content:format!("{}{}",crate::delivery::export_body(artifact)?,crate::sources::appendix(&data.tasks.iter().find(|t|t.artifacts.iter().any(|a|a.id==id)).unwrap().source_inputs,&artifact.source_input_ids)),url:None,tags:vec!["交付".into()]})
    }
    fn export_directory(&self, data: &Workspace) -> Result<PathBuf, String> {
        if let Some(selected) = &data.export_directory {
            let actual = selected
                .canonicalize()
                .map_err(|_| "导出文件夹已失效，请在设置中重新选择")?;
            if &actual != selected || !actual.is_dir() {
                return Err("导出文件夹已变化，请在设置中重新选择".into());
            }
            Ok(actual)
        } else {
            let directory = self.directory.join("exports");
            fs::create_dir_all(&directory).map_err(|_| "无法创建导出目录".to_string())?;
            let root = self
                .directory
                .canonicalize()
                .map_err(|_| "无法解析本地目录")?;
            let directory = directory.canonicalize().map_err(|_| "无法解析导出目录")?;
            if !directory.starts_with(root) {
                return Err("导出目录位于工作台目录之外".into());
            }
            Ok(directory)
        }
    }
    fn write_export(&self, data: &Workspace, name: &str, bytes: &[u8]) -> Result<String, String> {
        let directory = self.export_directory(data)?;
        let destination = directory.join(format!("{}_{}", uuid::Uuid::new_v4(), export_filename(name)));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&destination).map_err(|_| "无法保存导出文件")?;
        if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
            drop(file);
            let _ = fs::remove_file(&destination);
            return Err("导出文件写入失败".into());
        }
        Ok(destination.display().to_string())
    }
    pub(crate) fn persist(&self, data: &Workspace) -> Result<(), String> {
        for task in &data.tasks{validate(task)?;}crate::team::validate_links(&data.tasks)?;
        if data.error.is_some() {
            return Err("原记录无法读取，已禁止覆盖；当前操作暂存内存".into());
        }
        let tasks = data
            .tasks
            .iter()
            .cloned()
            .map(|mut task| {
                task.approvals.clear();
                task
            })
            .collect();
        let bytes = serde_json::to_vec(&Disk {
            version: 5,
            agents:data.agents.clone(),
            tasks,
            export_directory: data.export_directory.clone(),
        })
        .map_err(|_| "无法编码记录".to_string())?;
        if bytes.len() as u64 > MAX_STORE {
            return Err("记录超过 8 MiB，原文件保留".into());
        }
        let temporary = self.directory.join("workspace.tmp");
        let destination = self.directory.join("workspace.json");
        let mut file = File::create(&temporary).map_err(|_| "无法写入临时记录".to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "无法设置记录权限".to_string())?;
        }
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "记录写入失败，原文件保留".to_string())?;
        fs::rename(temporary, destination).map_err(|_| "记录替换失败，原文件保留".to_string())
    }
}
fn export_filename(name:&str)->String{
    let safe:String=name.chars().filter(|c|c.is_alphanumeric()||matches!(c,'.'|'_'|'-')).collect();
    let (stem,suffix)=safe.rsplit_once('.').filter(|(_,ext)|!ext.is_empty()&&ext.len()<=10&&ext.bytes().all(|b|b.is_ascii_alphanumeric())).map(|(stem,ext)|(stem,format!(".{ext}"))).unwrap_or((&safe,String::new()));
    // NAME_MAX=255 bytes, including the UUID and separator. Keep the extension intact.
    let budget=255-37-suffix.len();let mut base=String::new();
    for ch in stem.chars(){if base.len()+ch.len_utf8()>budget{break}base.push(ch);}
    if base.is_empty(){base.push_str("artifact");}format!("{base}{suffix}")
}
pub(crate) fn validate(task: &Task) -> Result<(), String> {
    crate::team::validate_task(task)?;
    crate::delivery::validate_task(task)?;
    if task.delivery_error.as_ref().is_some_and(|e|e.len()>2000){return Err("交付错误记录超限".into())}
    crate::sources::validate_inputs(&task.source_inputs)?;
    if task.artifacts.iter().any(|a|a.source_input_ids.len()>64||a.source_input_ids.iter().any(|id|!task.source_inputs.iter().any(|i|&i.id==id))) {return Err("交付来源记录无效".into())}
    if let Some(q)=&task.queue {
        if task.archived || q.request_id.is_empty() || q.request_id.len()>100 || q.next_run_id.is_empty() || q.next_run_id.len()>100 || q.order==0 || q.order>9_007_199_254_740_991 || !matches!(q.state.as_str(),"pending"|"claimed") || q.error.as_ref().is_some_and(|e|e.chars().count()>2000) {return Err("等待请求格式无效".into());}
        if let QueueAction::Continue{text,run_id,turn_id}=&q.action {if text.trim().is_empty() || text.chars().count()>2000 || run_id.as_ref().is_none_or(|r|r.is_empty()||r.len()>100) || turn_id.as_ref().is_some_and(|t|t.len()>100){return Err("续接请求格式无效".into());}}
    }
    if task.acceptance.as_ref().is_some_and(|a|a.artifact_ids.len()>10 || a.artifact_ids.iter().any(|id|id.len()>400)) {return Err("验收记录格式无效".into());}

    if let Some(session) = &task.session_ref {
        if session.provider != task.provider
            || session.id.is_empty()
            || session.id.len() > 400
            || session.protocol.is_empty()
            || session.protocol.len() > 64
            || session.cwd.as_ref().is_some_and(|p| p.len() > 4096)
            || serde_json::to_vec(session)
                .map_err(|_| "会话引用无效")?
                .len()
                > 16 * 1024
        {
            return Err("执行器与会话引用不一致或超出限制".into());
        }
    }
    if task.archived && if task.parent_link.is_some(){!task.terminal()}else{task.status != "completed" || task.artifacts.is_empty()} {
        return Err("归档记录缺少已完成的交付".into());
    }
    if task.conversation.iter().any(|x|x.source_input_ids.len()>64||x.source_input_ids.iter().any(|id|!task.source_inputs.iter().any(|i|&i.id==id))) {return Err("消息来源记录无效".into())}
    let mut conversation_ids = std::collections::HashSet::new();
    if task.conversation.iter().any(|x| !conversation_ids.insert((&x.run_id, &x.thread_id, &x.item_id))) {
        return Err("对话记录包含重复消息标识".into());
    }
    if task.agent_activity_ids.len() > crate::protocol::MAX_AGENT_ACTIVITIES
        || task.agent_activity_ids.iter().any(|id| id.len() > 400)
    {
        return Err("Agent 活动记录超过大小限制".into());
    }

    if task.requested_model.as_deref().is_some_and(|id| !crate::executor::valid_model_id(id))
        || !crate::executor::valid_provider(&task.provider)
        || !matches!(task.scene.as_str(), "research" | "coding" | "writing")
        || !matches!(
            task.status.as_str(),
            "queued"
                | "running"
                | "approval"
                | "cancelling"
                | "completed"
                | "failed"
                | "interrupted"
                | "unknown"
        )
        || task.id.len() > 100
        || task.title.len() > 500
        || task.prompt.len() > 48000
        || task.conversation.len() > crate::conversation::ITEM_LIMIT
        || task.conversation.iter().map(|x| x.text.chars().count()).sum::<usize>() > crate::conversation::TEXT_LIMIT
        || task.conversation.iter().any(|x| x.run_id.is_empty() || x.run_id.len() > 100 || x.thread_id.is_empty() || x.thread_id.len() > 400 || x.item_id.is_empty() || x.item_id.len() > 128 || x.title.len() > 100 || x.text.chars().count() > crate::conversation::ITEM_TEXT_LIMIT || !matches!(x.kind.as_str(), "assistant" | "tool") || !matches!(x.status.as_str(), "running" | "completed" | "failed" | "unknown"))
        || task.nodes.len() > 64
        || task.events.len() > 100
        || task.artifacts.len() > 10
        || task.answer_items.len() > crate::model::ANSWER_LIMIT
        || task.answer_items.iter().any(|item| item.id.len() > 200)
        || task
            .answer_items
            .iter()
            .map(|item| item.text.chars().count())
            .sum::<usize>()
            > crate::model::answer_budget()
        || task.directions.len() > 20
        || task.directions.iter().any(|d| {
            d.id.len() > 200
                || d.run_id.len() > 100
                || d.turn_id.len() > 100
                || d.text.trim().is_empty()
                || d.text.chars().count() > 2000
                || !matches!(
                    d.status.as_str(),
                    "pending" | "accepted" | "rejected" | "unknown"
                )
        })
        || task.supplements.len() > 10
        || task.supplements.iter().any(|s| {
            s.source_thread_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 100)
                || s.run_id.is_empty()
                || s.run_id.len() > 100
                || s.previous_turn_id.is_empty()
                || s.previous_turn_id.len() > 100
                || s.text.trim().is_empty()
                || s.text.chars().count() > 2000
        })
    {
        return Err("任务记录超出允许范围".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("orbit-store-test-{}", uuid::Uuid::new_v4()))
    }
    fn document() -> Task {
        let mut task = Task::new("Doc".into(), "Doc".into(), "writing".into());
        task.status = "completed".into();
        task.artifacts.push(crate::model::Artifact {
            id: "doc".into(),
            name: "doc.md".into(),
            kind: "markdown".into(),
            content: "# Original".into(),
            source_input_ids: vec![], created_at: 0,
        });
        task.acceptance=Some(Acceptance{run_id:task.run_id.clone(),turn_id:task.turn_id.clone(),artifact_ids:vec!["doc".into()]});
        task
    }
    #[test]
    fn export_folder_settings_are_atomic_and_invalid_folder_does_not_hide_tasks() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let task = document();
        store.save_task(task.clone()).unwrap();
        let chosen = directory.with_extension("outside exports");
        fs::create_dir(&chosen).unwrap();
        let canonical = chosen.canonicalize().unwrap();
        assert!(!store.export_settings().custom);
        assert!(store
            .set_export_directory(Some(PathBuf::from("relative")))
            .is_err());
        assert!(store
            .set_export_directory(Some(directory.join("missing")))
            .is_err());
        assert!(store
            .set_export_directory(Some(directory.join("workspace.json")))
            .is_err());
        fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(store.set_export_directory(Some(chosen.clone())).is_err());
        assert!(!store.export_settings().custom);
        assert_eq!(fs::read_dir(&chosen).unwrap().count(), 0);
        fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        assert!(
            store
                .set_export_directory(Some(chosen.clone()))
                .unwrap()
                .custom
        );
        store.save_task(task.clone()).unwrap();
        let reopened = Store::open(directory.clone()).unwrap();
        assert_eq!(
            PathBuf::from(reopened.export_settings().directory),
            canonical
        );
        reopened.archive_task(&task.id).unwrap();
        let first = PathBuf::from(reopened.export_artifact("doc").unwrap());
        let second = PathBuf::from(reopened.export_artifact("doc").unwrap());
        assert_eq!(first.parent().unwrap(), canonical);
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(&first).unwrap(), "# Original");
        fs::remove_dir_all(&chosen).unwrap();
        let offline = Store::open(directory.clone()).unwrap();
        assert_eq!(offline.workspace().tasks.len(), 1);
        assert!(offline.workspace().error.is_none());
        assert!(offline.export_settings().custom);
        assert!(offline.export_artifact("doc").unwrap_err().contains("失效"));
        offline.set_export_directory(None).unwrap();
        let default = PathBuf::from(offline.export_artifact("doc").unwrap());
        assert_eq!(
            default.parent().unwrap(),
            directory.canonicalize().unwrap().join("exports")
        );
        assert!(
            !Store::open(directory.clone())
                .unwrap()
                .export_settings()
                .custom
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn export_rejects_replaced_custom_and_redirected_default_folders() {
        use std::os::unix::fs::symlink;
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        store.save_task(document()).unwrap();
        let chosen = directory.join("chosen");
        let other = directory.with_extension("other");
        fs::create_dir(&chosen).unwrap();
        fs::create_dir(&other).unwrap();
        store.set_export_directory(Some(chosen.clone())).unwrap();
        fs::remove_dir(&chosen).unwrap();
        symlink(&other, &chosen).unwrap();
        assert!(store.export_artifact("doc").is_err());
        assert!(store.export_workspace().is_err());
        assert_eq!(fs::read_dir(&other).unwrap().count(), 0);
        store.set_export_directory(None).unwrap();
        symlink(&other, directory.join("exports")).unwrap();
        assert!(store.export_artifact("doc").is_err());
        assert!(store.export_workspace().is_err());
        assert_eq!(fs::read_dir(&other).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
        fs::remove_dir_all(other).unwrap();
    }
    #[test]
    fn configured_export_folder_is_used_after_reopen() {
        let directory = directory();
        drop(Store::open(directory.clone()).unwrap());
        let chosen = directory.join("chosen 中文 folder");
        fs::create_dir(&chosen).unwrap();
        let task = document();
        fs::write(
            directory.join("workspace.json"),
            serde_json::to_vec(&serde_json::json!({
            "version":1,"tasks":[task],"exportDirectory":chosen.canonicalize().unwrap()
            }))
            .unwrap(),
        )
        .unwrap();
        let store = Store::open(directory.clone()).unwrap();
        assert!(store.workspace().error.is_none());
        let exported = PathBuf::from(store.export_artifact("doc").unwrap());
        assert_eq!(exported.parent().unwrap(), chosen.canonicalize().unwrap());
        assert_eq!(fs::read_to_string(exported).unwrap(), "# Original");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn conversation_survives_storage_and_recovery_without_losing_completed_messages() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let mut task = Task::new("Chat".into(), "Goal".into(), "research".into());
        task.thread_id = Some("root".into()); task.turn_id = Some("turn".into());
        crate::protocol::project(&mut task, &serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"type":"agentMessage","id":"answer","text":"public reply"}}}));
        crate::protocol::project(&mut task, &serde_json::json!({"method":"item/agentMessage/delta","params":{"threadId":"root","turnId":"turn","itemId":"progress","delta":"working"}}));
        store.save_task(task.clone()).unwrap();
        let reopened = Store::open(directory.clone()).unwrap().task(&task.id).unwrap();
        assert_eq!(reopened.status, "unknown");
        assert_eq!(reopened.conversation[0].status, "completed");
        assert_eq!(reopened.conversation[0].text, "public reply");
        assert_eq!(reopened.conversation[1].status, "unknown");
        let mut duplicate = reopened.clone(); duplicate.conversation.push(duplicate.conversation[0].clone());
        assert!(validate(&duplicate).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn completed_message_is_persisted_before_any_later_turn_event() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let mut task = document();
        task.status = "running".into();
        task.thread_id = Some("root".into());
        task.turn_id = Some("turn".into());
        task.root_node();
        let mut child = task.nodes[0].clone();
        child.id = "child".into();
        child.parent_id = Some("root".into());
        child.role = "子 Agent".into();
        task.nodes.push(child);
        store.save_task(task.clone()).unwrap();
        for (thread, expected) in [
            ("root", "complete root answer"),
            ("child", "complete child answer"),
        ] {
            assert!(crate::protocol::project(
                &mut task,
                &serde_json::json!({"method":"item/completed","params":{"threadId":thread,"turnId":"turn","item":{"id":format!("answer-{thread}"),"type":"agentMessage","phase":"final_answer","text":expected}}})
            ));
            let saved = store
                .save_existing_task(task.clone())
                .unwrap()
                .expect("complete message must advance the saved revision immediately");
            assert_eq!(
                saved.nodes.iter().find(|n| n.id == thread).unwrap().output,
                expected
            );
            let reopened = Store::open(directory.clone())
                .unwrap()
                .task(&task.id)
                .unwrap();
            assert_eq!(
                reopened
                    .nodes
                    .iter()
                    .find(|n| n.id == thread)
                    .unwrap()
                    .output,
                expected
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn continuation_admission_is_atomic_and_old_runs_cannot_replace_versions() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let mut task = document();
        task.thread_id = Some("root".into());
        task.turn_id = Some("first".into());
        store.save_task(task.clone()).unwrap();
        let task = store
            .edit_artifact("doc", "# Original", "# Edited original")
            .unwrap();
        let bytes = fs::read(directory.join("workspace.json")).unwrap();
        fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(store
            .continue_task(
                &task.id,
                task.revision,
                &task.run_id,
                &task.turn_id,
                "more details"
            )
            .is_err());
        assert_eq!(store.task(&task.id).unwrap().run_id, task.run_id);
        assert_eq!(fs::read(directory.join("workspace.json")).unwrap(), bytes);
        fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        let (mut next, _) = store
            .continue_task(
                &task.id,
                task.revision,
                &task.run_id,
                &task.turn_id,
                "more details",
            )
            .unwrap();
        assert!(store
            .continue_task(
                &task.id,
                task.revision,
                &task.run_id,
                &task.turn_id,
                "duplicate"
            )
            .is_err());
        let mut late = task.clone();
        late.revision = next.revision + 100;
        assert!(store.save_existing_task(late).unwrap().is_none());
        next.turn_id = Some("second".into());
        crate::protocol::project(
            &mut next,
            &serde_json::json!({"method":"item/completed","params":{"threadId":"root","turnId":"second","item":{"id":"answer","type":"agentMessage","phase":"final_answer","text":crate::delivery::fixture("new version")}}}),
        );
        crate::protocol::project(
            &mut next,
            &serde_json::json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"second","status":"completed"}}}),
        );
        store.save_existing_task(next).unwrap().unwrap();
        let reopened = Store::open(directory.clone())
            .unwrap()
            .task(&task.id)
            .unwrap();
        assert_eq!(reopened.artifacts.len(), 2);
        assert_eq!(reopened.artifacts[0].id, "doc");
        assert_eq!(reopened.artifacts[0].content, "# Edited original");
        assert_eq!(reopened.artifacts[1].content, "new version");
        assert_eq!(reopened.supplements[0].text, "more details");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn legacy_demo_history_is_retired_without_losing_real_documents() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let real = document();
        let mut demo = document();
        demo.id = "retired-demo".into();
        demo.provider = "demo".into();
        demo.artifacts[0].content = "sample content".into();
        let file = directory.join("workspace.json");
        let bytes = serde_json::to_vec(&serde_json::json!({"exportDirectory":null,"version":1,"tasks":[demo.clone(),real.clone()]}))
        .unwrap();
        fs::write(&file, &bytes).unwrap();
        let store = Store::open(directory.clone()).unwrap();
        assert!(store.workspace().error.is_none());
        assert_eq!(store.workspace().tasks.len(), 1);
        assert_eq!(
            store.task(&real.id).unwrap().artifacts[0].content,
            "# Original"
        );
        assert_eq!(fs::read(&file).unwrap(), bytes);
        store
            .edit_artifact("doc", "# Original", "# Real edited")
            .unwrap();
        let exported = store.export_artifact("doc").unwrap();
        assert_eq!(fs::read_to_string(exported).unwrap(), "# Real edited");
        let reopened = Store::open(directory.clone()).unwrap();
        assert_eq!(reopened.workspace().tasks.len(), 1);
        assert_eq!(
            reopened.task(&real.id).unwrap().artifacts[0].content,
            "# Real edited"
        );
        let disk: Disk = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert!(disk.tasks.iter().all(|t| t.provider == "codex"));
        let bytes = serde_json::to_vec(&serde_json::json!({"exportDirectory":null,"version":1,"tasks":[demo]}))
        .unwrap();
        fs::write(&file, &bytes).unwrap();
        let empty = Store::open(directory.clone()).unwrap();
        assert!(empty.workspace().error.is_none());
        assert!(empty.workspace().tasks.is_empty());
        assert_eq!(fs::read(&file).unwrap(), bytes);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn unknown_or_invalid_real_history_remains_protected_after_demo_retirement() {
        for invalid_provider in [true, false] {
            let directory = directory();
            fs::create_dir_all(&directory).unwrap();
            let mut real = document();
            if invalid_provider {
                real.provider = "bad/provider".into();
            } else {
                real.status = "invalid".into();
            }
            let mut demo = document();
            demo.provider = "demo".into();
            let bytes = serde_json::to_vec(&serde_json::json!({"exportDirectory":null,"version":1,"tasks":[demo,real]}))
            .unwrap();
            let file = directory.join("workspace.json");
            fs::write(&file, &bytes).unwrap();
            let store = Store::open(directory.clone()).unwrap();
            assert!(store.workspace().error.is_some());
            assert!(store.save_task(document()).is_err());
            assert_eq!(fs::read(&file).unwrap(), bytes);
            fs::remove_dir_all(directory).unwrap();
        }
    }
    #[test]
    fn executor_history_preserves_qoder_unknown_and_legacy_codex_records() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let mut task = document();
        task.provider = "qoder".into();
        store.save_task(task.clone()).unwrap();
        assert_eq!(store.task(&task.id).unwrap().provider, "qoder");
        let mut unknown = document();
        unknown.provider = "future-provider".into();
        let mut value = serde_json::to_value(&unknown).unwrap();
        value["sessionRef"] = serde_json::json!({"provider":"future-provider","protocol":"future-v3","id":"session","metadata":{"opaque":"keep"},"extra":{"preserve":true}});
        unknown = serde_json::from_value(value).unwrap();
        store.save_task(unknown.clone()).unwrap();
        let reopened = Store::open(directory.clone()).unwrap();
        let saved = serde_json::to_value(reopened.task(&unknown.id).unwrap()).unwrap();
        assert_eq!(saved["sessionRef"]["metadata"]["opaque"], "keep");
        assert_eq!(saved["sessionRef"]["extra"]["preserve"], true);
        let mut legacy = serde_json::to_value(document()).unwrap();
        legacy.as_object_mut().unwrap().remove("provider");
        let legacy: Task = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.provider, "codex");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn archived_history_flag_survives_load_and_missing_flag_defaults_false() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let mut task = serde_json::to_value(document()).unwrap();
        task["archived"] = serde_json::json!(true);
        fs::write(
            directory.join("workspace.json"),
            serde_json::to_vec(&serde_json::json!({"version":1,"tasks":[task]})).unwrap(),
        )
        .unwrap();
        let store = Store::open(directory.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&store.workspace().tasks[0]).unwrap()["archived"],
            true
        );
        let mut legacy = serde_json::to_value(document()).unwrap();
        legacy.as_object_mut().unwrap().remove("archived");
        let legacy: Task = serde_json::from_value(legacy).unwrap();
        assert_eq!(serde_json::to_value(legacy).unwrap()["archived"], false);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn archive_export_delete_is_persistent_and_late_snapshots_cannot_restore_it() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let task = document();
        store.save_task(task.clone()).unwrap();
        assert!(store.delete_task(&task.id).is_err());
        let archived = store.archive_task(&task.id).unwrap();
        assert!(archived.archived);
        assert_eq!(
            store.archive_task(&task.id).unwrap().revision,
            archived.revision
        );
        assert!(store.edit_artifact("doc", "# Original", "changed").is_err());
        assert!(store
            .sync_agents(
                &task.id,
                "run",
                "turn",
                &crate::protocol::AgentHistory {
                    root: serde_json::Value::Null,
                    details: vec![]
                }
            )
            .unwrap_err()
            .contains("归档"));
        let path = store.export_artifact("doc").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Original");
        let mut late = task.clone();
        late.revision = 100;
        assert!(store.save_existing_task(late.clone()).unwrap().is_none());
        assert!(
            Store::open(directory.clone())
                .unwrap()
                .task(&task.id)
                .unwrap()
                .archived
        );
        store.delete_task(&task.id).unwrap();
        assert!(store.save_existing_task(late).unwrap().is_none());
        assert!(Store::open(directory.clone())
            .unwrap()
            .workspace()
            .tasks
            .is_empty());
        assert!(std::path::Path::new(&path).exists());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn archive_qualification_and_disk_failures_leave_state_unchanged() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let mut task = document();
        task.status = "running".into();
        store.save_task(task.clone()).unwrap();
        assert!(store.archive_task(&task.id).is_err());
        task.status = "completed".into();
        task.artifacts.clear();
        store.save_task(task.clone()).unwrap();
        assert!(store.archive_task(&task.id).is_err());
        task.artifacts = document().artifacts;
        store.save_task(task.clone()).unwrap();
        fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(store.archive_task(&task.id).is_err());
        assert!(!store.task(&task.id).unwrap().archived);
        fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        store.archive_task(&task.id).unwrap();
        fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(store.delete_task(&task.id).is_err());
        assert!(store.task(&task.id).unwrap().archived);
        assert!(
            Store::open(directory.clone())
                .unwrap()
                .task(&task.id)
                .unwrap()
                .archived
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn late_runtime_snapshots_preserve_edited_docs_and_require_current_run() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let task = document();
        store.save_task(task.clone()).unwrap();
        store
            .edit_artifact("doc", "# Original", "# Edited")
            .unwrap();
        let mut late = task.clone();
        late.revision = 100;
        let saved = store.save_existing_task(late.clone()).unwrap().unwrap();
        assert_eq!(saved.artifacts[0].content, "# Edited");
        late.revision = 101;
        late.run_id = Some("wrong-run".into());
        assert!(store.save_existing_task(late).unwrap().is_none());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn full_workspace_rejects_creation_and_retains_archived_deliveries() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let delivered = document();
        store.save_task(delivered.clone()).unwrap();
        store.archive_task(&delivered.id).unwrap();
        for _ in 0..49 {
            store
                .save_task(Task::new("Task".into(), "Goal".into(), "research".into()))
                .unwrap();
        }
        assert!(store
            .save_task(Task::new(
                "Overflow".into(),
                "Goal".into(),
                "research".into()
            ))
            .is_err());
        assert_eq!(store.workspace().tasks.len(), 50);
        let mut current = store.workspace().tasks[0].clone();
        current.revision += 1;
        current.title = "Updated at capacity".into();
        store.save_task(current.clone()).unwrap();
        assert_eq!(
            store.task(&current.id).unwrap().title,
            "Updated at capacity"
        );
        let archived = store.task(&delivered.id).unwrap();
        assert!(archived.archived);
        assert_eq!(archived.artifacts[0].content, "# Original");
        assert!(
            Store::open(directory.clone())
                .unwrap()
                .task(&delivered.id)
                .unwrap()
                .archived
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn edits_persist_export_and_reject_stale_or_ambiguous_content() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let task = document();
        store.save_task(task.clone()).unwrap();
        let next = store
            .edit_artifact("doc", "# Original", "# Edited")
            .unwrap();
        assert_eq!(next.status, "completed");
        assert!(next.revision > task.revision);
        assert!(store.edit_artifact("doc", "# Original", "stale").is_err());
        assert_eq!(
            fs::read_to_string(store.export_artifact("doc").unwrap()).unwrap(),
            "# Edited"
        );
        assert_eq!(
            Store::open(directory.clone())
                .unwrap()
                .task(&task.id)
                .unwrap()
                .artifacts[0]
                .content,
            "# Edited"
        );
        let mut other = document();
        other.id = "other".into();
        store.save_task(other).unwrap();
        assert!(store.edit_artifact("doc", "# Edited", "ambiguous").is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn failed_edit_does_not_commit_and_utf8_limits_are_bytes() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let task = document();
        store.save_task(task.clone()).unwrap();
        assert!(store
            .edit_artifact("doc", "# Original", &"字".repeat(85334))
            .is_err());
        fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(store.edit_artifact("doc", "# Original", "new").is_err());
        assert_eq!(
            store.task(&task.id).unwrap().artifacts[0].content,
            "# Original"
        );
        fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        assert_eq!(
            store
                .edit_artifact("doc", "# Original", "")
                .unwrap()
                .artifacts[0]
                .content,
            ""
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn collaboration_sync_is_atomic_preserves_edits_and_rejects_changed_run() {
        let directory = std::env::temp_dir().join(format!("orbit-sync-{}", uuid::Uuid::new_v4()));
        let store = Store::open(directory.clone()).unwrap();
        let history = crate::protocol::AgentHistory {
            root: serde_json::from_str(include_str!("../fixtures/subagent-history.json")).unwrap(),
            details: vec![],
        };
        let mut task = document();
        task.thread_id = Some(history.root["id"].as_str().unwrap().into());
        task.turn_id = Some(history.root["turns"][0]["id"].as_str().unwrap().into());
        task.root_node();
        task.nodes[0].status = "completed".into();
        store.save_task(task.clone()).unwrap();
        let edited = store
            .edit_artifact("doc", "# Original", "# Edited")
            .unwrap();
        std::fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(store
            .sync_agents(
                &task.id,
                task.run_id.as_deref().unwrap(),
                task.turn_id.as_deref().unwrap(),
                &history
            )
            .is_err());
        assert_eq!(store.task(&task.id).unwrap().revision, edited.revision);
        std::fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        assert!(store
            .sync_agents(
                &task.id,
                "old-run",
                task.turn_id.as_deref().unwrap(),
                &history
            )
            .is_err());
        let saved = store
            .sync_agents(
                &task.id,
                task.run_id.as_deref().unwrap(),
                task.turn_id.as_deref().unwrap(),
                &history,
            )
            .unwrap();
        assert_eq!(saved.nodes.len(), 4);
        assert_eq!(saved.artifacts[0].content, "# Edited");
        assert_eq!(
            store
                .sync_agents(
                    &task.id,
                    task.run_id.as_deref().unwrap(),
                    task.turn_id.as_deref().unwrap(),
                    &history
                )
                .unwrap()
                .revision,
            saved.revision
        );
        assert_eq!(
            Store::open(directory.clone())
                .unwrap()
                .task(&task.id)
                .unwrap()
                .nodes
                .len(),
            4
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn late_snapshot_cannot_replace_newer_state_and_restart_marks_unknown() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let mut task = Task::new("T".into(), "P".into(), "research".into());
        task.revision = 8;
        store.save_task(task.clone()).unwrap();
        let mut old = task.clone();
        old.revision = 2;
        old.status = "queued".into();
        store.save_task(old).unwrap();
        assert_eq!(store.task(&task.id).unwrap().revision, 8);
        drop(store);
        let restored = Store::open(directory.clone()).unwrap();
        assert_eq!(restored.task(&task.id).unwrap().status, "unknown");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn corrupt_snapshot_is_preserved_and_cannot_be_overwritten() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("workspace.json"), "broken").unwrap();
        let store = Store::open(directory.clone()).unwrap();
        assert!(store.workspace().error.is_some());
        assert!(store.save_task(document()).is_err());
        assert_eq!(
            fs::read_to_string(directory.join("workspace.json")).unwrap(),
            "broken"
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn workspace_export_keeps_all_providers_archives_and_current_details() {
        let dir = directory();
        let store = Store::open(dir.clone()).unwrap();
        let chosen = dir.join("chosen");
        fs::create_dir(&chosen).unwrap();
        store.set_export_directory(Some(chosen.clone())).unwrap();
        let mut archived = document();
        archived.id = "archived-task".into();
        archived.archived = true;
        archived.artifacts[0].content = "# Edited archived delivery\n你好".into();
        store.save_task(archived).unwrap();
        let mut current = Task::new("Research".into(), "Original goal".into(), "research".into());
        current.id = "current-task".into();
        current.provider = "qoder".into();
        current.session_ref = Some(crate::executor::SessionRef {
            provider: "qoder".into(),
            protocol: "acp-v1".into(),
            id: "session".into(),
            cwd: Some("/some/original/task".into()),
            metadata: serde_json::json!({"taskId":"current-task"}),
            extra: [("futureField".into(), serde_json::json!({"retained":true}))].into(),
        });
        current.capabilities = Some(crate::executor::Capabilities::acp(true));
        current.root_node();
        current.nodes[0].output = "Partial current answer".into();
        current.turn_id = Some("current-turn".into());
        current.event("Observed tool progress", "tool", "Qoder");
        current.directions.push(crate::model::Direction {
            id: "input".into(),
            run_id: current.run_id.clone().unwrap(),
            turn_id: "current-turn".into(),
            text: "Add evidence".into(),
            status: "pending".into(),
            created_at: 1,
        });
        current.supplements.push(crate::model::Supplement {
            source_thread_id: None,
            run_id: current.run_id.clone().unwrap(),
            previous_turn_id: "previous".into(),
            text: "More details".into(),
            created_at: 1,
        });
        current.approvals.push(crate::model::Approval {
            id: "question".into(),
            request_id: "rpc".into(),
            run_id: current.run_id.clone().unwrap(),
            turn_id: "current-turn".into(),
            title: "Confirm scope".into(),
            description: "Current request".into(),
            kind: "input".into(),
            question_ids: vec!["scope".into()],
            questions: vec![],
            question_error: None,
        });
        store.save_task(current).unwrap();
        let mut unknown = Task::new("Other".into(), "Keep history".into(), "writing".into());
        unknown.id = "unknown-task".into();
        unknown.provider = "future-executor".into();
        unknown.status = "queued".into();
        store.save_task(unknown).unwrap();
        let original = fs::read(dir.join("workspace.json")).unwrap();
        let first = PathBuf::from(store.export_workspace().unwrap());
        let second = PathBuf::from(store.export_workspace().unwrap());
        assert_ne!(first, second);
        assert_eq!(first.parent().unwrap(), chosen.canonicalize().unwrap());
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
        assert_eq!(value["format"], "orbit-workspace");
        assert_eq!(value["version"], 5);
        assert!(value["exportedAt"].as_u64().unwrap() > 0);
        assert_eq!(
            value["exportDirectory"],
            serde_json::json!(chosen.canonicalize().unwrap())
        );
        assert_eq!(value["tasks"].as_array().unwrap().len(), 3);
        let tasks = value["tasks"].as_array().unwrap();
        let current = tasks.iter().find(|t| t["id"] == "current-task").unwrap();
        assert_eq!(current["status"], "running");
        assert_eq!(current["provider"], "qoder");
        assert_eq!(current["nodes"][0]["output"], "Partial current answer");
        assert_eq!(current["events"][0]["text"], "Observed tool progress");
        assert_eq!(current["directions"][0]["text"], "Add evidence");
        assert_eq!(current["supplements"][0]["text"], "More details");
        assert_eq!(current["approvals"][0]["title"], "Confirm scope");
        assert_eq!(current["sessionRef"]["futureField"]["retained"], true);
        assert_eq!(current["capabilities"]["resume"], true);
        let archived = tasks.iter().find(|t| t["id"] == "archived-task").unwrap();
        assert_eq!(archived["archived"], true);
        assert_eq!(
            archived["artifacts"][0]["content"],
            "# Edited archived delivery\n你好"
        );
        assert!(tasks.iter().any(|t| t["provider"] == "future-executor"));
        assert_eq!(fs::read(dir.join("workspace.json")).unwrap(), original);
        assert_eq!(store.task("current-task").unwrap().status, "running");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&first).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn workspace_export_is_consistent_during_updates() {
        let dir = directory();
        let store = std::sync::Arc::new(Store::open(dir.clone()).unwrap());
        let task = document();
        let id = task.id.clone();
        store.save_task(task).unwrap();
        let writer_store = store.clone();
        let writer = std::thread::spawn(move || {
            for version in 1..=20 {
                let mut task = writer_store.task(&id).unwrap();
                task.title = format!("Version {version}");
                task.artifacts[0].content = format!("# Version {version}");
                task.event("update", "system", "workbench");
                writer_store.save_task(task).unwrap();
            }
        });
        for _ in 0..10 {
            let path = store.export_workspace().unwrap();
            let snapshot: serde_json::Value =
                serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            let task = &snapshot["tasks"][0];
            let title = task["title"].as_str().unwrap();
            if title == "Doc" {
                assert_eq!(task["artifacts"][0]["content"], "# Original");
            } else {
                assert_eq!(task["artifacts"][0]["content"], format!("# {title}"));
            }
        }
        writer.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn workspace_export_allows_healthy_empty_and_rejects_broken_source() {
        let dir = directory();
        let store = Store::open(dir.clone()).unwrap();
        let path = PathBuf::from(store.export_workspace().unwrap());
        assert_eq!(
            path.parent().unwrap(),
            dir.join("exports").canonicalize().unwrap()
        );
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(value["tasks"], serde_json::json!([]));
        assert!(!dir.join("workspace.json").exists());
        fs::remove_dir_all(&dir).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("workspace.json"), "broken source").unwrap();
        let broken = Store::open(dir.clone()).unwrap();
        assert!(broken.export_workspace().unwrap_err().contains("原记录"));
        assert_eq!(
            fs::read_to_string(dir.join("workspace.json")).unwrap(),
            "broken source"
        );
        assert!(!dir.join("exports").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn export_uses_owned_text_and_demo_input_is_rejected() {
        let directory = directory();
        let store = Store::open(directory.clone()).unwrap();
        let mut task = Task::new("T".into(), "P".into(), "research".into());
        task.artifacts.push(crate::model::Artifact {
            id: "result".into(),
            name: "../../report.md".into(),
            kind: "markdown".into(),
            content: "safe text".into(),
            source_input_ids: vec![], created_at: 0,
        });
        store.save_task(task.clone()).unwrap();
        let mut fake = task;
        fake.provider = "demo".into();
        assert!(store.save_task(fake).is_err());
        let path = PathBuf::from(store.export_artifact("result").unwrap());
        assert!(path.starts_with(directory.join("exports").canonicalize().unwrap()));
        assert_eq!(fs::read_to_string(path).unwrap(), "safe text");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn complete_export_includes_library_bodies_drafts_and_pdf_bytes() {
        use crate::knowledge::{LibraryStore,NewDocument,Kind};
        let directory=std::env::temp_dir().join(format!("orbit-full-library-{}",uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let library=LibraryStore::open(directory.clone()).unwrap();
        let doc=library.create(NewDocument{title:"知识库笔记".into(),kind:Kind::Markdown,content:"独立文档".into(),url:None,tags:vec!["研究".into()]}).unwrap();
        let source=directory.join("input.pdf");fs::write(&source,b"%PDF-1.4\nlocal PDF").unwrap();let pdf=library.import_path(&source).unwrap();
        let store=Store::open(directory.clone()).unwrap();
        let committed=store.library.change(crate::knowledge::Change{document_id:doc.id.clone(),expected_revision:doc.revision,session_id:"export-test".into(),sequence:1,operation:"commit".into(),content:"已提交正文".into(),version_id:None,title:None,tags:None}).unwrap();
        store.library.change(crate::knowledge::Change{document_id:doc.id.clone(),expected_revision:committed.revision,session_id:"export-test".into(),sequence:2,operation:"draft".into(),content:"已存草稿".into(),version_id:None,title:None,tags:None}).unwrap();
        let collection=store.library.save_collection(crate::knowledge::Collection{name:"Exported collection".into(),..Default::default()}).unwrap();
        let organized=store.library.organize(&pdf.id,pdf.revision,vec![collection.id.clone()]).unwrap();
        let trashed=store.library.trash(&pdf.id,organized.revision).unwrap();
        let exported=store.export_workspace().unwrap();let value:serde_json::Value=serde_json::from_slice(&fs::read(exported).unwrap()).unwrap();
        assert_eq!(value["version"],5);assert!(value["library"]["records"]["documents"].as_array().unwrap().iter().any(|d|d["id"]==doc.id&&d["content"]=="已提交正文"&&d["draft"]["content"]=="已存草稿"&&d["versions"][0]["content"]=="独立文档"));
        assert_eq!(value["library"]["records"]["schemaVersion"],3);
        assert_eq!(value["library"]["records"]["collections"][0]["id"],collection.id);
        assert!(value["library"]["records"]["documents"].as_array().unwrap().iter().any(|d|d["id"]==pdf.id&&d["deletedAt"].is_number()&&d["revision"]==trashed.revision));
        assert_eq!(value["library"]["attachments"][&pdf.id],"JVBERi0xLjQKbG9jYWwgUERG");
        fs::remove_file(directory.join("library-blobs").join(format!("{}.pdf",pdf.id))).unwrap();assert!(store.export_workspace().is_err());
        fs::write(directory.join("library.json"),"broken library").unwrap();let damaged=Store::open(directory.clone()).unwrap();assert!(damaged.export_workspace().is_err());
        let _=fs::remove_dir_all(directory);
    }

    #[test]
    fn library_export_preserves_long_unicode_titles_with_valid_file_names(){
        use crate::knowledge::{NewDocument,Kind};
        let directory=directory();let store=Store::open(directory.clone()).unwrap();
        let doc=store.library.create(NewDocument{title:"中文".repeat(64),kind:Kind::Markdown,content:"正文".into(),url:None,tags:vec![]}).unwrap();
        let path=store.export_document(&doc.id).unwrap();assert!(path.ends_with(".md"));assert_eq!(fs::read_to_string(path).unwrap(),"正文");assert_eq!(store.library.get(&doc.id).unwrap().title,"中文".repeat(64));fs::remove_dir_all(directory).unwrap();
    }

}

#[cfg(test)]
mod workflow_tests {
    use super::*;
    use crate::model::{QueueAction,Artifact};
    fn fresh(provider:&str)->Task { let mut t=Task::new("queue".into(),"goal".into(),"research".into());t.provider=provider.into();t.status="queued".into();t.run_id=None;t.started_at=None;t }
    fn store()->Store { Store::open(std::env::temp_dir().join(format!("orbit-queue-{}",uuid::Uuid::new_v4()))).unwrap() }
    #[test]
    fn fifo_claim_recovery_and_old_writer_protection() {
        let s=store();let a=fresh("qoder");let b=fresh("codex");s.save_task(a.clone()).unwrap();s.save_task(b.clone()).unwrap();
        let queued=s.enqueue(&a.id,a.revision,QueueAction::Start).unwrap();
        assert!(s.enqueue(&a.id,queued.revision,QueueAction::Start).is_err());
        s.enqueue(&b.id,b.revision,QueueAction::Start).unwrap();
        let (claimed,anchor)=s.claim_next().unwrap().unwrap();assert_eq!(claimed.id,a.id);assert!(anchor.is_none());
        let q=claimed.queue.clone().unwrap();assert_eq!(claimed.run_id.as_ref(),Some(&q.next_run_id));assert_eq!(q.state,"claimed");
        let mut stale=a.clone();stale.revision=100;assert!(s.save_existing_task(stale).unwrap().is_none());
        let reopened=Store::open(s.directory.clone()).unwrap();let recovered=reopened.task(&a.id).unwrap();assert_eq!(recovered.status,"unknown");assert_eq!(recovered.queue.unwrap().state,"claimed");
        assert!(reopened.claim_next().is_err());
        let saved=s.finish_claim(&a.id,&q.request_id,&q.next_run_id,None).unwrap();assert!(saved.queue.is_none());
        let mut event=claimed.clone();event.event("progress","system","agent");event.revision=saved.revision+1;
        assert!(s.save_existing_task(event).unwrap().unwrap().queue.is_none());
        std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test]
    fn acceptance_is_version_bound_and_edit_invalidates_it() {
        let s=store();let mut t=Task::new("review".into(),"goal".into(),"research".into());t.status="completed".into();t.turn_id=Some("turn".into());t.thread_id=Some("thread".into());
        t.artifacts.push(Artifact{id:"artifact".into(),name:"result.md".into(),kind:"markdown".into(),content:"old".into(),source_input_ids: vec![], created_at:0});s.save_task(t.clone()).unwrap();
        assert!(s.accept_task(&t.id,t.revision,&t.run_id,&Some("wrong".into())).is_err());
        assert!(s.archive_task(&t.id).is_err());
        let accepted=s.accept_task(&t.id,t.revision,&t.run_id,&t.turn_id).unwrap();assert!(accepted.accepted());
        let mut late=t.clone();late.revision=accepted.revision+1;late.artifacts[0].content="unexpected".into();
        assert_eq!(s.save_existing_task(late).unwrap().unwrap().artifacts[0].content,"old");
        let edited=s.edit_artifact("artifact","old","edited").unwrap();assert!(!edited.accepted());
        let queue=s.enqueue(&t.id,edited.revision,QueueAction::Continue{text:"more".into(),run_id:t.run_id.clone(),turn_id:t.turn_id.clone()}).unwrap();
        assert!(s.archive_task(&t.id).is_err());assert!(s.edit_artifact("artifact","edited","changed").is_err());
        let cancelled=s.cancel_queued(&t.id,queue.revision).unwrap();assert!(cancelled.queue.is_none());
        std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test]
    fn platform_queue_revision_cannot_swallow_fast_executor_completion() {
        let s=store();let t=fresh("codex");s.save_task(t.clone()).unwrap();s.enqueue(&t.id,t.revision,QueueAction::Start).unwrap();
        let (mut actor,_)=s.claim_next().unwrap().unwrap();let q=actor.queue.clone().unwrap();
        let platform=s.finish_claim(&t.id,&q.request_id,&q.next_run_id,None).unwrap();
        actor.status="completed".into();actor.artifacts.push(Artifact{id:"fast".into(),name:"fast.md".into(),kind:"markdown".into(),content:"result".into(),source_input_ids: vec![], created_at:0});actor.event("completed","system","executor");
        assert_eq!(actor.revision,platform.revision);
        let completed=s.save_existing_task(actor.clone()).unwrap().expect("a genuine executor completion must survive platform bookkeeping");
        assert_eq!(completed.status,"completed");assert!(completed.queue.is_none());assert!(completed.revision>platform.revision);assert!(s.save_existing_task(actor).unwrap().is_none());
        std::fs::remove_dir_all(s.directory).unwrap();
    }
    #[test]
    fn write_failure_retains_request_and_claim_failure_is_not_retried() {
        let s=store();let t=fresh("codex");s.save_task(t.clone()).unwrap();s.enqueue(&t.id,t.revision,QueueAction::Start).unwrap();
        std::fs::create_dir(s.directory.join("workspace.tmp")).unwrap();assert!(s.claim_next().is_err());assert_eq!(s.task(&t.id).unwrap().queue.unwrap().state,"pending");
        std::fs::remove_dir(s.directory.join("workspace.tmp")).unwrap();
        let (claimed,_)=s.claim_next().unwrap().unwrap();let q=claimed.queue.unwrap();
        let failed=s.finish_claim(&t.id,&q.request_id,&q.next_run_id,Some("spawn failed".into())).unwrap();assert_eq!(failed.status,"unknown");assert!(failed.queue.as_ref().unwrap().error.is_some());assert!(s.claim_next().is_err());
        assert!(s.cancel_queued(&t.id,failed.revision).is_err());
        let preserved=s.task(&t.id).unwrap();assert_eq!(preserved.revision,failed.revision);assert_eq!(preserved.source_inputs,failed.source_inputs);assert_eq!(preserved.queue.as_ref().unwrap().request_id,q.request_id);
        let cancelled=s.cancel_claimed(&t.id,failed.revision,&q.request_id,&q.next_run_id).unwrap();assert!(cancelled.queue.is_none());
        std::fs::remove_dir_all(s.directory).unwrap();
    }
}


#[cfg(test)]
mod model_selection_tests {
    use super::*;
    #[test]
    fn requested_model_is_immutable_in_both_save_paths_and_survives_reopen() {
        let dir = std::env::temp_dir().join(format!("orbit-model-{}",uuid::Uuid::new_v4()));
        let store=Store::open(dir.clone()).unwrap();
        let mut task=Task::new("selected".into(),"goal".into(),"research".into());
        task.requested_model=Some("gpt-test".into());
        store.save_task(task.clone()).unwrap();
        let mut changed=task.clone(); changed.revision += 1; changed.requested_model=Some("other-model".into());
        assert!(store.save_task(changed.clone()).is_err());
        assert!(store.save_existing_task(changed.clone()).unwrap().is_none());
        changed.requested_model=task.requested_model.clone();
        assert!(store.save_existing_task(changed).unwrap().is_some());
        let mut invalid=Task::new("invalid".into(),"goal".into(),"research".into());
        invalid.requested_model=Some("bad\nmodel".into());
        assert!(store.save_task(invalid).is_err());
        drop(store);
        assert_eq!(Store::open(dir.clone()).unwrap().task(&task.id).unwrap().requested_model,Some("gpt-test".into()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
