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
struct QueueControl {state:QueueState,observed:Option<(String,String)>,started:bool}


#[derive(Clone)]
pub struct Runtime {
    pub store: Arc<Store>,
    executors: Arc<Vec<Arc<dyn Executor>>>,
    // ponytail: one root task globally; per-workspace admission if concurrency is introduced.
    lifecycle: Arc<Mutex<bool>>,
    queue: Arc<Mutex<QueueControl>>,
}
impl Runtime {
    pub fn new(store: Store) -> Self {
        let workspace=store.workspace();
        let reason=workspace.error.clone().or_else(||workspace.tasks.iter().any(|t|t.queue.is_some()).then(||"重启后队列已暂停，请核对启动请求后恢复".into()));
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
        let _guard = self.begin()?;
        let executor = self.executor(&provider)?;
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
        if let Some(active)=self.store.workspace().tasks.iter().find(|t|matches!(t.status.as_str(),"running"|"approval"|"cancelling")){q.observed=active.run_id.as_ref().map(|run|(active.id.clone(),run.clone()));}
        q.state=QueueState{paused,reason:paused.then(||"队列已手动暂停；当前运行不受影响".into())};Ok(q.state.clone())
    }
    fn pause_error(&self,error:String){self.queue.lock().unwrap().state=QueueState{paused:true,reason:Some(error)};}
    fn enqueue_start(&self,id:&str,revision:u64,on_queued:impl FnOnce(&Task))->Result<Task,String> {
        let _guard=self.begin()?;let task=self.store.task(id).ok_or("任务不存在")?;self.executor(&task.provider)?;
        let saved=self.store.enqueue(id,revision,QueueAction::Start)?;self.refresh(&saved);on_queued(&saved);Ok(saved)
    }
    pub fn start(&self,app:AppHandle,task_id:String,revision:u64)->Result<Task,String>{
        self.enqueue_start(&task_id,revision,|saved|{let _=app.emit("runtime-task",saved);})
    }
    pub fn continue_task(&self,app:AppHandle,task_id:String,revision:u64,run_id:Option<String>,turn_id:Option<String>,text:String)->Result<Task,String>{
        let _guard=self.begin()?;let existing=self.store.task(&task_id).ok_or("任务不存在")?;let executor=self.executor(&existing.provider)?;
        if existing.session_ref.as_ref().is_some_and(|s|s.provider!=existing.provider||s.protocol!=executor.descriptor().protocol){return Err("会话协议与任务执行器不匹配，未发送补充".into());}
        let saved=self.store.enqueue(&task_id,revision,QueueAction::Continue{text,run_id,turn_id})?;self.refresh(&saved);let _=app.emit("runtime-task",&saved);Ok(saved)
    }
    pub fn cancel_queued(&self,id:String,revision:u64)->Result<Task,String>{
        let _guard=self.begin()?;let task=self.store.task(&id).ok_or("任务不存在")?;
        let saved=if let Some(request)=task.queue.as_ref().filter(|q|q.state=="claimed"){
            self.ensure_idle()?;
            let saved=self.store.cancel_claimed(&id,revision,&request.request_id,&request.next_run_id)?;
            let mut control=self.queue.lock().unwrap();
            if control.observed.as_ref()==Some(&(id.clone(),request.next_run_id.clone())){control.observed=None;}
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
        let workspace=self.store.workspace();
        if let Some(error)=workspace.error{return Err(error)};
        if let Some(active)=workspace.tasks.iter().find(|t|matches!(t.status.as_str(),"running"|"approval"|"cancelling")){
            control.observed=active.run_id.as_ref().map(|run|(active.id.clone(),run.clone()));return Ok(None)
        }
        if let Some((id,run))=control.observed.take(){
            if workspace.tasks.iter().any(|t|t.id==id&&t.run_id.as_ref()==Some(&run)&&matches!(t.status.as_str(),"failed"|"interrupted"|"unknown")){return Err("上一任务未成功结束，队列已暂停；核对后可恢复".into());}
        }
        if !workspace.tasks.iter().any(|t|t.queue.is_some()){return Ok(None)};
        self.ensure_idle()?;
        let next=self.store.claim_next()?;
        if let Some((task,_))=&next{control.observed=task.run_id.as_ref().map(|run|(task.id.clone(),run.clone()));}
        Ok(next)
    }
    #[cfg(test)]
    fn next_for_dispatch(&self)->Result<Option<(Task,Option<String>)>,String>{let _guard=self.begin()?;self.next_locked()}
    pub fn start_dispatcher(&self,app:AppHandle){
        {let mut q=self.queue.lock().unwrap();if q.started{return}q.started=true;}
        let owned=self.clone();
        std::thread::spawn(move||{
            let mut last_state=None;
            loop {
                {
                    let Ok(_guard)=owned.begin() else {break};
                    match owned.next_locked(){
                        Ok(Some((task,anchor)))=>{
                            let q=task.queue.as_ref().unwrap().clone();let _=app.emit("runtime-task",&task);
                            let result=owned.executor(&task.provider).and_then(|executor|executor.launch(app.clone(),task.clone(),anchor));
                            let error=match result {Ok(ref launched) if matches!(launched.status.as_str(),"failed"|"interrupted"|"unknown")=>Some("执行器启动未成功，请核对运行记录".to_string()),Err(ref error)=>Some(error.clone()),_=>None};
                            if let Some(error)=&error{owned.pause_error(error.clone());}
                            match owned.store.finish_claim(&task.id,&q.request_id,&q.next_run_id,error){
                                Ok(saved)=>{owned.refresh(&saved);let _=app.emit("runtime-task",&saved);},
                                Err(error)=>owned.pause_error(format!("启动记录未确认，请求已保留：{error}")),
                            }
                        },
                        Err(error)=>owned.pause_error(error),
                        _=>{},
                    }
                    let state=owned.queue_state();if last_state.as_ref()!=Some(&state){let _=app.emit("runtime-queue",&state);last_state=Some(state);}
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        });
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
        self.ensure_idle()?;
        let task = self.store.task(&task_id).ok_or("任务不存在")?;
        self.executor(&task.provider)?.sync_agents(task_id)
    }
    fn ensure_task_idle(&self, task_id: &str) -> Result<(), String> {
        for executor in self.executors.iter() {
            executor.ensure_task_idle(task_id)?;
        }
        Ok(())
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
        self.ensure_task_idle(&task_id)?;
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
        self.ensure_task_idle(&task_id)?;
        self.store.delete_task(&task_id)?;
        for executor in self.executors.iter() {
            executor.forget(&task_id);
        }
        Ok(())
    }
    pub fn shutdown(&self) {
        let mut guard = self.lifecycle.lock().unwrap();
        *guard = true;
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
        completed.status = "completed".into();
        completed.artifacts.push(crate::model::Artifact {
            id: "third-delivery".into(),
            name: "third.md".into(),
            kind: "markdown".into(),
            content: "delivered".into(),
            created_at: 0,
        });
        runtime.store.save_task(completed.clone()).unwrap();
        runtime.store.accept_task(&completed.id,completed.revision,&completed.run_id,&completed.turn_id).unwrap();
        assert!(runtime.archive_task(active.id.clone()).is_err());
        assert!(runtime.archive_task(completed.id.clone()).unwrap().archived);
        let path = runtime.store.export_artifact("third-delivery").unwrap();
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
