import {AgentManager,TeamEditor,TeamCard,DiffDialog} from "./TeamUI";
import type {TeamAction} from "./TeamUI";
import type {AgentProfile,CodeEvidence,PlanDraft} from "./teams";
import {rootTasks} from "./teams";
import {listAgents,saveAgent,deleteAgent,createTeam,confirmTeam,reviseTeam,reviseSummary,cancelTeam,snapshotCode,integrateCode,mergeCode,openCodeWorkspace} from "./bridge";
import {applyTheme,initialTheme,saveTheme} from './theme';
import type {Theme} from './theme';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Activity, BookOpen, Archive, Trash2, ArrowUpRight, RefreshCw, Bell, Bot, CheckCircle2, CircleHelp, Clock3, Command, FileText, FolderOpen, GitBranch, LayoutDashboard, ListFilter, Loader2, PanelLeftClose, PanelLeftOpen, Plus, Search, Send, Settings2, ShieldCheck, Sparkles, Square, Terminal, X, Zap } from 'lucide-react';
import { applyRuntime, canContinue, isAccepted, boardColumn, isActive, isTerminal, taskCapabilities, statusLabels } from './model';
import type { AgentNode, Artifact, ChatItem, Status, Task } from './model';
import { saveMessage, loadQueue, watchQueue, pauseQueue, cancelQueued, acceptTask, loadLibrary, collectArtifact, listExecutorModels, listExecutors, getExportSettings, chooseExportDirectory, resetExportDirectory, cancelReal, checkCli, createReal, desktop, editArtifact, exportArtifact, exportWorkspace, loadTasks, replyReal, archiveTask, deleteTask, continueReal, startReal, syncAgents, steerReal, watchRuntime } from './bridge';
import type { Doctor, ExportSettings, QueueState } from './bridge';
import { ExecutorPicker } from './ExecutorPicker';
import { ExecutorSettings } from './ExecutorSettings';
import { executorName } from './executors';
import {modelAvailable,modelDefaultsKey,readModelDefaults} from './modelSelection';
import type { ModelCatalogState, ExecutorDescriptor } from './executors';
import { exportAfterSave } from './exportSnapshot';
import { ExportDirectorySettings } from './ExportDirectorySettings';
import { Graph } from './Graph';
import {KnowledgeLibrary} from './KnowledgeLibrary';
import type {LibraryHandle} from './KnowledgeLibrary';
import { TaskChat } from './TaskChat';
import {RealApprovalForm} from './ApprovalForm';
import {AttentionPanel} from './AttentionPanel';
import {TaskWorkspace} from './TaskWorkspace';
import {SourceComposer} from './SourceComposer';
import type {SourceSelection} from './SourceComposer';
import type {SourceRequest,SourceInput} from './taskSources';
import type {DocumentLocation} from './pdfReaderState';
import './workspace.css';
import { AgentOutput, ArtifactEditor } from './ArtifactEditor';

type View='tasks'|'archive'|'attention'|'artifacts'|'settings'|'knowledge'|'agents';
const time=(n:number)=>new Date(n).toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit',second:'2-digit'});
const duration=(task:Task,now:number)=>{if(!task.startedAt)return '尚未启动';const seconds=Math.max(0,Math.round(((task.finishedAt||now)-task.startedAt)/1000));return seconds<60?`${seconds} 秒`:`${Math.floor(seconds/60)} 分 ${seconds%60} 秒`;};
const errorText=(error:unknown)=>error instanceof Error?error.message:String(error);

function Badge({status}:{status:Status}) {return <span className={`status-badge ${status}`}><span className={`status-dot ${status}`}/>{statusLabels[status]}</span>;}

