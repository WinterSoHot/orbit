// Memory-only input requests: no Store, CLI or model invocation.
import {useState} from 'react';
import {createRoot} from 'react-dom/client';
import {RealApprovalForm} from '../src/ApprovalForm';
import {TaskChat} from '../src/TaskChat';
import type {Approval,Task} from '../src/model';
import {applyTheme,initialTheme} from '../src/theme';
import '../src/styles.css';
import '../src/workspace.css';
applyTheme(initialTheme());
const question='本地优先的边界更接近哪一种？';
const approval:Approval={id:'input',requestId:'42',runId:'run',turnId:'turn',title:'需要你的答复',description:'',kind:'input',questionIds:['scope','goal'],questions:[{id:'scope',header:'处理范围',question,isOther:true,options:[{label:'核心本地，AI 可选云端（默认关闭）',description:'阅读、编辑与检索在本地完成，模型按需连接。'},{label:'所有处理必须离线完成',description:'只使用本机模型与工具。'},{label:'数据本地，默认使用云端 AI',description:'资料保留本地，任务默认交给云端模型。'}]},{id:'goal',header:'补充约束',question:'还有哪些约束需要补充？',options:null}]};
const task:Task={id:'task',title:'个人知识库设计',prompt:'设计一个本地优先的个人知识库。',provider:'codex',scene:'research',status:'approval',createdAt:0,startedAt:1,finishedAt:null,runId:'run',turnId:'turn',threadId:'thread',revision:1,phase:0,nodes:[],events:[],approvals:[approval],artifacts:[],tokens:null};
function Preview(){
 const [request,setRequest]=useState(0),[busy,setBusy]=useState(false),[handled,setHandled]=useState(false),[payload,setPayload]=useState(''),[draft,setDraft]=useState('');
 const reserved=new URLSearchParams(location.search).has('reserved');
 const current={...approval,id:`input-${request}`,questionIds:reserved?['constructor','__proto__']:approval.questionIds,questions:reserved?approval.questions!.map((q,i)=>({...q,id:i?'__proto__':'constructor'})):approval.questions};
 return <main style={{maxWidth:780,margin:'24px auto',padding:'0 20px'}}><h1 style={{fontSize:20}}>澄清卡片 · 隔离检查</h1><div style={{display:'flex',gap:12,margin:'16px 0'}}><button className="secondary-button" onClick={()=>{setRequest(n=>n+1);setHandled(false);setPayload('');}}>切换请求</button><button className="secondary-button" onClick={()=>setBusy(v=>!v)}>切换忙碌</button><button className="secondary-button" onClick={()=>applyTheme(document.documentElement.dataset.orbitTheme==='dark'?'light':'dark')}>切换主题</button></div><TaskChat task={{...task,status:handled?'running':'approval'}} draft={draft} busy={busy} desktop executorName="Codex" onChange={setDraft} onSend={()=>{}} onOpen={()=>{}}>{!handled&&<div className="chat-approval"><strong>{current.title}</strong><RealApprovalForm approval={current} busy={busy} onAnswer={answers=>{setPayload(JSON.stringify(answers));setHandled(true);}}/></div>}</TaskChat><output aria-label="提交的答复" style={{display:'block',marginTop:16,overflowWrap:'anywhere'}}>{payload}</output></main>;
}
createRoot(document.getElementById('root')!).render(<Preview/>);
