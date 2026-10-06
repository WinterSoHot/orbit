// Full App with memory-only tasks; unsupported mutations never reach a real executor/Store.
import {createRoot} from 'react-dom/client';
import {previewExecutors} from '../src/executors';
import {applyTheme,initialTheme} from '../src/theme';
import type {Task} from '../src/model';
import '../src/styles.css';
applyTheme(initialTheme());
if(new URLSearchParams(location.search).has('light'))applyTheme('light');
const at=Date.UTC(2026,9,6),artifact={id:'artifact',name:'个人知识库调研.md',kind:'markdown' as const,createdAt:at,content:'# 调研交付\n\n'+('本地阅读与资料管理支持离线使用；模型能力按执行器选择。\n\n'.repeat(45))};
const base:Task={id:'archived',title:'个人 AI 知识库工作台研究与功能设计',prompt:'整理本地资料管理和多 Agent 工作流程。',scene:'research',provider:'codex',status:'completed',archived:true,createdAt:at,startedAt:at,finishedAt:at+20000,runId:'run',threadId:'thread',turnId:'turn',revision:1,phase:1,nodes:[{id:'lead',name:'Codex',role:'主 Agent',model:'模型',status:'completed',summary:'已整理结果',output:'Agent 输出。\n'.repeat(100),parentId:null}],events:[{id:'event',at,kind:'system',agent:'工作台',text:'交付已保存'}],approvals:[],artifacts:[artifact],tokens:null};
const empty=new URLSearchParams(location.search).has('empty');
const tasks=empty?[]:[base,...Array.from({length:18},(_,i)=>({...base,id:'archive-'+i,title:'归档资料研究 '+(i+1)})),{...base,id:'needs',archived:false,status:'approval' as const,title:'等待确认本地与云端的处理范围',approvals:[{id:'request',requestId:'request',runId:'run',turnId:'turn',title:'请确认第一版范围',description:'核心功能完全本地运行，是否允许按需连接云端模型？',kind:'input',questionIds:['scope']}],artifacts:[]},{...base,id:'delivery',archived:false,title:'个人资料管理工具能力对比',artifacts:[artifact,{...artifact,id:'v2',name:'很长的交付文件名称与个人资料管理使用流程说明.md'}]}];
let callbackId=0;
const workspaceTasks=new URLSearchParams(location.search).has('review')?tasks.map(t=>t.id==='needs'?{...t,provider:'qoder',status:'unknown' as const,title:'知识库调研',approvals:[]}:t):tasks;
const displayedTasks=new URLSearchParams(location.search).has('clarify')?workspaceTasks.map(t=>({...t,approvals:t.approvals.map(a=>({...a,questions:[{id:'scope',header:'处理范围',question:'本地优先的边界更接近哪一种？',isOther:true,options:[{label:'核心本地，AI 可选云端（默认关闭）',description:'阅读、编辑与检索在本地完成，模型按需连接。'},{label:'所有处理必须离线完成',description:'只使用本机模型与工具。'},{label:'数据本地，默认使用云端 AI',description:'资料保留本地，任务默认交给云端模型。'}]}]}))})):workspaceTasks;
Object.assign(window,{isTauri:true,__TAURI_INTERNALS__:{transformCallback:()=>++callbackId,invoke:async(command:string)=>{
 if(command==='plugin:event|listen')return ++callbackId;
 if(command==='plugin:event|unlisten')return;
 if(command==='load_workspace')return {tasks:structuredClone(displayedTasks),error:null};
 if(command==='list_executors')return previewExecutors;
 if(command==='load_queue_state')return {paused:false,reason:null};
 if(command==='get_export_settings')return {directory:'/本地资料/研究项目/知识库/交付导出目录/'.repeat(6),custom:true};
 if(command==='load_library')return {schemaVersion:3,error:null,documents:[],groups:[],collections:[]};
 throw Error('Fixture blocked mutation: '+command);
}}});
const {default:App}=await import('../src/App');
createRoot(document.getElementById('root')!).render(<App/>);