export default function App() {
  const [agents,setAgents]=useState<AgentProfile[]>([]),[teamEditor,setTeamEditor]=useState<Task|null|undefined>(undefined),[codeDiff,setCodeDiff]=useState<CodeEvidence|null>(null);
  const [theme,setTheme]=useState(initialTheme);
  const [sidebarCollapsed,setSidebarCollapsed]=useState(()=>{try{return window.localStorage.getItem('orbit.sidebarCollapsed')==='true';}catch{return false;}});
  function toggleSidebar(){const next=!sidebarCollapsed;setSidebarCollapsed(next);try{window.localStorage.setItem('orbit.sidebarCollapsed',String(next));}catch{/* Navigation still works when storage is unavailable. */}}
  function chooseTheme(value:Theme){applyTheme(value);setTheme(value);try{if(!saveTheme(value,window.localStorage))setToast('主题已生效，但无法保存选择');}catch{setToast('主题已生效，但无法保存选择');}}
  const [tasks,setTasks]=useState<Task[]>([]),[loaded,setLoaded]=useState(false),[view,setView]=useState<View>('tasks');
  const [selectedId,setSelectedId]=useState<string|null>(null),[nodeId,setNodeId]=useState<string|null>(null);
  const [query,setQuery]=useState(''),[taskFilter,setTaskFilter]=useState<'all'|'active'|'completed'>('all');
  const [newTaskSeed,setNewTaskSeed]=useState<Task|null>(null);
  const [modal,setModal]=useState(false),[artifact,setArtifact]=useState<Artifact|null>(null),[toast,setToast]=useState(''),[agentOutput,setAgentOutput]=useState<AgentNode|null>(null);
  const [doctors,setDoctors]=useState<Record<string,Doctor>>({}),[checking,setChecking]=useState<string|null>(null),[busy,setBusy]=useState(false),[steer,setSteer]=useState(''),[now,setNow]=useState(Date.now());
  const [queue,setQueue]=useState<QueueState>({paused:false,reason:null});
  const [chatDrafts,setChatDrafts]=useState<Record<string,string>>({});
  const [sourceDrafts,setSourceDrafts]=useState<Record<string,SourceSelection[]>>({});
  const [libraryLocation,setLibraryLocation]=useState<DocumentLocation|null>(null);
  const artifactOwner=tasks.find(t=>t.artifacts.some(a=>a.id===artifact?.id));
  const artifactSources=(artifactOwner?.sourceInputs||[]).filter(i=>artifact?.sourceInputIds?.includes(i.id));
  async function openSource(id:string,location:DocumentLocation|undefined,inputs:SourceInput[]){
    await perform(async()=>{
      if(!inputs.some(i=>i.sources.some(s=>s.documentId===id&&(!location?.page||s.pages.includes(location.page))&&(!location?.annotation||s.annotationId===location.annotation))))throw Error('此链接不属于本次提供的资料');
      const data=await loadLibrary();if(data.error)throw Error(data.error);
      const doc=data.documents.find(d=>d.id===id&&d.deletedAt==null);
      if(!doc)throw Error('原资料已删除，仍可在资料记录中查看发送时的摘录');
      const source=inputs.flatMap(i=>i.sources).find(s=>s.documentId===id&&(!location?.page||s.pages.includes(location.page))&&(!location?.annotation||s.annotationId===location.annotation))!;
      const missingAnnotation=!!location?.annotation&&!doc.pdfReader?.annotations.some(a=>a.id===location.annotation);
      const changed=doc.revision!==source.revision||source.readerRevision!==null&&source.readerRevision!==doc.pdfReader?.revision;
      await library.current?.flush();setArtifact(null);setLibraryLocation(missingAnnotation?{id,page:location?.page}:location||{id});setView('knowledge');
      if(missingAnnotation)setToast('原批注已移除，已打开对应页面；发送时的摘录仍保留');
      else if(changed)setToast('原资料已更新，正在显示当前版本；发送时的摘录可在资料记录中查看');
    });
  }
  const [exportSettings,setExportSettings]=useState<ExportSettings|null>(null);
  const [modelDefaults,setModelDefaults]=useState<Record<string,string|null>>(()=>{try{return readModelDefaults(window.localStorage);}catch{return {};}});
  const [modelCatalogs,setModelCatalogs]=useState<Record<string,ModelCatalogState>>({});
  const modelCache=useRef<Record<string,ModelCatalogState>>({}),modelQueries=useRef(new Set<string>());
  const loadModels=useCallback((provider:string,refresh=false)=>{
    if(!desktop||!provider||modelQueries.current.has(provider)||!refresh&&modelCache.current[provider])return;
    modelQueries.current.add(provider);
    const publish=(value:ModelCatalogState)=>{modelCache.current={...modelCache.current,[provider]:value};setModelCatalogs(modelCache.current);};
    publish({...modelCache.current[provider],loading:true,error:null});
    void listExecutorModels(provider).then(models=>publish({models,loading:false,error:null}),error=>publish({loading:false,error:errorText(error)})).finally(()=>modelQueries.current.delete(provider));
  },[]);
  function chooseModel(provider:string,model:string|null){
    const next={...modelDefaults,[provider]:model};setModelDefaults(next);
    try{window.localStorage.setItem(modelDefaultsKey,JSON.stringify(next));}catch{setToast('模型选择已应用，本机存储不可用，重启后不会保留');}
  }
  const [executors,setExecutors]=useState<ExecutorDescriptor[]>([]);
  const continuing=useRef(false),lastSave=useRef<Promise<unknown>|null>(null);
  const library=useRef<LibraryHandle>(null);
  const persistence=useRef(true),saveQueue=useRef(Promise.resolve()),mounted=useRef(false),deletedIds=useRef(new Set<string>());
  const visibleTasks=tasks.filter(t=>view==='archive'?t.archived:!t.archived);
  const task=visibleTasks.find(t=>t.id===selectedId)||visibleTasks[0];
  const node=task?.nodes.find(n=>n.id===nodeId)||task?.nodes[0];
  const attention=rootTasks(tasks).filter(t=>!t.archived&&boardColumn(t)==='attention');
  const deliveries=tasks.filter(t=>!t.archived).flatMap(t=>t.artifacts.map(a=>({task:t,artifact:a})));

  useEffect(()=>{
    mounted.current=true;let unlisten:(()=>void)|undefined;
    (async()=>{
      const release=await watchRuntime(incoming=>{if(mounted.current&&!deletedIds.current.has(incoming.id))setTasks(current=>{if(deletedIds.current.has(incoming.id))return current;const found=current.find(t=>t.id===incoming.id);return found?current.map(t=>t.id===incoming.id?applyRuntime(t,incoming):t):[incoming,...current];});},message=>{if(mounted.current){persistence.current=false;setToast(message);}});
      if(!mounted.current){release();return;}unlisten=release;
      const result=await loadTasks();if(!mounted.current)return;
      if(result.error){persistence.current=false;setToast(result.error);}
      const initial=result.tasks;setTasks(initial);setSelectedId(initial[0]?.id||null);setLoaded(true);
    })().catch(error=>{if(mounted.current){persistence.current=false;setTasks([]);setLoaded(true);setToast(`记录读取失败：${errorText(error)}`);}});
    return()=>{mounted.current=false;unlisten?.();};
  },[]);
  useEffect(()=>{void listAgents().then(setAgents).catch(e=>setToast(errorText(e)));},[]);
  useEffect(()=>{let alive=true;listExecutors().then(catalog=>{if(alive)setExecutors(catalog);}).catch(error=>{if(alive)setToast(`执行器目录读取失败：${errorText(error)}`);});return()=>{alive=false;};},[]);
  useEffect(()=>{let alive=true;let release:(()=>void)|undefined;(async()=>{const stop=await watchQueue(value=>{if(alive)setQueue(value);});if(!alive){stop();return;}release=stop;const value=await loadQueue();if(alive)setQueue(value);})().catch(error=>{if(alive)setToast(errorText(error));});return()=>{alive=false;release?.();};},[]);
  useEffect(()=>{if(!desktop)return;let alive=true;getExportSettings().then(value=>{if(alive)setExportSettings(value);}).catch(error=>{if(alive)setToast(`导出设置读取失败：${errorText(error)}`);});return()=>{alive=false;};},[]);
  useEffect(()=>{if(!loaded)return;const timer=setInterval(()=>setNow(Date.now()),3500);return()=>clearInterval(timer);},[loaded]);
  useEffect(()=>{if(!toast)return;const timer=setTimeout(()=>setToast(''),6500);return()=>clearTimeout(timer);},[toast]);
  useEffect(()=>{const key=(event:KeyboardEvent)=>{if(artifact||agentOutput||teamEditor!==undefined||codeDiff)return;if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==='n'){event.preventDefault();setNewTaskSeed(null);setModal(true);}if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==='k'&&view!=='tasks'){event.preventDefault();void navigate('tasks');const search=document.querySelector<HTMLInputElement>('input[aria-label="搜索任务"]');const filter=search?.closest('details');if(filter)filter.open=true;search?.focus();}if(event.key==='Escape'){setModal(false);}};window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);},[artifact,agentOutput,view,teamEditor,codeDiff]);

  async function navigate(next:View){try{if(view==='knowledge'&&next!=='knowledge')await library.current?.flush();setView(next);}catch(error){setToast(errorText(error));}}
  async function saveToLibrary(file:Artifact){await perform(async()=>{await saveQueue.current;const saved=await collectArtifact(file.id);setToast(`已存入知识库：${saved.title}`);});}
  async function perform(work:()=>Promise<unknown>) {setBusy(true);try{await work();}catch(error){setToast(errorText(error));}finally{setBusy(false);}}
  function select(t:Task){setSelectedId(t.id);setNodeId(t.nodes[0]?.id||null);setSteer('');}
  function record(saved:Task){setTasks(current=>current.map(t=>t.id===saved.id&&t.revision<=saved.revision?saved:t));}
  async function launch(target:Task){
    if(target.queue||isActive(target.status))return;
    if(!target.parentLink&&target.artifacts.length&&!window.confirm('重新开始会建立新会话并清空本任务的旧交付。若要保留版本，请在任务对话中发送补充信息。继续重新开始？'))return;
    await perform(async()=>{await saveQueue.current;const saved=await startReal(target);record(saved);select(saved);setToast('任务已加入等待队列');});
  }
  async function sendDirection(){const text=steer.trim();if(!text||!task||busy)return;await perform(async()=>{await steerReal(task.runId!,text);setSteer('');});}
  async function steerConversation(){
    if(!task||task.queue||!isActive(task.status)||task.status==='cancelling'||!taskCapabilities(task).steer||!desktop)return;
    const id=task.id,draft=chatDrafts[id]||'';if(!draft.trim())return;
    await perform(async()=>{await steerReal(task.runId!,draft.trim(),(sourceDrafts[id]||[]).map(s=>s.request));setSourceDrafts(current=>({...current,[id]:[]}));setChatDrafts(current=>current[id]===draft?{...current,[id]:''}:current);});
  }
  async function continueDelivery(){
    if(!task||!(canContinue(task)||task.team&&['revision','ready'].includes(task.team.phase))||busy||continuing.current||!desktop)return;
    const target=task,draft=chatDrafts[target.id]||'',text=draft.trim();if(!text)return;
    continuing.current=true;
    await perform(async()=>{await saveQueue.current;const incoming=target.team?await reviseSummary(target,text):await continueReal(target,text,(sourceDrafts[target.id]||[]).map(s=>s.request));record(incoming);if(target.team)await reloadTeams();setSourceDrafts(current=>({...current,[target.id]:[]}));setChatDrafts(current=>current[target.id]===draft?{...current,[target.id]:''}:current);setToast('补充已加入等待队列，已有交付版本保留');});
    continuing.current=false;
  }
  async function reloadTeams(){const result=await loadTasks();if(result.error)throw Error(result.error);setTasks(result.tasks);setAgents(await listAgents());}
  async function saveProfile(a:AgentProfile){setBusy(true);try{await saveAgent(a);setAgents(await listAgents());}catch(e){setToast(errorText(e));throw e;}finally{setBusy(false);}}
  async function removeProfile(a:AgentProfile){setBusy(true);try{await deleteAgent(a);setAgents(await listAgents());}catch(e){setToast(errorText(e));throw e;}finally{setBusy(false);}}
  async function saveTeam(d:PlanDraft){setBusy(true);try{if(view==='knowledge')await library.current?.flush();const saved=teamEditor?await reviseTeam(teamEditor,d):await createTeam(d);await reloadTeams();select(saved);setView('tasks');setTeamEditor(undefined);}catch(e){setToast(errorText(e));throw e;}finally{setBusy(false);}}
  async function teamAction(action:TeamAction,t:Task){await perform(async()=>{const commands={confirm:confirmTeam,cancel:cancelTeam,snapshot:snapshotCode,integrate:integrateCode,merge:mergeCode,workspace:openCodeWorkspace};await commands[action](t);await reloadTeams();});}
  async function cancelPending(target:Task){await perform(async()=>{record(await cancelQueued(target));setToast('等待请求已撤销，历史保留');});}
  async function acceptDelivery(target:Task){await perform(async()=>{await saveQueue.current;record(await acceptTask(target));setToast('最新交付已验收');});}
  async function controlQueue(paused:boolean){await perform(async()=>setQueue(await pauseQueue(paused)));}
  async function stop(target:Task|undefined=task){if(!target?.runId)return;await perform(()=>cancelReal(target.runId!));}
  async function answer(target:Task,id:string,answers:Record<string,string>){await perform(()=>replyReal(target.runId!,id,answers));}
  async function chooseFolder(){await perform(async()=>{const value=await chooseExportDirectory();if(value){setExportSettings(value);setToast(`导出文件夹已设置：${value.directory}`);}});}
  async function resetFolder(){await perform(async()=>{const value=await resetExportDirectory();setExportSettings(value);setToast('导出文件夹已恢复默认');});}
  async function inspectCli(provider:string){setChecking(provider);try{const result=await checkCli(provider);setDoctors(current=>({...current,[provider]:result}));}catch(error){setToast(errorText(error));}finally{setChecking(null);}}
  async function create(title:string,prompt:string,provider:string,model:string|null,sources:SourceRequest[]){await perform(async()=>{if(view==='knowledge')await library.current?.flush();const next=await createReal(title,prompt,provider,model,sources);setTasks(current=>[next,...current]);select(next);setView('tasks');setTaskFilter('all');setQuery('');setModal(false);});}
  async function saveReply(owner:Task,item:ChatItem){await perform(async()=>{const saved=await saveMessage(owner,item);setTasks(current=>current.map(t=>t.id===saved.id&&t.revision<=saved.revision?saved:t));setToast('回复已保存为 Markdown');});}
  async function saveDocument(expected:string,content:string):Promise<Artifact>{
    const id=artifact!.id;
    const operation=saveQueue.current.then(async()=>{
      if(!persistence.current)throw new Error('本地记录无法写入，草稿已保留，请先解决保存问题');
      const saved=await editArtifact(id,expected,content);
      setTasks(current=>current.map(t=>t.id===saved.id&&t.revision<=saved.revision?saved:t));
      return saved.artifacts.find(a=>a.id===id)!;
    });
    lastSave.current=operation;
    saveQueue.current=operation.then(()=>{},()=>{});
    return operation;
  }
  async function syncCollaboration(){if(!task)return;await perform(async()=>{const saved=await syncAgents(task.id);setTasks(current=>current.map(t=>t.id===saved.id&&t.revision<=saved.revision?saved:t));setToast(`协作记录已同步：${saved.nodes.length} 个 Agent`);});}
  async function exportAllData(){await perform(async()=>{const path=await exportAfterSave(lastSave.current,exportWorkspace);setToast(`全部工作台数据已导出：${path}`);});}
  async function download(file:Artifact){await perform(async()=>{await saveQueue.current;const path=await exportArtifact(file.id);setToast(`交付已保存：${path}`);});}
  async function archive(target:Task|undefined=task){if(!target)return;await perform(async()=>{const saved=await archiveTask(target.id);setTasks(current=>current.map(t=>t.id===saved.id?saved:t));if(target.team)await reloadTeams();setToast('任务已归档，可在归档列表查看和导出');});}
  async function remove(target:Task|undefined=task){if(!target||deletedIds.current.has(target.id)||!window.confirm(`删除“${target.title}”？此操作永久删除工作台记录及内嵌正文。执行器会话、运行目录和已导出的文件仍保留。`))return;const id=target.id,ids=tasks.filter(t=>t.id===id||t.parentLink?.parentId===id).map(t=>t.id);for(const key of ids)deletedIds.current.add(key);await perform(async()=>{try{await saveQueue.current;await deleteTask(id);setTasks(current=>current.filter(t=>!ids.includes(t.id)));setToast('任务已删除');}catch(error){const latest=await loadTasks();for(const key of ids)if(latest.tasks.some(t=>t.id===key))deletedIds.current.delete(key);if(latest.error){persistence.current=false;setToast(latest.error);}setTasks(latest.tasks);throw error;}});}
  async function exportDelivery(){if(!task)return;await perform(async()=>{await saveQueue.current;const paths=[];for(const file of task.artifacts)paths.push(await exportArtifact(file.id));setToast(`已导出 ${paths.length} 份交付：${paths.join('；')}`);});}
  const sourceComposer=task?<SourceComposer value={sourceDrafts[task.id]||[]} disabled={busy||!!task.archived||!!task.queue||!(canContinue(task)||isActive(task.status)&&taskCapabilities(task).steer&&task.status!=='cancelling')} onChange={value=>setSourceDrafts(current=>({...current,[task.id]:value}))}/>:undefined;
  const filtered=rootTasks(visibleTasks).filter(t=>(!query||`${t.title} ${t.prompt}`.toLowerCase().includes(query.toLowerCase()))&&(taskFilter==='all'||(taskFilter==='active'?boardColumn(t)==='progress':isAccepted(t))));

  return <div className={`app-shell ${sidebarCollapsed?'sidebar-collapsed':''} ${view==='tasks'?'compact-workspace':view==='knowledge'?'library-workspace':`aux-workspace ${view}-page`}`}>
    <aside className="orbit-sidebar" aria-label="侧边栏">
      <div className="sidebar-heading"><div className="brand" title="Orbit"><img src="/orbit.svg" alt=""/><span>Orbit<span className="brand-sub">你的 Agent 工作台</span></span></div><button className="icon-button sidebar-toggle" aria-label={sidebarCollapsed?'展开侧边栏':'收起侧边栏'} title={sidebarCollapsed?'展开侧边栏':'收起侧边栏'} aria-expanded={!sidebarCollapsed} aria-controls="orbit-navigation" onClick={toggleSidebar}>{sidebarCollapsed?<PanelLeftOpen size={18}/>:<PanelLeftClose size={18}/>}</button></div>
      <button className="nav-item new-task-entry" aria-label="新建任务" title="新建任务 (⌘ N)" onClick={()=>{setNewTaskSeed(null);setModal(true);}}><Plus size={17}/><span className="nav-label">新建任务</span><span className="nav-meta">⌘ N</span></button>
      <nav id="orbit-navigation" aria-label="主导航">
        <button className={view==='tasks'?'nav-item active':'nav-item'} aria-label="任务工作台" title={`任务工作台 · ${rootTasks(tasks).filter(t=>!t.archived).length} 个任务`} onClick={()=>void navigate('tasks')}><LayoutDashboard size={17}/><span className="nav-label">任务工作台</span><span className="nav-meta">{rootTasks(tasks).filter(t=>!t.archived).length}</span></button>
        <button className={view==='attention'?'nav-item active':'nav-item'} aria-label={`待介入${attention.length?` · ${attention.length} 项`:''}`} title={`待介入 · ${attention.length} 项`} onClick={()=>void navigate('attention')}><Bell size={17}/><span className="nav-label">待介入</span>{attention.length>0&&<span className="nav-meta attention-count">{attention.length}</span>}</button>
        <button className={view==='archive'?'nav-item active':'nav-item'} aria-label="归档" title={`归档 · ${rootTasks(tasks).filter(t=>t.archived).length} 个任务`} onClick={()=>{void navigate('archive');setQuery('');setTaskFilter('all');}}><Archive size={17}/><span className="nav-label">归档</span><span className="nav-meta">{rootTasks(tasks).filter(t=>t.archived).length}</span></button>
        <button className={view==='artifacts'?'nav-item active':'nav-item'} aria-label="交付物" title={`交付物 · ${deliveries.length} 份`} onClick={()=>void navigate('artifacts')}><FolderOpen size={17}/><span className="nav-label">交付物</span><span className="nav-meta">{deliveries.length}</span></button>
        <button className={view==='knowledge'?'nav-item active':'nav-item'} aria-label="知识库" title="知识库" onClick={()=>void navigate('knowledge')}><BookOpen size={17}/><span className="nav-label">知识库</span></button>
        <button className={view==='agents'?'nav-item active':'nav-item'} aria-label="Agent" title="Agent 配置" onClick={()=>void navigate('agents')}><Bot size={17}/><span className="nav-label">Agent</span></button>
      </nav>
      <div className="sidebar-bottom">{(!desktop||!persistence.current)&&<div className="sidebar-storage-note" title={!desktop?'浏览器仅预览':'保存不可用，修改暂存内存'}><CircleHelp size={16}/><span>{!desktop?'浏览器仅预览':'保存不可用，修改暂存内存'}</span></div>}<button className={view==='settings'?'nav-item active':'nav-item'} aria-label="设置" title="设置" onClick={()=>void navigate('settings')}><Settings2 size={17}/><span className="nav-label">设置</span></button><div className="profile" title="Orbit · 版本 0.1 · 本地工作台"><img src="/orbit.svg" alt=""/><div>Orbit<small>版本 0.1 · 本地工作台</small></div></div></div>
    </aside>
    <main className="main-shell">
      {view!=='tasks'&&<header className="topbar"><div className="breadcrumb">Orbit <span>/</span> <strong>{view==='archive'?'归档':view==='attention'?'待介入':view==='artifacts'?'交付物':view==='knowledge'?'知识库':view==='agents'?'Agent':'设置'}</strong></div><div className="topbar-right"><span className="local-status"><span className="local-indicator"/>{desktop?'桌面运行':'浏览器预览'}</span><span className="topbar-divider"/><button className="icon-button" aria-label="查看待介入" onClick={()=>void navigate('attention')}><Bell size={17}/>{attention.length>0&&<i/>}</button></div></header>}
      <div className="page-content">
        {view!=='tasks'&&<div className="page-heading"><div><h1>{view==='archive'?'归档':view==='attention'?'待介入':view==='artifacts'?'交付物':view==='knowledge'?'知识库':view==='agents'?'Agent':'设置'}</h1><p>{view==='archive'?'查看已归档的交付，导出或删除任务。':view==='attention'?'处理需要答复、核对或验收的任务。':view==='artifacts'?'预览、编辑和保存任务成果。':view==='knowledge'?'管理 PDF、网页和 Markdown，让文档彼此连接。':view==='agents'?'配置独立 Agent 的执行器、模型与职责。':'管理导出位置与本机执行器。'}</p></div></div>}

        {view==='tasks'&&<TaskWorkspace teamCard={task?.team?<TeamCard task={task} tasks={tasks} busy={busy} desktop={desktop} onSelect={select} onEdit={setTeamEditor} onAction={(a,t)=>void teamAction(a,t)} onStart={launch} onDiff={setCodeDiff}/>:undefined} parent={tasks.find(t=>t.id===task?.parentLink?.parentId)} onDelete={remove} onSaveMessage={saveReply} sourceComposer={sourceComposer} onDocument={(id,location)=>void openSource(id,location,task?.sourceInputs||[])} onDuplicate={target=>{setNewTaskSeed(target);setModal(true);}} shortcutsEnabled={!artifact&&!agentOutput&&!modal&&teamEditor===undefined&&!codeDiff} onNew={()=>{setNewTaskSeed(null);setModal(true);}} tasks={visibleTasks} selected={task} queue={queue} draft={task?chatDrafts[task.id]||'':''} busy={busy} desktop={desktop} executors={executors} onSelect={select} onChange={text=>task&&setChatDrafts(current=>({...current,[task.id]:text}))} onSend={continueDelivery} onSteer={steerConversation} onOpen={setArtifact} onStart={launch} onCancel={cancelPending} onAccept={acceptDelivery} onPause={controlQueue} onArchive={archive} onStop={stop} onSync={syncCollaboration} onAnswer={answer}/>}
        {view==='archive'&&<div className="workbench"><div className="archive-mobile-picker"><label>搜索归档<input aria-label="搜索归档任务" placeholder="搜索任务…" value={query} onChange={e=>setQuery(e.target.value)}/></label><label>选择任务<select aria-label="选择归档任务" disabled={!filtered.length} value={filtered.some(t=>t.id===task?.id)?task?.id||'':''} onChange={e=>{const selected=filtered.find(t=>t.id===e.target.value);if(selected)select(selected);}}><option value="">选择归档任务…</option>{filtered.map(t=><option key={t.id} value={t.id}>{t.title}</option>)}</select></label></div>
          <section className="task-panel"><div className="panel-heading"><h2>{view==='archive'?'归档任务':'任务'}<span>{filtered.length}</span></h2>{view!=='archive'&&<button className="icon-button" aria-label="切换任务筛选" onClick={()=>setTaskFilter(taskFilter==='all'?'active':taskFilter==='active'?'completed':'all')}><ListFilter size={16}/></button>}</div>{view!=='archive'&&<div className="task-tabs">{(['all','active','completed'] as const).map(f=><button key={f} className={f===taskFilter?'active':''} onClick={()=>setTaskFilter(f)}>{f==='all'?'全部':f==='active'?'进行中':'已结束'}</button>)}</div>}<label className="search"><Search size={14}/><input aria-label="搜索任务" placeholder="搜索任务…" value={query} onChange={e=>setQuery(e.target.value)}/></label><div className="task-list">{filtered.map(t=><button key={t.id} className={`task-card ${task?.id===t.id?'selected':''}`} aria-pressed={task?.id===t.id} onClick={()=>select(t)}><div className="task-card-top"><span className={`provider-label ${t.provider}`}>{executorName(t.provider,executors)}</span></div><h3>{t.title}</h3><p>{t.prompt}</p><div className="task-card-footer"><Badge status={t.status}/><span>{t.nodes.length>0&&<><Bot size={12}/>{t.nodes.length}</>}</span></div></button>)}{!filtered.length&&<div className="empty-list"><Search size={22}/><p>{!visibleTasks.length?'暂无任务':'没有匹配的任务'}</p><button onClick={()=>{setQuery('');setTaskFilter('all');}}>清除筛选</button></div>}</div><div className="task-panel-footer"><span className="status-dot completed"/>{!desktop?'浏览器仅预览界面':persistence.current?'自动保存到本地':'当前修改暂存内存'}</div></section>
          <div className="task-detail">{task?<>
            <div className="detail-header"><div className="detail-title"><span className={`provider-label ${task.provider}`}>{task.archived?'已归档':executorName(task.provider,executors)}</span><h2>{task.title}</h2></div><div className="detail-actions">{desktop&&!task.archived&&isTerminal(task.status)&&taskCapabilities(task).agentHistory&&task.threadId&&<button className="secondary-button compact" disabled={busy} onClick={syncCollaboration}><RefreshCw size={13}/>同步协作</button>}{!task.archived&&isAccepted(task)&&!task.queue&&<button className="secondary-button compact" disabled={busy} onClick={()=>archive()}><Archive size={13}/>归档任务</button>}{task.archived&&<><button className="secondary-button compact" disabled={busy} onClick={exportDelivery}><FileText size={13}/>导出交付</button><button className="secondary-button compact" disabled={busy} onClick={()=>void remove()}><Trash2 size={13}/>删除任务</button></>}<Badge status={task.status}/>{!task.archived&&(isActive(task.status)?<button className="secondary-button compact" disabled={busy||task.status==='cancelling'||!taskCapabilities(task).interrupt} onClick={()=>stop()}><Square size={12}/>{task.status==='cancelling'?'请求中':'中断'}</button>:<button className="secondary-button compact" disabled={busy||!desktop||!executors.some(e=>e.id===task.provider)} onClick={()=>launch(task)}>{busy?<Loader2 className="spin" size={13}/>:<Zap size={13}/>} {task.status==='queued'?'启动任务':'重新开始'}</button>)}</div></div>
            {task.team&&<TeamCard task={task} tasks={tasks} busy={busy} desktop={desktop} onSelect={select} onEdit={setTeamEditor} onAction={(a,t)=>void teamAction(a,t)} onStart={launch} onDiff={setCodeDiff}/>}<div className="run-strip"><span><Clock3 size={13}/>{duration(task,now)}</span><span><Bot size={13}/>{task.nodes.length} 个 Agent</span><span><Activity size={13}/>{task.tokens===null?'用量暂不可用':`${task.tokens.toLocaleString()} tokens`}</span><span className="run-id">{task.runId?`运行 ${task.runId.slice(0,8)}`:'尚无运行'}</span></div>
            {task.status==='unknown'&&<div className="inline-notice"><CircleHelp size={16}/><span>上次运行状态需核对。历史已保留，旧审批已失效；新运行不会自动重试旧工具。</span></div>}
            {task.artifacts.length>0&&<div className="delivery-inline"><span className="delivery-icon"><CheckCircle2 size={20}/></span><div><strong>交付已生成</strong><span>{task.archived?'可预览或导出。':'可预览、编辑或下载。'}</span></div>{task.artifacts.map((a,i)=><button key={a.id} className="secondary-button compact" onClick={()=>setArtifact(a)}><FileText size={14}/>版本 {i+1}<ArrowUpRight size={13}/></button>)}</div>}
            {(!!task.conversation?.length||task.artifacts.length>0||!!task.supplements?.length||!!task.directions?.length)&&<TaskChat onSaveMessage={item=>task&&void saveReply(task,item)} sourceComposer={sourceComposer} onDocument={(id,location)=>void openSource(id,location,task?.sourceInputs||[])} task={task} executorName={executorName(task.provider,executors)} registered={executors.some(e=>e.id===task.provider)} draft={chatDrafts[task.id]||''} busy={busy} desktop={desktop} onChange={text=>setChatDrafts(current=>({...current,[task.id]:text}))} onSend={continueDelivery} onSteer={steerConversation} onStop={()=>void stop(task)} onOpen={setArtifact}/>}
            <details className="archive-evidence" key={task.id}><summary>Agent 与运行记录<span>{task.nodes.length} 个 Agent · {task.events.length} 条事件</span></summary>
            {!taskCapabilities(task).agentHistory&&<div className="inline-notice"><CircleHelp size={16}/>此执行器当前仅展示主 Agent 输出；工具进度见运行记录，子 Agent 历史尚未开放。</div>}<div className="execution-layout"><section className="graph-panel"><div className="panel-heading"><h2><GitBranch size={15}/>Agent 协作</h2><span className="live-label"><span className={`status-dot ${task.status}`}/>已观测关系</span></div><Graph nodes={task.nodes} selected={node?.id||null} onSelect={setNodeId}/><div className="graph-legend"><span><i className="legend-dot running"/>运行</span><span><i className="legend-dot approval"/>等待</span><span><i className="legend-dot completed"/>结束</span><span className="graph-hint">点击 Agent 查看详情</span></div></section>
              <section className="inspector"><div className="panel-heading"><h2>Agent 详情</h2></div>{node?<><div className="inspector-avatar"><Bot size={24}/><span className={`status-dot ${node.status}`}/></div><h3>{node.name}</h3><p className="inspector-role">{node.role} <span>·</span> {node.model}</p><Badge status={node.status}/><div className="inspector-rule"/><span className="inspector-label">当前工作</span><p className="inspector-summary">{node.summary}</p><span className="inspector-label">输出摘要</span><p className="inspector-output">{node.output||(task.provider==='codex'&&node.parentId?(task.archived?'归档前未同步子会话正文。':isTerminal(task.status)?'点击“同步协作”读取子会话正文。':'任务结束后，可同步子会话正文。'):'尚无文本输出。')}</p><div className="inspector-detail-actions">{node.agentPath&&<code>{node.agentPath}</code>}{!!node.detailTurnCount&&<span>子会话历史 · {node.detailTurnCount} 轮输出</span>}{node.detailNotice&&<p>{node.detailNotice}</p>}{node.output&&<button className="secondary-button compact" onClick={()=>setAgentOutput(node)}><FileText size={13}/>{node.outputTruncated?'查看输出':'查看完整输出'}</button>}</div><div className="inspector-foot"><ShieldCheck size={13}/>状态来自执行器事件</div></>:<div className="inspector-empty"><Bot size={26}/><p>启动任务后查看 Agent 详情</p></div>}</section></div>
            {task.approvals.map(a=><div key={a.id} className="approval-card"><span className="approval-icon"><Bell size={19}/></span><div><span className="approval-eyebrow">需要你的判断</span><h3>{a.title}</h3>{!a.questions?.length&&!a.questionError&&<p>{a.description}</p>}<RealApprovalForm approval={a} busy={busy||!desktop||!!task.archived||!taskCapabilities(task).input} onAnswer={answers=>answer(task,a.id,answers)}/></div></div>)}
            <section className="activity-panel"><div className="panel-heading"><h2><Terminal size={15}/>运行记录</h2><span className="subtle">{task.events.length} 条事件</span></div><div className="activity-list">{task.events.slice(-5).reverse().map(event=><div key={event.id} className="activity-row"><span className={`activity-mark ${event.kind}`}/><time>{time(event.at)}</time><strong>{event.agent}</strong><span>{event.text}</span></div>)}{!task.events.length&&<div className="activity-empty">启动任务后，执行事件会记录在这里。</div>}</div>{isActive(task.status)&&taskCapabilities(task).steer&&<form className="steer-form" onSubmit={e=>{e.preventDefault();sendDirection();}}><Command size={15}/><input aria-label="补充任务方向" placeholder="补充方向，让 Agent 继续推进…" value={steer} maxLength={2000} onChange={e=>setSteer(e.target.value)}/><button aria-label="发送补充方向" disabled={!steer.trim()||busy||task.status==='cancelling'}><Send size={15}/></button></form>}</section>
            </details>
          </>:<div className="large-empty"><Bot size={40}/><h2>{view==='archive'?'还没有归档任务':'从一个任务开始'}</h2><p>{view==='archive'?'已交付任务可归档，在这里导出或删除。':desktop?'调研、编程或写作，都可以从这里推进。':'打开 Orbit 桌面 App 后可选择执行器，创建和运行任务。'}</p>{view!=='archive'&&<button className="primary-button" onClick={()=>{setNewTaskSeed(null);setModal(true);}}><Plus size={16}/>新建任务</button>}</div>}</div>
        </div>}

        {view==='attention'&&<AttentionPanel tasks={attention} executors={executors} busy={busy} desktop={desktop} onOpen={target=>{select(target);void navigate('tasks');}} onAnswer={answer}/>}
        {view==='artifacts'&&<section className="collection-panel"><div className="panel-heading"><h2>交付物 <span>{deliveries.length}</span></h2><span className="subtle">任务实际生成的交付</span></div><div className="artifact-grid">{deliveries.map(({task:t,artifact:a})=><button className="artifact-card" key={`${t.id}-${a.id}`} onClick={()=>setArtifact(a)}><span className="artifact-file-icon"><FileText size={26}/></span><span className="provider-label">{executorName(t.provider,executors)} 输出</span><h3>{a.name}</h3><p>{t.title}</p><div><span>{new Date(a.createdAt).toLocaleDateString('zh-CN')}</span><ArrowUpRight size={15}/></div></button>)}</div>{!deliveries.length&&<div className="large-empty"><FolderOpen size={42}/><h2>还没有交付物</h2><p>任务生成的纯文本交付会保存在这里。</p></div>}</section>}
        {view==='knowledge'&&<KnowledgeLibrary ref={library} openLocation={libraryLocation} onNotice={setToast}/>}
        {view==='agents'&&<AgentManager agents={agents} executors={executors} catalogs={modelCatalogs} onLoad={loadModels} onSave={saveProfile} onDelete={removeProfile} busy={busy} desktop={desktop}/>}
        {view==='settings'&&<section className="settings-panel"><div className="settings-title"><span className="metric-icon mint"><Terminal size={21}/></span><div><h2>工作台偏好</h2><p>管理主题、数据位置与本机执行器。</p></div><span className="provider-label">{executors.length} 个执行器</span></div><div className="settings-body"><div className="settings-line"><div><strong>外观</strong><p>选择工作台主题，下次打开时保留。</p></div><div className="theme-picker" role="group" aria-label="主题">{(['light','dark'] as const).map(value=><button key={value} aria-pressed={theme===value} onClick={()=>chooseTheme(value)}>{value==='light'?'Light':'Dark'}</button>)}</div></div><h3 className="settings-section-title">数据与导出</h3><ExportDirectorySettings value={exportSettings} desktop={desktop} busy={busy} onChoose={chooseFolder} onReset={resetFolder} onExport={exportAllData}/><h3 className="settings-section-title">执行器</h3><ExecutorSettings executors={executors} doctors={doctors} checking={checking} desktop={desktop} onCheck={inspectCli} catalogs={modelCatalogs} defaults={modelDefaults} onModelChange={chooseModel} onLoad={loadModels}/>{!desktop&&<div className="inline-notice"><CircleHelp size={16}/>浏览器仅预览界面；打开 Orbit 桌面 App 后可连接本机执行器。</div>}<h3 className="settings-section-title">运行</h3><div className="settings-line"><div><strong>并发与恢复</strong><p>所有执行器共用运行控制；重启保留记录，旧审批失效。</p></div><span className="setting-value">同时最多 3 个运行</span></div><div className="settings-footnote"><CircleHelp size={15}/><p>检查连接只验证初始化，不调用模型。任务运行会消耗对应执行器的额度；功能以本次连接确认的能力为准。</p></div></div></section>}
        <footer className="page-footer"><span><ShieldCheck size={12}/>{!desktop?'桌面 App 中保存任务记录':persistence.current?'记录保存在本地':'当前修改暂存内存'}</span><span>Orbit <span>·</span> 个人 Agent 工作台</span></footer>
      </div>
    </main>
    {teamEditor!==undefined&&<TeamEditor task={teamEditor} agents={agents} busy={busy} desktop={desktop} onClose={()=>setTeamEditor(undefined)} onSave={saveTeam}/>}
    {codeDiff&&<DiffDialog evidence={codeDiff} onClose={()=>setCodeDiff(null)}/>}
    {modal&&<NewTask onTeam={()=>{setModal(false);setTeamEditor(null);}} seed={newTaskSeed} executors={executors} catalogs={modelCatalogs} defaults={modelDefaults} onLoad={loadModels} onClose={()=>setModal(false)} onCreate={create} busy={busy}/>}
    {agentOutput&&<AgentOutput node={agentOutput} onClose={()=>setAgentOutput(null)}/> }
    {artifact&&<ArtifactEditor key={artifact.id} artifact={artifact} sourceInputs={artifactSources} onDocument={(id,location)=>void openSource(id,location,artifactSources)} readOnly={tasks.some(t=>t.archived&&t.artifacts.some(a=>a.id===artifact.id))} onClose={()=>{lastSave.current=null;setArtifact(null);}} onSave={saveDocument} onDownload={download} onCollect={desktop?saveToLibrary:undefined}/>}
    {toast&&<div className="toast" role="status"><CircleHelp size={16}/><span>{toast}</span><button className="icon-button" aria-label="关闭提示" onClick={()=>setToast('')}><X size={14}/></button></div>}
  </div>;
}

