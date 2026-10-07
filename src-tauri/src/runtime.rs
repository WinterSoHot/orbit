use crate::{
    executor::{Descriptor, Doctor, Executor},
    model::{Task, QueueAction},
    store::Store,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter};
use serde::Serialize;

#[derive(Clone, Default, Serialize, PartialEq)]
pub struct QueueState {pub paused:bool,pub reason:Option<String>}
#[derive(Default)]
struct QueueControl {state:QueueState,observed:HashMap<String,String>,reservations:HashMap<String,Reservation>,protected:std::collections::HashSet<String>,started:bool}
#[derive(Clone)]
struct Reservation {task_id:String,request_id:String,cancelled:bool}


#[derive(Clone)]
pub struct Runtime {
    pub store: Arc<Store>,
    executors: Arc<Vec<Arc<dyn Executor>>>,
    // Short admission lock only; each adapter retains its real process ownership.
    lifecycle: Arc<Mutex<bool>>,
    queue: Arc<Mutex<QueueControl>>,
}
impl Runtime {
    pub fn new(store: Store) -> Self {
        let workspace=store.workspace();
        let reason=workspace.error.clone().or_else(||workspace.tasks.iter().any(|t|t.queue.is_some()||t.team.as_ref().is_some_and(|w|matches!(w.phase.as_str(),"work"|"summary"|"review")||w.git_operation.as_ref().is_some_and(|o|o.state=="pending"))).then(||"重启后队列已暂停，请核对启动请求后恢复".into()));
        let queue=Arc::new(Mutex::new(QueueControl{state:QueueState{paused:reason.is_some(),reason},..Default::default()}));
        let store = Arc::new(store);
        Self {
            executors: Arc::new(vec![
                Arc::new(crate::runner::Runtime::from_shared(store.clone())),
                Arc::new(crate::qoder::QoderExecutor::new(store.clone())),
            ]),
            store,
            queue,
            lifecycle: Arc::new(Mutex::new(false)),
        }
    }
    pub fn list_executors(&self) -> Vec<Descriptor> {
        self.executors.iter().map(|e| e.descriptor()).collect()
    }
    fn executor(&self, provider: &str) -> Result<&Arc<dyn Executor>, String> {
        self.executors
            .iter()
            .find(|e| e.descriptor().id == provider)
            .ok_or_else(|| format!("执行器 {provider} 尚未接入；历史仍可查看和导出"))
    }
    pub fn doctor(&self, provider: &str) -> Result<Doctor, String> {
        Ok(self.executor(provider)?.doctor())
    }
    pub fn models(&self, provider: &str) -> Result<Vec<crate::executor::ExecutorModel>, String> {
        self.executor(provider)?.models()
    }
    fn begin(&self) -> Result<std::sync::MutexGuard<'_, bool>, String> {
        let guard = self.lifecycle.lock().unwrap();
        if *guard {
            Err("工作台正在关闭".into())
        } else {
            Ok(guard)
        }
    }
    fn ensure_idle(&self) -> Result<(), String> {
        for executor in self.executors.iter() {
            executor.ensure_idle()?;
        }
        Ok(())
    }
    pub fn create(
        &self,
        title: String,
        prompt: String,
        scene: String,
        provider: String,
        requested_model: Option<String>,
    ) -> Result<Task, String> {
        self.create_with_sources(title,prompt,scene,provider,requested_model,vec![])
    }
    pub fn create_with_sources(&self,title:String,prompt:String,scene:String,provider:String,requested_model:Option<String>,requests:Vec<crate::sources::SourceRequest>)->Result<Task,String>{
        let _guard = self.begin()?;
        let executor = self.executor(&provider)?;
        let sources=crate::sources::resolve(&self.store.library,requests)?;
        if requested_model.as_deref().is_some_and(|id| !crate::executor::valid_model_id(id)) {
            return Err("模型标识无效".into());
        }
        if title.trim().is_empty()
            || title.chars().count() > 100
            || prompt.trim().is_empty()
            || prompt.chars().count() > 12000
            || !matches!(scene.as_str(), "research" | "coding" | "writing")
        {
            return Err("请提供有效的任务名称、目标与场景".into());
        }
        let mut task = Task::new(title.trim().into(), prompt.trim().into(), scene);
        if !sources.is_empty(){task.source_inputs.push(crate::sources::SourceInput::new(task.id.clone(),"template",task.prompt.clone(),sources,None,None));}
        task.explicit_delivery=true;
        task.provider = provider;
        task.requested_model = requested_model;
        task.capabilities = Some(executor.descriptor().capabilities);
        task.status = "queued".into();
        task.started_at = None;
        task.run_id = None;
        task.revision = 0;
        self.store.save_task(task.clone())?;
        Ok(task)
    }
    pub fn queue_state(&self)->QueueState {self.queue.lock().unwrap().state.clone()}
    pub fn set_queue_paused(&self,paused:bool)->Result<QueueState,String> {
        let _guard=self.begin()?;
        if !paused {
            let workspace=self.store.workspace();
            if workspace.error.is_some(){return Err("本地记录无法读取，请先修复保存问题".into());}
            if workspace.tasks.iter().any(|t|t.queue.as_ref().is_some_and(|q|q.state=="claimed")){return Err("存在未确认的启动请求，请核对并撤销请求后恢复".into());}
        }
        let mut q=self.queue.lock().unwrap();
        for active in self.store.workspace().tasks.iter().filter(|t|matches!(t.status.as_str(),"running"|"approval"|"cancelling")){if let Some(run)=&active.run_id{q.observed.insert(run.clone(),active.id.clone());}}
        q.state=QueueState{paused,reason:paused.then(||"队列已手动暂停；当前运行不受影响".into())};Ok(q.state.clone())
    }
    fn pause_error(&self,error:String){self.queue.lock().unwrap().state=QueueState{paused:true,reason:Some(error)};}
    fn enqueue_start(&self,id:&str,revision:u64,on_queued:impl FnOnce(&Task))->Result<Task,String> {
        let _guard=self.begin()?;self.ensure_task_idle(id)?;let task=self.store.task(id).ok_or("任务不存在")?;self.executor(&task.provider)?;
        let saved=self.store.enqueue(id,revision,QueueAction::Start)?;self.refresh(&saved);on_queued(&saved);Ok(saved)
    }
    pub fn start(&self,app:AppHandle,task_id:String,revision:u64)->Result<Task,String>{
        self.enqueue_start(&task_id,revision,|saved|{let _=app.emit("runtime-task",saved);})
    }
    pub fn continue_task(&self,app:AppHandle,task_id:String,revision:u64,run_id:Option<String>,turn_id:Option<String>,text:String,requests:Vec<crate::sources::SourceRequest>)->Result<Task,String>{
        let _guard=self.begin()?;let sources=crate::sources::resolve(&self.store.library,requests)?;let existing=self.store.task(&task_id).ok_or("任务不存在")?;let executor=self.executor(&existing.provider)?;if existing.team.is_some()||existing.parent_link.is_some(){return Err("团队任务请通过计划或修订入口推进".into())}
        if existing.session_ref.as_ref().is_some_and(|s|s.provider!=existing.provider||s.protocol!=executor.descriptor().protocol){return Err("会话协议与任务执行器不匹配，未发送补充".into());}
        let saved=self.store.enqueue_with_sources(&task_id,revision,QueueAction::Continue{text,run_id,turn_id},sources)?;self.refresh(&saved);let _=app.emit("runtime-task",&saved);Ok(saved)
    }
    pub fn cancel_queued(&self,id:String,revision:u64)->Result<Task,String>{
        let _guard=self.begin()?;let task=self.store.task(&id).ok_or("任务不存在")?;
        let saved=if let Some(request)=task.queue.as_ref().filter(|q|q.state=="claimed"){
            if self.queue.lock().unwrap().reservations.contains_key(&request.next_run_id){
                let saved=self.store.request_claim_cancel(&id,revision)?;
                self.queue.lock().unwrap().reservations.get_mut(&request.next_run_id).unwrap().cancelled=true;
                self.refresh(&saved);return Ok(saved)
            }
            self.ensure_task_idle(&id)?;
            let saved=self.store.cancel_claimed(&id,revision,&request.request_id,&request.next_run_id)?;
            let mut control=self.queue.lock().unwrap();
            control.observed.remove(&request.next_run_id);
            saved
        }else{self.store.cancel_queued(&id,revision)?};self.refresh(&saved);Ok(saved)
    }
    pub fn accept_task(&self,id:String,revision:u64,run:Option<String>,turn:Option<String>)->Result<Task,String>{
        let _guard=self.begin()?;self.ensure_task_idle(&id)?;
        let saved=self.store.accept_task(&id,revision,&run,&turn)?;self.refresh(&saved);Ok(saved)
    }
    // Only called under lifecycle admission; no Store guard survives this method.
    fn next_locked(&self)->Result<Option<(Task,Option<String>)>,String>{
        let mut control=self.queue.lock().unwrap();if control.state.paused{return Ok(None)};
        let workspace=self.store.advance_teams()?;if let Some(error)=workspace.error{return Err(error)};
        let ownership:Vec<_>=self.executors.iter().flat_map(|e|e.ownership()).collect();
        let mut slots=std::collections::HashSet::new();
        for owned in &ownership{slots.insert(owned.run_id.clone());}
        for run in control.reservations.keys(){slots.insert(run.clone());}
        for task in workspace.tasks.iter().filter(|t|matches!(t.status.as_str(),"running"|"approval"|"cancelling")){
            if let Some(run)=&task.run_id{slots.insert(run.clone());control.observed.insert(run.clone(),task.id.clone());}
        }
        let ended:Vec<_>=control.observed.iter().filter(|(run,_)|!slots.contains(*run)).map(|(run,id)|(run.clone(),id.clone())).collect();
        for (run,id) in ended{control.observed.remove(&run);if workspace.tasks.iter().any(|t|t.id==id&&t.run_id.as_ref()==Some(&run)&&matches!(t.status.as_str(),"failed"|"interrupted"|"unknown")){return Err("已有任务未成功结束，队列已暂停；核对后可恢复".into());}}
        if slots.len()>=3{return Ok(None)}
        let Some(pending)=workspace.tasks.iter().filter(|t|t.queue.as_ref().is_some_and(|q|q.state=="pending")).min_by_key(|t|t.queue.as_ref().unwrap().order) else{return Ok(None)};
        if control.protected.contains(&pending.id){return Ok(None)}
        if ownership.iter().any(|o|o.task_id==pending.id||o.session_id.as_ref().is_some_and(|id|pending.session_ref.as_ref().is_some_and(|s|&s.id==id)||pending.thread_id.as_ref()==Some(id))){return Err("任务或会话仍有写入者，请核对执行器".into())}
        for executor in self.executors.iter(){executor.ensure_task_idle(&pending.id)?;}
        let live:Vec<_>=control.reservations.values().map(|r|r.request_id.clone()).collect();
        // An unknown recovered claim remains blocking even if no runnable pending task exists.
        let next=self.store.claim_next_reserved(&live)?;
        if let Some((task,_))=&next{let q=task.queue.as_ref().unwrap();control.observed.insert(q.next_run_id.clone(),task.id.clone());control.reservations.insert(q.next_run_id.clone(),Reservation{task_id:task.id.clone(),request_id:q.request_id.clone(),cancelled:false});}
        Ok(next)
    }
    #[cfg(test)]
    fn next_for_dispatch(&self)->Result<Option<(Task,Option<String>)>,String>{
        let _guard=self.begin()?;let next=self.next_locked()?;
        // Claim-only fixture: no launch thread is created by this helper.
        if let Some((t,_))=&next{self.queue.lock().unwrap().reservations.remove(t.run_id.as_ref().unwrap());}Ok(next)
    }
    fn finish_launch(&self,app:AppHandle,task:Task,anchor:Option<String>){
        let q=task.queue.as_ref().unwrap().clone();
        let allowed={let guard=self.lifecycle.lock().unwrap();let c=self.queue.lock().unwrap();!*guard&&c.reservations.get(&q.next_run_id).is_some_and(|r|r.task_id==task.id&&r.request_id==q.request_id&&!r.cancelled)};
        let result=if allowed{self.prepare_writing_task(&task).and_then(|prepared|{let cancelled={let closing=self.lifecycle.lock().unwrap();*closing||self.queue.lock().unwrap().reservations.get(&q.next_run_id).is_none_or(|r|r.cancelled)};if cancelled{Err("启动已取消".into())}else{self.executor(&task.provider).and_then(|e|e.launch(app.clone(),prepared.actor_snapshot(),anchor))}})}else{Err("启动已取消".into())};
        let error=match result{Ok(ref launched) if matches!(launched.status.as_str(),"failed"|"interrupted"|"unknown")=>Some("执行器启动未成功，请核对运行记录".to_string()),Err(ref error)=>Some(error.clone()),_=>None};
        if let Some(error)=&error{self.pause_error(error.clone());}
        let saved=self.settle_launch(&task,&q,error,||self.executor(&task.provider).and_then(|e|{if e.ownership().iter().any(|o|o.run_id==q.next_run_id){e.abort_start(app.clone(),q.next_run_id.clone())}else{Ok(())}}));
        match saved{Ok(saved)=>{self.refresh(&saved);let _=app.emit("runtime-task",&saved);},Err(error)=>self.pause_error(format!("启动记录未确认，请求已保留：{error}"))}
    }
    fn settle_launch(&self,task:&Task,q:&crate::model::QueueRequest,error:Option<String>,cleanup:impl FnOnce()->Result<(),String>)->Result<Task,String>{
        let mut guard=self.lifecycle.lock().unwrap();
        let cancelled=*guard||self.queue.lock().unwrap().reservations.get(&q.next_run_id).is_none_or(|r|r.task_id!=task.id||r.request_id!=q.request_id||r.cancelled)||self.store.task(&task.id).is_none_or(|t|t.queue.as_ref().is_none_or(|queued|queued.cancel_requested||queued.request_id!=q.request_id||queued.next_run_id!=q.next_run_id));
        if cancelled{drop(guard);let cleaned=cleanup();guard=self.lifecycle.lock().unwrap();if let Err(error)=cleaned{self.queue.lock().unwrap().reservations.remove(&q.next_run_id);return Err(error)}}
        let saved=if cancelled{let current=self.store.task(&task.id).ok_or("任务不存在")?;self.store.cancel_claimed(&task.id,current.revision,&q.request_id,&q.next_run_id)}else{self.store.finish_claim(&task.id,&q.request_id,&q.next_run_id,error)};
        // Launch has returned: the adapter owns any remaining process. Durable claimed recovery is separate.
        self.queue.lock().unwrap().reservations.remove(&q.next_run_id);drop(guard);saved
    }
    pub fn start_dispatcher(&self,app:AppHandle){
        {let mut q=self.queue.lock().unwrap();if q.started{return}q.started=true;}
        let owned=self.clone();std::thread::spawn(move||{let mut last_state=None;let mut revisions=HashMap::new();loop{
            let next={let Ok(_guard)=owned.begin()else{break};match owned.next_locked(){Ok(value)=>value,Err(error)=>{owned.pause_error(error);None}}};
            if let Some((task,anchor))=next{let _=app.emit("runtime-task",&task);let launcher=owned.clone();let launch_app=app.clone();std::thread::spawn(move||launcher.finish_launch(launch_app,task,anchor));}
            for task in owned.store.workspace().tasks {if revisions.get(&task.id)!=Some(&task.revision){revisions.insert(task.id.clone(),task.revision);let _=app.emit("runtime-task",task);}}
            let state=owned.queue_state();if last_state.as_ref()!=Some(&state){let _=app.emit("runtime-queue",&state);last_state=Some(state);}
            std::thread::sleep(std::time::Duration::from_millis(300));
        }});
    }
    fn run_executor(&self, run_id: &str) -> Result<&Arc<dyn Executor>, String> {
        let workspace = self.store.workspace();
        let task = workspace
            .tasks
            .iter()
            .find(|t| t.run_id.as_deref() == Some(run_id) && !t.archived)
            .ok_or("运行已失效")?;
        self.executor(&task.provider)
    }
    pub fn steer(&self, app: &AppHandle, id: String, text: String) -> Result<(), String> {
        self.run_executor(&id)?.steer(app, id, text)
    }
    pub fn steer_sources(&self,app:&AppHandle,id:String,text:String,requests:Vec<crate::sources::SourceRequest>)->Result<(),String>{
        let _guard=self.begin()?;let sources=crate::sources::resolve(&self.store.library,requests)?;
        self.run_executor(&id)?.steer_sources(app,id,text,sources)
    }
    pub fn interrupt(&self, app: AppHandle, id: String) -> Result<(), String> {
        self.run_executor(&id)?.interrupt(app, id)
    }
    pub fn reply(
        &self,
        app: &AppHandle,
        id: String,
        approval_id: String,
        answers: HashMap<String, String>,
    ) -> Result<(), String> {
        self.run_executor(&id)?.reply(app, id, approval_id, answers)
    }
    pub fn sync_agents(&self, task_id: String) -> Result<Task, String> {
        let _guard = self.begin()?;
        self.ensure_task_idle(&task_id)?;
        let task = self.store.task(&task_id).ok_or("任务不存在")?;
        self.executor(&task.provider)?.sync_agents(task_id)
    }
    fn ensure_task_idle(&self, task_id: &str) -> Result<(), String> {
        let control=self.queue.lock().unwrap();
        if control.protected.contains(task_id)||control.reservations.values().any(|r|r.task_id==task_id){return Err("任务仍在启动或保存变更中".into())}
        drop(control);
        for executor in self.executors.iter() {
            executor.ensure_task_idle(task_id)?;
        }
        Ok(())
    }
    pub fn save_message(&self,id:String,revision:u64,run:String,thread:String,item:String)->Result<Task,String>{
        let _guard=self.begin()?;self.ensure_task_idle(&id)?;
        let task=self.store.save_message(&id,revision,&run,&thread,&item)?;self.refresh(&task);Ok(task)
    }
    pub fn edit_artifact(
        &self,
        id: String,
        expected: String,
        content: String,
    ) -> Result<Task, String> {
        let _guard = self.begin()?;
        let workspace = self.store.workspace();
        let owner = workspace
            .tasks
            .iter()
            .find(|task| task.artifacts.iter().any(|a| a.id == id))
            .ok_or("交付物不存在")?;
        self.ensure_task_idle(&owner.id)?;
        let task = self.store.edit_artifact(&id, &expected, &content)?;
        self.refresh(&task);
        Ok(task)
    }
    pub fn archive_task(&self, task_id: String) -> Result<Task, String> {
        let _guard = self.begin()?;
        for id in self.related(&task_id){self.ensure_task_idle(&id)?;}
        let task = self.store.archive_task(&task_id)?;
        self.refresh(&task);
        Ok(task)
    }
    fn refresh(&self, task: &Task) {
        for executor in self.executors.iter() {
            executor.refresh(task);
        }
    }
    pub fn delete_task(&self, task_id: String) -> Result<(), String> {
        let _guard = self.begin()?;
        let ids=self.related(&task_id);for id in &ids{self.ensure_task_idle(id)?;}
        self.store.delete_task(&task_id)?;
        for executor in self.executors.iter() {
            for id in &ids{executor.forget(id);}
        }
        Ok(())
    }
    pub fn shutdown(&self) {
        let mut guard = self.lifecycle.lock().unwrap();
        *guard = true;
        for reserved in self.queue.lock().unwrap().reservations.values_mut(){reserved.cancelled=true;}
        for executor in self.executors.iter() {
            executor.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_executor_is_required_and_provider_is_immutable() {
        let dir = std::env::temp_dir().join(format!("orbit-executors-{}", uuid::Uuid::new_v4()));
        let runtime = Runtime::new(Store::open(dir.clone()).unwrap());
        assert_eq!(
            runtime
                .list_executors()
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            vec!["codex", "qoder"]
        );
        assert!(runtime
            .create(
                "x".into(),
                "y".into(),
                "research".into(),
                "unregistered".into(), None)
            .is_err());
        let mut task = runtime
            .create("x".into(), "y".into(), "research".into(), "qoder".into(), None)
            .unwrap();
        assert_eq!(task.provider, "qoder");
        task.provider = "codex".into();
        assert!(runtime.store.save_task(task).is_err());
        assert!(runtime.doctor("unregistered").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    pub(super) struct ActiveExecutor {
        pub(super) active_id: String,
    }
    impl Executor for ActiveExecutor {
        fn descriptor(&self) -> Descriptor {
            Descriptor {
                id: "third".into(),
                name: "Third".into(),
                protocol: "test".into(),
                description: String::new(),
                permission_note: String::new(),
                capabilities: Default::default(),
            }
        }
        fn doctor(&self) -> Doctor {
            Doctor {
                provider: "third".into(),
                available: false,
                initialized: false,
                path: String::new(),
                version: String::new(),
                message: String::new(),
                capabilities: Default::default(),
            }
        }
        fn ensure_idle(&self) -> Result<(), String> {
            Err("active task".into())
        }
        fn ensure_task_idle(&self, id: &str) -> Result<(), String> {
            if id == self.active_id {
                Err("active task".into())
            } else {
                Ok(())
            }
        }
        fn launch(
            &self,
            _app: AppHandle,
            _task: Task,
            _anchor: Option<String>,
        ) -> Result<Task, String> {
            Err("not used".into())
        }
        fn interrupt(&self, _app: AppHandle, _id: String) -> Result<(), String> {
            Err("not used".into())
        }
        fn refresh(&self, _task: &Task) {}
        fn forget(&self, _task_id: &str) {}
        fn shutdown(&self) {}
    }
    #[test]
    fn completed_task_can_be_archived_while_another_executor_task_is_running() {
        let dir =
            std::env::temp_dir().join(format!("orbit-dispatch-scope-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(dir.clone()).unwrap());
        let mut active = Task::new("active".into(), "run".into(), "research".into());
        active.provider = "third".into();
        store.save_task(active.clone()).unwrap();
        let runtime = Runtime {
            store,
            executors: Arc::new(vec![Arc::new(ActiveExecutor {
                active_id: active.id.clone(),
            })]),
            lifecycle: Arc::new(Mutex::new(false)),
            queue: Arc::new(Mutex::new(QueueControl::default())),
        };
        assert!(runtime.ensure_idle().is_err());
        let mut completed = runtime
            .create(
                "done".into(),
                "finished".into(),
                "research".into(),
                "third".into(), None)
            .unwrap();
        completed.begin_run(false);completed.turn_id=Some("turn".into());completed.thread_id=Some("thread".into());
        runtime.store.save_task(completed.clone()).unwrap();
        completed.status="completed".into();completed.delivery_candidate.capture("reply",&crate::delivery::fixture("delivered"));completed.event("complete","system","fixture");
        let completed=runtime.store.save_existing_task(completed).unwrap().unwrap();let artifact_id=completed.artifacts[0].id.clone();
        runtime.store.accept_task(&completed.id,completed.revision,&completed.run_id,&completed.turn_id).unwrap();
        assert!(runtime.archive_task(active.id.clone()).is_err());
        assert!(runtime.delete_task(active.id.clone()).is_err());
        assert!(runtime.archive_task(completed.id.clone()).unwrap().archived);
        let path = runtime.store.export_artifact(&artifact_id).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "delivered");
        runtime.delete_task(completed.id.clone()).unwrap();
        assert!(runtime.store.task(&completed.id).is_none());
        assert!(runtime.store.task(&active.id).is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    #[test]
    fn distinct_tasks_claim_three_slots_without_waiting_for_other_roots() {
        let dir=std::env::temp_dir().join(format!("orbit-three-slots-{}",uuid::Uuid::new_v4()));
        let runtime=Runtime::new(Store::open(dir.clone()).unwrap());
        for name in ["a","b","c","d"] {
            let t=runtime.create(name.into(),"goal".into(),"research".into(),"codex".into(),None).unwrap();
            runtime.enqueue_start(&t.id,t.revision,|_|{}).unwrap();
        }
        let mut ids=vec![];
        for _ in 0..3 {
            let (t,_)=runtime.next_for_dispatch().unwrap().expect("independent task must claim a free slot");
            assert!(!ids.contains(&t.id));ids.push(t.id.clone());
            let q=t.queue.unwrap();runtime.store.finish_claim(&t.id,&q.request_id,&q.next_run_id,None).unwrap();
        }
        assert!(runtime.next_for_dispatch().unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn restart_publishes_reserved_identity_before_dispatch() {
        let dir=std::env::temp_dir().join(format!("orbit-order-{}",uuid::Uuid::new_v4()));
        let runtime=Runtime::new(Store::open(dir.clone()).unwrap());
        let mut old=Task::new("old".into(),"prompt".into(),"research".into());old.status="completed".into();runtime.store.save_task(old.clone()).unwrap();
        let mut published=None;
        runtime.enqueue_start(&old.id,old.revision,|queued|{
            assert!(runtime.lifecycle.try_lock().is_err());
            assert_eq!(queued.run_id,old.run_id);
            published=Some(queued.queue.as_ref().unwrap().next_run_id.clone());
        }).unwrap();
        let (claimed,_)=runtime.next_for_dispatch().unwrap().unwrap();assert_eq!(claimed.run_id,published);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn pause_resume_keeps_active_failure_checkpoint() {
        let directory=std::env::temp_dir().join(format!("orbit-pause-race-{}",uuid::Uuid::new_v4()));
        let runtime=Runtime::new(Store::open(directory.clone()).unwrap());
        let a=runtime.create("a".into(),"goal".into(),"research".into(),"codex".into(), None).unwrap();
        let b=runtime.create("b".into(),"goal".into(),"research".into(),"qoder".into(), None).unwrap();
        runtime.enqueue_start(&a.id,a.revision,|_|{}).unwrap();runtime.enqueue_start(&b.id,b.revision,|_|{}).unwrap();
        let (mut actor,_)=runtime.next_for_dispatch().unwrap().unwrap();let q=actor.queue.clone().unwrap();runtime.store.finish_claim(&a.id,&q.request_id,&q.next_run_id,None).unwrap();
        runtime.set_queue_paused(true).unwrap();runtime.set_queue_paused(false).unwrap();
        actor.status="failed".into();actor.event("quick failure","error","executor");runtime.store.save_existing_task(actor).unwrap().unwrap();
        assert!(runtime.next_for_dispatch().is_err());assert_eq!(runtime.store.task(&b.id).unwrap().queue.unwrap().state,"pending");
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn idle_claim_recovers_after_bookkeeping_and_final_write_failure() {
        let directory=std::env::temp_dir().join(format!("orbit-claim-disk-{}",uuid::Uuid::new_v4()));
        let runtime=Runtime::new(Store::open(directory.clone()).unwrap());let task=runtime.create("a".into(),"goal".into(),"research".into(),"codex".into(), None).unwrap();
        runtime.enqueue_start(&task.id,task.revision,|_|{}).unwrap();let (mut actor,_)=runtime.next_for_dispatch().unwrap().unwrap();let q=actor.queue.clone().unwrap();
        std::fs::create_dir(directory.join("workspace.tmp")).unwrap();
        assert!(runtime.store.finish_claim(&task.id,&q.request_id,&q.next_run_id,None).is_err());
        actor.status="completed".into();actor.event("done","system","executor");assert!(runtime.store.save_existing_task(actor).is_err());
        runtime.pause_error("保存失败".into());std::fs::remove_dir(directory.join("workspace.tmp")).unwrap();
        let stale=runtime.store.task(&task.id).unwrap();assert_eq!(stale.status,"running");
        assert!(runtime.store.cancel_claimed(&task.id,stale.revision,"wrong",&q.next_run_id).is_err());
        assert!(runtime.store.cancel_claimed(&task.id,stale.revision,&q.request_id,"wrong").is_err());
        assert!(runtime.store.cancel_claimed(&task.id,stale.revision+1,&q.request_id,&q.next_run_id).is_err());
        let cancelled=runtime.cancel_queued(task.id.clone(),stale.revision).expect("idle ownership must permit explicit claim recovery");assert!(cancelled.queue.is_none());assert_eq!(cancelled.status,"unknown");
        runtime.set_queue_paused(false).unwrap();assert!(runtime.next_for_dispatch().unwrap().is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn unknown_claim_cannot_be_cancelled_until_executor_is_idle() {
        let directory=std::env::temp_dir().join(format!("orbit-cancel-claim-{}",uuid::Uuid::new_v4()));
        let idle=Runtime::new(Store::open(directory.clone()).unwrap());
        let task=idle.create("x".into(),"goal".into(),"research".into(),"codex".into(), None).unwrap();
        idle.enqueue_start(&task.id,task.revision,|_|{}).unwrap();
        let (claimed,_)=idle.next_for_dispatch().unwrap().unwrap();let request=claimed.queue.unwrap();
        let unknown=idle.store.finish_claim(&task.id,&request.request_id,&request.next_run_id,Some("launch result unknown".into())).unwrap();
        assert_eq!(unknown.status,"unknown");assert!(unknown.finished_at.is_none());
        let active=Runtime{store:idle.store.clone(),executors:Arc::new(vec![Arc::new(super::tests::ActiveExecutor{active_id:task.id.clone()})]),lifecycle:Arc::new(Mutex::new(false)),queue:idle.queue.clone()};
        assert!(active.cancel_queued(task.id.clone(),unknown.revision).is_err());
        assert!(idle.store.task(&task.id).unwrap().queue.is_some());
        assert!(idle.cancel_queued(task.id.clone(),unknown.revision).unwrap().queue.is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn restart_pauses_pending_and_resume_obeys_active_executor() {
        let directory=std::env::temp_dir().join(format!("orbit-dispatch-test-{}",uuid::Uuid::new_v4()));
        let runtime=Runtime::new(Store::open(directory.clone()).unwrap());
        let task=runtime.create("first".into(),"goal".into(),"research".into(),"qoder".into(), None).unwrap();
        runtime.enqueue_start(&task.id,task.revision,|_|{}).unwrap();assert!(!runtime.queue_state().paused);
        let restarted=Runtime::new(Store::open(directory.clone()).unwrap());assert!(restarted.queue_state().paused);assert!(restarted.next_for_dispatch().unwrap().is_none());
        restarted.set_queue_paused(false).unwrap();let (claimed,_)=restarted.next_for_dispatch().unwrap().unwrap();assert_eq!(claimed.provider,"qoder");
        assert!(restarted.set_queue_paused(false).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}

// ponytail: Git metadata operations are globally serialized; per-repository locks if throughput matters.
static GIT_OPERATIONS:Mutex<()>=Mutex::new(());
struct TaskProtection{queue:Arc<Mutex<QueueControl>>,ids:Vec<String>}
impl Drop for TaskProtection{fn drop(&mut self){let mut q=self.queue.lock().unwrap();for id in &self.ids{q.protected.remove(id);}}}
impl Runtime{
 fn protect_tasks(&self,ids:Vec<String>)->Result<TaskProtection,String>{for id in &ids{self.ensure_task_idle(id)?;}let mut q=self.queue.lock().unwrap();for id in &ids{q.protected.insert(id.clone());}Ok(TaskProtection{queue:self.queue.clone(),ids})}
 fn emit_workspace(&self,app:&AppHandle){for task in self.store.workspace().tasks{let _=app.emit("runtime-task",task);}}
 pub fn delete_agent(&self,id:String,revision:u64)->Result<(),String>{let _g=self.begin()?;self.store.delete_agent(&id,revision)}
 pub fn open_code_workspace(&self,id:String)->Result<(),String>{let t=self.store.task(&id).ok_or("任务不存在")?;let w=t.code_workspace.as_ref().ok_or("工作区未准备")?;let p=crate::coding::workspace_path(&self.git_root()?,&id,w)?;
 #[cfg(target_os="macos")] {if std::process::Command::new("/usr/bin/open").arg(p).status().map_err(|_|"打开失败")?.success(){Ok(())}else{Err("系统无法打开工作区".into())}}
 #[cfg(not(target_os="macos"))] {let _=p;Err("此平台暂不支持系统打开".into())}}
 pub fn agents(&self)->Vec<crate::team::AgentProfile>{self.store.workspace().agents}
 pub fn save_agent(&self,a:crate::team::AgentProfile)->Result<crate::team::AgentProfile,String>{let _g=self.begin()?;self.executor(&a.provider)?;self.store.save_agent(a)}
 pub fn create_team(&self,app:&AppHandle,mut draft:crate::team::PlanDraft)->Result<Task,String>{
  if let Some(p)=&draft.project{draft.project=Some(crate::coding::preflight(std::path::Path::new(&p.repo),&p.target)?);}
  let _g=self.begin()?;let profiles=self.store.workspace().agents;for id in [&draft.coordinator_id,&draft.reviewer_id].into_iter().chain(draft.workers.iter().map(|w|&w.agent_id)){let profile=profiles.iter().find(|a|&a.id==id).ok_or("Agent 已删除")?;self.executor(&profile.provider)?;}
  let task=self.store.create_team(draft)?;self.emit_workspace(app);Ok(task)
 }
 pub fn confirm_team(&self,app:&AppHandle,id:String,revision:u64,version:String)->Result<Task,String>{let _g=self.begin()?;self.ensure_task_idle(&id)?;let saved=self.store.confirm_team(&id,revision,&version)?;self.emit_workspace(app);Ok(saved)}
 pub fn revise_team(&self,app:&AppHandle,id:String,revision:u64,mut draft:crate::team::PlanDraft)->Result<Task,String>{if let Some(p)=&draft.project{draft.project=Some(crate::coding::preflight(std::path::Path::new(&p.repo),&p.target)?);}for a in self.agents().iter().filter(|a|a.id==draft.coordinator_id||a.id==draft.reviewer_id||draft.workers.iter().any(|w|w.agent_id==a.id)){self.executor(&a.provider)?;}let _g=self.begin()?;let ids=self.related(&id);let _p=self.protect_tasks(ids)?;let t=self.store.revise_team(&id,revision,draft)?;self.emit_workspace(app);Ok(t)}
 pub fn revise_summary(&self,app:&AppHandle,id:String,revision:u64,text:String)->Result<Task,String>{let _g=self.begin()?;let _p=self.protect_tasks(self.related(&id))?;let t=self.store.revise_summary(&id,revision,&text)?;self.emit_workspace(app);Ok(t)}
 fn related(&self,id:&str)->Vec<String>{self.store.workspace().tasks.iter().filter(|t|t.id==id||t.parent_link.as_ref().is_some_and(|p|p.parent_id==id)).map(|t|t.id.clone()).collect()}
 pub fn cancel_team(&self,app:&AppHandle,id:String,revision:u64)->Result<Task,String>{
  let saved={let _g=self.begin()?;let related=self.related(&id);if self.queue.lock().unwrap().protected.iter().any(|r|related.contains(r)){return Err("正在保存或合并变更，请等本次操作结束".into())}let saved=self.store.cancel_team(&id,revision)?;let mut q=self.queue.lock().unwrap();for r in q.reservations.values_mut(){if related.contains(&r.task_id){r.cancelled=true;}}saved};
  let related=self.related(&id);for e in self.executors.iter(){for o in e.ownership().iter().filter(|o|related.contains(&o.task_id)){let _=e.interrupt(app.clone(),o.run_id.clone());}}self.emit_workspace(app);Ok(saved)
 }
 fn git_root(&self)->Result<std::path::PathBuf,String>{let root=self.store.directory.join("coding");if std::fs::symlink_metadata(&root).is_ok_and(|m|m.file_type().is_symlink()){return Err("代码管理目录不能为符号链接".into())}std::fs::create_dir_all(&root).map_err(|_|"无法创建代码管理目录")?;root.canonicalize().map_err(|_|"代码管理目录不可用".into())}
 fn prepare_writing_task(&self,t:&Task)->Result<Task,String>{
  if t.parent_link.as_ref().is_none_or(|p|!p.coding){return Ok(t.clone())}
  if t.provider!="codex"{return Err("此执行器未开放受限写任务".into())}
  let _git=GIT_OPERATIONS.lock().unwrap();crate::team_store::coding_authorized(&self.store.workspace(),t)?;let parent=t.parent_link.as_ref().unwrap().parent_id.clone();let project=self.store.task(&parent).and_then(|p|p.team).filter(|w|!w.cancelled&&w.plan.version==t.parent_link.as_ref().unwrap().plan_version).and_then(|w|w.plan.project).ok_or("计划写授权已失效")?;let root=self.git_root()?;
  let mut op=crate::team::GitOperation{id:uuid::Uuid::new_v4().to_string(),kind:"prepare".into(),old:project.base.clone(),new:project.base.clone(),target:root.join("worktrees").join(&t.id).to_string_lossy().into(),state:"pending".into()};self.store.git_operation(&parent,op.clone())?;
  let workspace=crate::coding::prepare(&root,&t.id,&project)?;let task=self.store.prepared_code(&t.id,t.run_id.as_deref().ok_or("缺少启动身份")?,workspace)?;op.state="completed".into();self.store.git_operation(&parent,op)?;Ok(task)
 }
 pub fn snapshot_code(&self,app:&AppHandle,id:String,revision:u64)->Result<Task,String>{
  let (t,_protection)={let _g=self.begin()?;let t=self.store.task(&id).filter(|t|t.revision==revision&&t.status=="completed"&&t.queue.is_none()).ok_or("请等任务结束后再保存快照")?;crate::team_store::coding_authorized(&self.store.workspace(),&t)?;let parent=t.parent_link.as_ref().ok_or("任务没有父计划")?.parent_id.clone();let _p=self.protect_tasks(self.related(&parent).into_iter().filter(|r|r==&id||r==&parent||self.store.task(&parent).and_then(|t|t.team).is_some_and(|w|w.phase!="work")).collect())?;(t,_p)};
  let _git=GIT_OPERATIONS.lock().unwrap();let w=t.code_workspace.as_ref().ok_or("工作区尚未准备")?;let e=crate::coding::snapshot(&self.git_root()?,&t.id,w)?;let saved=self.store.code_snapshot(&id,revision,e)?;self.emit_workspace(app);Ok(saved)
 }
 pub fn integrate_code(&self,app:&AppHandle,id:String,revision:u64)->Result<Task,String>{
  let (t,_p)={let _g=self.begin()?;let t=self.store.task(&id).filter(|t|t.revision==revision&&!t.archived).ok_or("任务已更新")?;let p=self.protect_tasks(self.related(&id))?;(t,p)};let _git=GIT_OPERATIONS.lock().unwrap();let w=t.team.as_ref().filter(|w|w.phase=="work"&&!w.cancelled&&w.confirmed.as_ref()==Some(&w.plan.version)&&w.children.len()==w.plan.workers.len()).ok_or("当前不能集成")?;let project=w.plan.project.as_ref().ok_or("未选择 Git 项目")?;let mut snapshots=vec![];for child in &w.children{let task=self.store.task(child).ok_or("子任务不存在")?;if task.parent_link.as_ref().is_some_and(|p|p.coding){crate::team_store::coding_authorized(&self.store.workspace(),&task)?;let code=task.code_workspace.as_ref().and_then(|c|c.snapshot.as_ref()).ok_or("请先保存每个代码分工的真实快照")?;snapshots.push(code.clone());}}
  let mut op=crate::team::GitOperation{id:uuid::Uuid::new_v4().to_string(),kind:"integrate".into(),old:project.base.clone(),new:String::new(),target:project.target.clone(),state:"pending".into()};self.store.git_operation(&id,op.clone())?;let e=crate::coding::integrate(&self.git_root()?,project,&snapshots)?;op.new=e.commit.clone();let saved=self.store.code_integration(&id,e)?;op.state="completed".into();self.store.git_operation(&id,op)?;self.emit_workspace(app);Ok(saved)
 }
 pub fn merge_code(&self,app:&AppHandle,id:String,revision:u64)->Result<Task,String>{
  let (t,_p)={let _g=self.begin()?;let t=self.store.task(&id).filter(|t|t.revision==revision&&t.accepted()&&!t.archived).ok_or("请先验收当前已评审成果")?;crate::team_store::can_accept(&self.store.workspace(),&t)?;let p=self.protect_tasks(self.related(&id))?;(t,p)};let _git=GIT_OPERATIONS.lock().unwrap();let w=t.team.as_ref().unwrap();let p=w.plan.project.as_ref().ok_or("非代码任务")?;let e=w.integration.as_ref().filter(|e|e.complete).ok_or("没有完整评审的集成提交")?;
  if w.review_input.as_ref().and_then(|i|i.code.as_ref())!=Some(e){return Err("集成代码已变化，需要新评审".into())}
  let mut op=w.git_operation.clone().filter(|o|o.kind=="merge"&&o.old==p.base&&o.new==e.commit&&o.target==p.target).unwrap_or(crate::team::GitOperation{id:uuid::Uuid::new_v4().to_string(),kind:"merge".into(),old:p.base.clone(),new:e.commit.clone(),target:p.target.clone(),state:"pending".into()});
  // A completed or interrupted identical CAS is recovered by reading the actual ref, never force-writing it.
  if crate::coding::ref_oid(p)?!=e.commit{op.state="pending".into();self.store.git_operation(&id,op.clone())?;crate::coding::merge_ref(p,&e.commit)?;}
  op.state="completed".into();let saved=self.store.git_operation(&id,op)?;self.emit_workspace(app);Ok(saved)
 }
}

#[cfg(test)]
mod handoff_regressions{
 use super::*;
 #[test]fn durable_cancel_cannot_be_erased_by_successful_claim(){let dir=std::env::temp_dir().join(format!("orbit-cancel-final-{}",uuid::Uuid::new_v4()));let r=Runtime::new(Store::open(dir.clone()).unwrap());let t=r.create("x".into(),"goal".into(),"research".into(),"codex".into(),None).unwrap();r.enqueue_start(&t.id,t.revision,|_|{}).unwrap();let(t,_)=r.next_for_dispatch().unwrap().unwrap();let q=t.queue.clone().unwrap();r.store.request_claim_cancel(&t.id,t.revision).unwrap();assert!(r.store.finish_claim(&t.id,&q.request_id,&q.next_run_id,None).is_err());assert!(r.store.task(&t.id).unwrap().queue.is_some());std::fs::remove_dir_all(dir).unwrap();}
}
#[cfg(test)]
mod final_handoff_barrier{
 use super::*;use std::sync::{Barrier,atomic::{AtomicBool,Ordering}};
 #[test]fn cancel_between_launch_and_final_handoff_is_not_lost(){
  let dir=std::env::temp_dir().join(format!("orbit-handoff-window-{}",uuid::Uuid::new_v4()));let r=Runtime::new(Store::open(dir.clone()).unwrap());let t=r.create("x".into(),"goal".into(),"research".into(),"codex".into(),None).unwrap();r.enqueue_start(&t.id,t.revision,|_|{}).unwrap();let(t,_)= {let _g=r.begin().unwrap();r.next_locked().unwrap().unwrap()};let q=t.queue.clone().unwrap();let arrived=Arc::new(Barrier::new(2));let resume=Arc::new(Barrier::new(2));let cleaned=Arc::new(AtomicBool::new(false));let worker={let r=r.clone();let t=t.clone();let q=q.clone();let arrived=arrived.clone();let resume=resume.clone();let cleaned=cleaned.clone();std::thread::spawn(move||{arrived.wait();resume.wait();r.settle_launch(&t,&q,None,||{cleaned.store(true,Ordering::SeqCst);Ok(())})})};
  arrived.wait();let canceled=r.cancel_queued(t.id.clone(),t.revision).unwrap();assert!(canceled.queue.unwrap().cancel_requested);assert!(r.queue.lock().unwrap().reservations.contains_key(&q.next_run_id));resume.wait();let saved=worker.join().unwrap().unwrap();assert!(cleaned.load(Ordering::SeqCst));assert!(saved.queue.is_none());assert!(r.queue.lock().unwrap().reservations.is_empty());std::fs::remove_dir_all(dir).unwrap();
 }
}
#[cfg(test)]
mod settled_claim_recovery{
 use super::*;
 #[test]fn finished_launcher_releases_reservation_even_when_store_write_fails(){
  for cancel in [false,true]{let dir=std::env::temp_dir().join(format!("orbit-final-io-{}",uuid::Uuid::new_v4()));let r=Runtime::new(Store::open(dir.clone()).unwrap());let t=r.create("x".into(),"goal".into(),"research".into(),"codex".into(),None).unwrap();r.enqueue_start(&t.id,t.revision,|_|{}).unwrap();let(t,_)= {let _g=r.begin().unwrap();r.next_locked().unwrap().unwrap()};let q=t.queue.clone().unwrap();if cancel{r.cancel_queued(t.id.clone(),t.revision).unwrap();}std::fs::create_dir(dir.join("workspace.tmp")).unwrap();assert!(r.settle_launch(&t,&q,None,||Ok(())).is_err());assert!(r.queue.lock().unwrap().reservations.is_empty());assert!(r.store.task(&t.id).unwrap().queue.is_some());std::fs::remove_dir(dir.join("workspace.tmp")).unwrap();let current=r.store.task(&t.id).unwrap();assert!(r.cancel_queued(t.id.clone(),current.revision).unwrap().queue.is_none());assert!(!r.set_queue_paused(false).unwrap().paused);std::fs::remove_dir_all(dir).unwrap();}
 }
}
