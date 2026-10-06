mod executor;
mod model;
mod conversation;
#[cfg(feature = "desktop")]
mod knowledge;
#[cfg(feature = "desktop")]
mod process;
mod protocol;
#[cfg(feature = "desktop")]
mod qoder;
#[cfg(feature = "desktop")]
mod runner;
#[cfg(feature = "desktop")]
mod runtime;
#[cfg(feature = "desktop")]
mod store;

#[cfg(feature = "desktop")]
mod desktop {
    use super::{
        executor::{Descriptor, Doctor},
        model::Task,
        knowledge::{Library,Document,NewDocument,Change,SmartGroup,Collection,fetch_web},
        runner,
        runtime::Runtime,
        store::{ExportSettings, Store, Workspace},
    };
    use tauri::{AppHandle, Manager, State};
    use tauri_plugin_dialog::DialogExt;

    #[tauri::command]
    fn load_workspace(runtime: State<Runtime>) -> Workspace {
        runtime.store.workspace()
    }
    #[tauri::command]
    fn archive_task(runtime: State<Runtime>, task_id: String) -> Result<Task, String> {
        runtime.archive_task(task_id)
    }
    #[tauri::command]
    fn delete_task(runtime: State<Runtime>, task_id: String) -> Result<(), String> {
        runtime.delete_task(task_id)
    }
    #[tauri::command]
    async fn sync_agents(runtime: State<'_, Runtime>, task_id: String) -> Result<Task, String> {
        let owned = runtime.inner().clone();
        tauri::async_runtime::spawn_blocking(move || owned.sync_agents(task_id))
            .await
            .map_err(|_| "协作记录读取进程失败".to_string())?
    }
    #[tauri::command]
    fn edit_artifact(
        runtime: State<Runtime>,
        artifact_id: String,
        expected_content: String,
        content: String,
    ) -> Result<Task, String> {
        runtime.edit_artifact(artifact_id, expected_content, content)
    }
    #[tauri::command]
    fn get_export_settings(runtime: State<Runtime>) -> ExportSettings {
        runtime.store.export_settings()
    }
    #[tauri::command]
    fn reset_export_directory(runtime: State<Runtime>) -> Result<ExportSettings, String> {
        runtime.store.set_export_directory(None)
    }
    #[tauri::command]
    async fn choose_export_directory(
        app: AppHandle,
        runtime: State<'_, Runtime>,
    ) -> Result<Option<ExportSettings>, String> {
        let store = runtime.store.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let current = store.export_settings();
            let mut dialog = app.dialog().file().set_title("选择导出文件夹");
            let current = std::path::PathBuf::from(current.directory);
            if current.is_dir() {
                dialog = dialog.set_directory(&current);
            }
            if let Some(window) = app.get_webview_window("main") {
                dialog = dialog.set_parent(&window);
            }
            match dialog.blocking_pick_folder() {
                Some(path) => store
                    .set_export_directory(Some(path.into_path().map_err(|_| "请选择本机文件夹")?))
                    .map(Some),
                None => Ok(None),
            }
        })
        .await
        .map_err(|_| "文件夹选择失败".to_string())?
    }
    #[tauri::command]
    fn export_artifact(runtime: State<Runtime>, artifact_id: String) -> Result<String, String> {
        runtime.store.export_artifact(&artifact_id)
    }
    #[tauri::command]
    async fn export_workspace(runtime: State<'_, Runtime>) -> Result<String, String> {
        let store = runtime.store.clone();
        tauri::async_runtime::spawn_blocking(move || store.export_workspace())
            .await
            .map_err(|_| "工作台数据导出失败".to_string())?
    }
    async fn library_work<T:Send+'static>(store:std::sync::Arc<Store>,work:impl FnOnce(&Store)->Result<T,String>+Send+'static)->Result<T,String>{
        tauri::async_runtime::spawn_blocking(move||work(&store)).await.map_err(|_|"知识库操作进程失败".to_string())?
    }
    #[tauri::command]
    fn load_library(runtime:State<Runtime>)->Library{runtime.store.library.view()}
    #[tauri::command]
    async fn create_document(runtime:State<'_,Runtime>,input:NewDocument)->Result<Document,String>{library_work(runtime.store.clone(),move|s|s.library.create(input)).await}
    #[tauri::command]
    async fn change_document(runtime:State<'_,Runtime>,change:Change)->Result<Document,String>{library_work(runtime.store.clone(),move|s|s.library.change(change)).await}
    #[tauri::command]
    async fn save_smart_group(runtime:State<'_,Runtime>,group:SmartGroup)->Result<SmartGroup,String>{library_work(runtime.store.clone(),move|s|s.library.save_group(group)).await}
    #[tauri::command]
    async fn save_collection(runtime:State<'_,Runtime>,collection:Collection)->Result<Collection,String>{library_work(runtime.store.clone(),move|s|s.library.save_collection(collection)).await}
    #[tauri::command]
    async fn delete_collection(runtime:State<'_,Runtime>,collection_id:String,revision:u64)->Result<Library,String>{library_work(runtime.store.clone(),move|s|s.library.delete_collection(&collection_id,revision)).await}
    #[tauri::command]
    async fn organize_document(runtime:State<'_,Runtime>,document_id:String,revision:u64,collection_ids:Vec<String>)->Result<Document,String>{library_work(runtime.store.clone(),move|s|s.library.organize(&document_id,revision,collection_ids)).await}
    #[tauri::command]
    async fn trash_document(runtime:State<'_,Runtime>,document_id:String,revision:u64)->Result<Document,String>{library_work(runtime.store.clone(),move|s|s.library.trash(&document_id,revision)).await}
    #[tauri::command]
    async fn restore_document(runtime:State<'_,Runtime>,document_id:String,revision:u64)->Result<Document,String>{library_work(runtime.store.clone(),move|s|s.library.restore_document(&document_id,revision)).await}
    #[tauri::command]
    async fn purge_document(runtime:State<'_,Runtime>,document_id:String,revision:u64)->Result<Option<String>,String>{library_work(runtime.store.clone(),move|s|s.library.purge(&document_id,revision)).await}
    #[tauri::command]
    fn get_library_directory(runtime:State<Runtime>)->String{runtime.store.library.directory().display().to_string()}
    #[tauri::command]
    async fn open_library_location(runtime:State<'_,Runtime>,document_id:Option<String>,reveal:bool)->Result<(),String>{
        library_work(runtime.store.clone(),move|s|{
            #[cfg(target_os="macos")]
            {
                let mut temporary=None;
                let path=if let Some(id)=document_id {
                    let original=s.library.attachment_path(&id)?;
                    if reveal { original } else {
                        // External readers may edit files; open a copy to keep the managed PDF immutable.
                        use std::io::Write;
                        use std::os::unix::fs::OpenOptionsExt;
                        let (_,bytes)=s.library.document_bytes(&id)?;
                        let path=std::env::temp_dir().join(format!("Orbit-preview-{}.pdf",uuid::Uuid::new_v4()));
                        let result=(||{
                            let mut f=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&path).map_err(|_|"无法创建 PDF 阅读副本")?;
                            f.write_all(&bytes).and_then(|_|f.sync_all()).map_err(|_|"PDF 阅读副本写入失败")
                        })();
                        if let Err(e)=result {let _=std::fs::remove_file(&path);return Err(e.into());}
                        temporary=Some(path.clone());path
                    }
                } else {s.library.directory().to_path_buf()};
                let mut command=std::process::Command::new("/usr/bin/open");
                if reveal {command.arg("-R");}
                let result=command.arg(&path).status().map_err(|_|"无法调用系统打开操作").and_then(|status|if status.success(){Ok(())}else{Err("系统未能打开资料")});
                if result.is_err(){if let Some(path)=temporary{let _=std::fs::remove_file(path);}}
                result.map_err(str::to_string)
            }
            #[cfg(not(target_os="macos"))]
            { let _=(s,document_id,reveal);Err("此平台暂不支持系统打开操作".into()) }
        }).await
    }
    #[tauri::command]
    async fn fetch_web_document(runtime:State<'_,Runtime>,url:String)->Result<Document,String>{library_work(runtime.store.clone(),move|s|s.library.create(fetch_web(&url)?)).await}
    #[tauri::command]
    async fn save_pdf_reader(runtime:State<'_,Runtime>,document_id:String,value:crate::knowledge::PdfReaderData)->Result<crate::knowledge::PdfReaderData,String>{library_work(runtime.store.clone(),move|s|s.library.save_pdf_reader(&document_id,value)).await}
    #[tauri::command]
    async fn read_pdf(runtime:State<'_,Runtime>,document_id:String)->Result<String,String>{library_work(runtime.store.clone(),move|s|s.library.pdf_base64(&document_id)).await}
    #[tauri::command]
    async fn export_document(runtime:State<'_,Runtime>,document_id:String)->Result<String,String>{library_work(runtime.store.clone(),move|s|s.export_document(&document_id)).await}
    #[tauri::command]
    async fn collect_artifact(runtime:State<'_,Runtime>,artifact_id:String)->Result<Document,String>{library_work(runtime.store.clone(),move|s|s.collect_artifact(&artifact_id)).await}
    #[tauri::command]
    async fn import_documents(app:AppHandle,runtime:State<'_,Runtime>)->Result<Vec<Document>,String>{
        let store=runtime.store.clone();
        tauri::async_runtime::spawn_blocking(move||{
            let mut picker=app.dialog().file().set_title("导入 PDF 或 Markdown").add_filter("文档",&["pdf","md","markdown","txt"]);
            if let Some(window)=app.get_webview_window("main"){picker=picker.set_parent(&window);}
            match picker.blocking_pick_file(){Some(path)=>Ok(vec![store.library.import_path(&path.into_path().map_err(|_|"请选择本机文件")?)?]),None=>Ok(vec![])}
        }).await.map_err(|_|"文档选择失败".to_string())?
    }
    #[tauri::command]
    fn list_executors(runtime: State<Runtime>) -> Vec<Descriptor> {
        runtime.list_executors()
    }
    #[tauri::command]
    async fn doctor(
        runtime: State<'_, Runtime>,
        provider: Option<String>,
    ) -> Result<Doctor, String> {
        let owned = runtime.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            owned.doctor(provider.as_deref().unwrap_or("codex"))
        })
        .await
        .map_err(|_| "连接检查进程失败".to_string())?
    }
    #[tauri::command]
    async fn list_executor_models(runtime: State<'_, Runtime>, provider: String) -> Result<Vec<crate::executor::ExecutorModel>, String> {
        let owned = runtime.inner().clone();
        tauri::async_runtime::spawn_blocking(move || owned.models(&provider)).await
            .map_err(|_| "模型目录查询进程失败".to_string())?
    }
    #[tauri::command]
    fn create_task(
        runtime: State<Runtime>,
        title: String,
        prompt: String,
        scene: String,
        provider: Option<String>,
        requested_model: Option<String>,
    ) -> Result<Task, String> {
        runtime.create(
            title,
            prompt,
            scene,
            provider.unwrap_or_else(|| "codex".into()),
            requested_model,
        )
    }
    #[tauri::command]
    fn load_queue_state(runtime:State<Runtime>)->crate::runtime::QueueState {runtime.queue_state()}
    #[tauri::command]
    fn set_queue_paused(app:AppHandle,runtime:State<Runtime>,paused:bool)->Result<crate::runtime::QueueState,String>{
        let state=runtime.set_queue_paused(paused)?;let _=tauri::Emitter::emit(&app,"runtime-queue",&state);Ok(state)
    }
    #[tauri::command]
    fn cancel_queued(app:AppHandle,runtime:State<Runtime>,task_id:String,revision:u64)->Result<Task,String>{
        let task=runtime.cancel_queued(task_id,revision)?;let _=tauri::Emitter::emit(&app,"runtime-task",&task);Ok(task)
    }
    #[tauri::command]
    fn accept_task(app:AppHandle,runtime:State<Runtime>,task_id:String,revision:u64,run_id:Option<String>,turn_id:Option<String>)->Result<Task,String>{
        let task=runtime.accept_task(task_id,revision,run_id,turn_id)?;let _=tauri::Emitter::emit(&app,"runtime-task",&task);Ok(task)
    }
    #[tauri::command]
    async fn start_run(
        app: AppHandle,
        runtime: State<'_, Runtime>,
        task_id: String,
        revision: u64,
    ) -> Result<Task, String> {
        let owned = runtime.inner().clone();
        tauri::async_runtime::spawn_blocking(move || owned.start(app, task_id, revision))
            .await
            .map_err(|_| "启动进程失败".to_string())?
    }
    #[tauri::command]
    async fn continue_task(
        app: AppHandle,
        runtime: State<'_, Runtime>,
        task_id: String,
        revision: u64,
        run_id: Option<String>,
        turn_id: Option<String>,
        text: String,
    ) -> Result<Task, String> {
        let owned = runtime.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            owned.continue_task(app, task_id, revision, run_id, turn_id, text)
        })
        .await
        .map_err(|_| "续接进程失败".to_string())?
    }
    #[tauri::command]
    fn steer_run(
        app: AppHandle,
        runtime: State<Runtime>,
        run_id: String,
        text: String,
    ) -> Result<(), String> {
        runtime.steer(&app, run_id, text)
    }
    #[tauri::command]
    fn interrupt_run(
        app: AppHandle,
        runtime: State<Runtime>,
        run_id: String,
    ) -> Result<(), String> {
        runtime.interrupt(app, run_id)
    }
    #[tauri::command]
    fn reply_input(
        app: AppHandle,
        runtime: State<Runtime>,
        run_id: String,
        approval_id: String,
        answers: std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        runtime.reply(&app, run_id, approval_id, answers)
    }

    pub fn run() {
        // Distribution smoke check: same binary and CLI lookup as the UI, without
        // opening a window, creating a task, or invoking a model.
        if std::env::args().skip(1).any(|arg| arg == "--doctor") {
            let result = runner::doctor();
            println!(
                "{}",
                serde_json::to_string(&result).expect("doctor serialization")
            );
            return;
        }
        let app = tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .setup(|app| {
                let directory = app.path().app_data_dir()?;
                let store = Store::open(directory).map_err(std::io::Error::other)?;
                let runtime=Runtime::new(store);
                runtime.start_dispatcher(app.handle().clone());
                app.manage(runtime);
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                load_workspace,
                get_export_settings,
                choose_export_directory,
                reset_export_directory,
                archive_task,
                delete_task,
                export_artifact,
                export_workspace,
                load_library,
                save_collection,
                delete_collection,
                organize_document,
                trash_document,
                restore_document,
                purge_document,
                get_library_directory,
                open_library_location,
                create_document,
                change_document,
                save_smart_group,
                fetch_web_document,
                read_pdf,
                save_pdf_reader,
                export_document,
                collect_artifact,
                import_documents,
                edit_artifact,
                sync_agents,
                doctor,
                list_executors,
                list_executor_models,
                create_task,
                load_queue_state, set_queue_paused, cancel_queued, accept_task,
                start_run,
                continue_task,
                steer_run,
                interrupt_run,
                reply_input
            ])
            .on_window_event(|window, event| {
                if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                    window.state::<Runtime>().shutdown();
                }
            })
            .build(tauri::generate_context!())
            .expect("unable to start Orbit");
        app.run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                app.state::<Runtime>().shutdown();
            }
        });
    }
}
#[cfg(feature = "desktop")]
pub use desktop::run;