function NewTask({onTeam,seed,executors,catalogs,defaults,onLoad,onClose,onCreate,busy}:{onTeam:()=>void;seed?:Task|null;executors:ExecutorDescriptor[];catalogs:Record<string,ModelCatalogState>;defaults:Record<string,string|null>;onLoad:(id:string,refresh?:boolean)=>void;onClose:()=>void;onCreate:(title:string,prompt:string,provider:string,model:string|null,sources:SourceRequest[])=>void;busy:boolean}) {
  const [title,setTitle]=useState(seed?`${seed.title.slice(0,96)} 副本`:''),[prompt,setPrompt]=useState(seed?.prompt||''),[provider,setProvider]=useState(executors.some(e=>e.id===seed?.provider)?seed!.provider:executors[0]?.id||'');
  const [choices,setChoices]=useState<Record<string,string|null>>(()=>seed?{...defaults,[seed.provider]:seed.requestedModel??null}:{...defaults});
  const [sources,setSources]=useState<SourceSelection[]>([]);
  const model=choices[provider]??null,available=modelAvailable(model,catalogs[provider]?.models);
  useEffect(()=>{if(!provider&&executors.length)setProvider(executors[0].id);},[executors,provider]);
  return <div className="modal-backdrop" onClick={onClose}><form className="new-task-modal" role="dialog" aria-modal="true" aria-label="新建任务" onClick={e=>e.stopPropagation()} onSubmit={e=>{e.preventDefault();if(desktop&&title.trim()&&prompt.trim()&&available)onCreate(title,prompt,provider,model,sources.map(s=>s.request));}}><div className="modal-heading"><div><span className="modal-spark"><Sparkles size={20}/></span><h2>新建任务</h2></div><button type="button" className="icon-button" aria-label="关闭新建任务" onClick={onClose}><X size={20}/></button></div><div className="modal-body"><button type="button" className="chat-document-link" onClick={onTeam}><Bot size={14}/>创建团队任务：计划 → 并行分工 → 独立评审</button><p className="modal-description">描述你想达成的结果，让 Agent 帮你向前推进。</p><label>任务名称<input autoFocus placeholder="例如：比较个人知识库工具" value={title} maxLength={100} onChange={e=>setTitle(e.target.value)} required/></label><label>目标与交付<textarea placeholder="你需要什么结果？有什么范围或限制？" value={prompt} maxLength={12000} onChange={e=>setPrompt(e.target.value)} required rows={4}/></label><SourceComposer value={sources} onChange={setSources} disabled={busy}/><ExecutorPicker executors={executors} value={provider} onChange={setProvider} model={model} onModelChange={id=>setChoices(current=>({...current,[provider]:id}))} catalog={catalogs[provider]} desktop={desktop} onLoad={onLoad}/><div className="new-task-note"><ShieldCheck size={14}/>{desktop?executors.find(e=>e.id===provider)?.permissionNote:'需要打开 Orbit 桌面 App 运行任务'}</div><div className="new-task-note">创建后仍需点击启动，已有任务始终使用创建时选择的执行器与模型。</div></div><div className="modal-footer"><button type="button" className="secondary-button" onClick={onClose}>取消</button><button className="primary-button" disabled={!desktop||busy||!title.trim()||!prompt.trim()||!executors.some(e=>e.id===provider)||!available}>创建任务</button></div></form></div>;
}
