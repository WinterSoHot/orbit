import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Task } from './model';
import type { Capabilities } from './model';
import { previewExecutors } from './executors';
import type { ExecutorDescriptor } from './executors';

export const desktop = isTauri();
export interface Doctor { provider:string; available: boolean; initialized: boolean; path: string; version: string; message: string; capabilities:Capabilities }
export const listExecutors=():Promise<ExecutorDescriptor[]>=>desktop?invoke('list_executors'):Promise.resolve(previewExecutors);

export async function loadTasks(): Promise<{tasks:Task[];error:string|null}> {
  if(desktop) return invoke('load_workspace');
  return {tasks:[],error:null};
}
export async function watchRuntime(callback:(task:Task)=>void,onWarning:(message:string)=>void) {
  if(!desktop)return ()=>{};
  const releaseTask=await listen<Task>('runtime-task',event=>callback(event.payload));
  const releaseWarning=await listen<string>('runtime-warning',event=>onWarning(event.payload));
  return ()=>{releaseTask();releaseWarning();};
}
export const checkCli=(provider:string)=>invoke<Doctor>('doctor',{provider});
export const exportArtifact=(artifactId:string)=>invoke<string>('export_artifact',{artifactId});
// Legacy storage/IPC still requires scene; it no longer controls UI or execution.
export const createReal=(title:string,prompt:string,provider:string)=>invoke<Task>('create_task',{title,prompt,scene:'research',provider});
export const startReal=(task:Task)=>invoke<Task>('start_run',{taskId:task.id,revision:task.revision});
export const steerReal=(runId:string,text:string)=>invoke('steer_run',{runId,text});
export const cancelReal=(runId:string)=>invoke('interrupt_run',{runId});
export const replyReal=(runId:string,approvalId:string,answers:Record<string,string>)=>invoke('reply_input',{runId,approvalId,answers});

export const editArtifact=(artifactId:string,expectedContent:string,content:string)=>invoke<Task>('edit_artifact',{artifactId,expectedContent,content});

export const syncAgents=(taskId:string)=>invoke<Task>('sync_agents',{taskId});

export const archiveTask=(taskId:string)=>invoke<Task>('archive_task',{taskId});
export const deleteTask=(taskId:string)=>invoke('delete_task',{taskId});

export const continueReal=(task:Task,text:string)=>invoke<Task>('continue_task',{taskId:task.id,revision:task.revision,runId:task.runId,turnId:task.turnId,text});

export interface ExportSettings { directory:string; custom:boolean }
export const getExportSettings=()=>invoke<ExportSettings>('get_export_settings');
export const chooseExportDirectory=()=>invoke<ExportSettings|null>('choose_export_directory');
export const resetExportDirectory=()=>invoke<ExportSettings>('reset_export_directory');

export const exportWorkspace=()=>invoke<string>('export_workspace');

// Independent local knowledge library; browsers remain read-only previews.
export const loadLibrary=()=>desktop?invoke<import('./knowledge').Library>('load_library'):Promise.resolve({documents:[],groups:[],collections:[],schemaVersion:3,error:null});
export const createDocument=(input:{title:string;kind:string;content:string;url:string|null;tags:string[]})=>invoke<import('./knowledge').LibraryDocument>('create_document',{input});
export const importDocuments=()=>invoke<import('./knowledge').LibraryDocument[]>('import_documents');
export const fetchWebDocument=(url:string)=>invoke<import('./knowledge').LibraryDocument>('fetch_web_document',{url});
export const changeDocument=(change:import('./knowledge').DocumentChange)=>invoke<import('./knowledge').LibraryDocument>('change_document',{change});
export const saveSmartGroup=(group:import('./knowledge').SmartGroup)=>invoke<import('./knowledge').SmartGroup>('save_smart_group',{group});
export const readPdf=(documentId:string)=>invoke<string>('read_pdf',{documentId});
export const exportDocument=(documentId:string)=>invoke<string>('export_document',{documentId});
export const collectArtifact=(artifactId:string)=>invoke<import('./knowledge').LibraryDocument>('collect_artifact',{artifactId});

export const saveCollection=(collection:import('./knowledge').Collection)=>invoke<import('./knowledge').Collection>('save_collection',{collection});
export const deleteCollection=(collectionId:string,revision:number)=>invoke<import('./knowledge').Library>('delete_collection',{collectionId,revision});
export const organizeDocument=(documentId:string,revision:number,collectionIds:string[])=>invoke<import('./knowledge').LibraryDocument>('organize_document',{documentId,revision,collectionIds});
export const trashDocument=(documentId:string,revision:number)=>invoke<import('./knowledge').LibraryDocument>('trash_document',{documentId,revision});
export const restoreDocument=(documentId:string,revision:number)=>invoke<import('./knowledge').LibraryDocument>('restore_document',{documentId,revision});
export const purgeDocument=(documentId:string,revision:number)=>invoke<string|null>('purge_document',{documentId,revision});
export const getLibraryDirectory=()=>desktop?invoke<string>('get_library_directory'):Promise.resolve('');
export const openLibraryLocation=(documentId:string|null=null,reveal=false)=>invoke('open_library_location',{documentId,reveal});

export const savePdfReader=(documentId:string,value:import("./pdfReaderState").PdfReaderData)=>invoke<import("./pdfReaderState").PdfReaderData>("save_pdf_reader",{documentId,value});

export interface QueueState {paused:boolean;reason:string|null}
export const loadQueue=():Promise<QueueState>=>desktop?invoke('load_queue_state'):Promise.resolve({paused:true,reason:'在桌面 App 中运行任务'});
export const pauseQueue=(paused:boolean)=>invoke<QueueState>('set_queue_paused',{paused});
export const cancelQueued=(task:Task)=>invoke<Task>('cancel_queued',{taskId:task.id,revision:task.revision});
export const acceptTask=(task:Task)=>invoke<Task>('accept_task',{taskId:task.id,revision:task.revision,runId:task.runId,turnId:task.turnId});
export const watchQueue=async(callback:(queue:QueueState)=>void)=>desktop?listen<QueueState>('runtime-queue',event=>callback(event.payload)):()=>{};
