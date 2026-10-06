import {useEffect,useRef,useState} from 'react';
import {MessageCircle,Send,Loader2,FileText,Square,ArrowDown,Check,Terminal,AlertCircle} from 'lucide-react';
import {canContinue,isActive,taskCapabilities} from './model';
import type {Artifact,Task} from './model';
import type {ReactNode} from 'react';
import {MessageBody} from './MessageBody';
import {chatTimeline,followsLatest,shouldSendKey} from './chatTimeline';

type Props={compact?:boolean;task:Task;draft:string;busy:boolean;desktop:boolean;executorName?:string;registered?:boolean;onChange:(text:string)=>void;onSend:()=>void;onOpen:(artifact:Artifact)=>void;onSteer?:()=>void;onStop?:()=>void;children?:ReactNode};
export function TaskChat({compact=false,task,draft,busy,desktop,executorName=task.provider,registered=true,onChange,onSend,onOpen,onSteer,onStop,children}:Props){
  const historyRef=useRef<HTMLDivElement>(null),following=useRef(true),[newOutput,setNewOutput]=useState(false);
  const supplements=task.supplements||[],history=chatTimeline(task);
  const steering=!!onSteer&&!task.archived&&!task.queue&&isActive(task.status)&&task.status!=='cancelling'&&taskCapabilities(task).steer;
  const available=registered&&(canContinue(task)||steering);
  const currentMessages=task.conversation?.some(item=>item.runId===task.runId&&item.kind==='assistant'&&!!item.text);
  const liveOutput=isActive(task.status)&&!currentMessages?task.nodes.find(node=>!node.parentId)?.output:undefined;
  const failure=task.status==='failed'?task.events.filter(event=>event.kind==='error').at(-1)?.text:undefined;
  const retry=task.status==='failed'&&!task.turnId?supplements.at(-1):undefined;
  const limit=task.artifacts.length>=10||supplements.length>=10;
  const canStop=!!onStop&&!task.archived&&isActive(task.status)&&taskCapabilities(task).interrupt;
  const notice=task.archived?'已归档 · 对话只读':!registered?'该执行器尚未接入，历史交付可预览和导出':task.queue?'已排队，等待执行 · 草稿已保留':task.status==='approval'?'请先答复上方的问题，也可以补充方向':steering?'可以随时补充方向':task.status==='queued'?'点击启动开始对话':isActive(task.status)?'草稿已保留，本轮结束后可发送':limit?'已达到交付或补充上限，请新建任务':available?'继续对话，补充或调整要求':'当前会话无法继续，请检查连接或新建任务';
  const outputVersion=history.map(entry=>`${entry.id}:${entry.text}:${entry.item?.status}:${entry.item?.exitCode}`).join('\u0000')+(liveOutput||'')+task.status+task.approvals.map(a=>a.id).join(':');
  function latest(){const element=historyRef.current;if(element){element.scrollTop=element.scrollHeight;following.current=true;setNewOutput(false);}}
  useEffect(()=>{following.current=true;setNewOutput(false);latest();},[task.id]);
  useEffect(()=>{if(following.current)latest();else setNewOutput(true);},[outputVersion]);
  function submit(){if(available&&!busy&&desktop&&draft.trim())(steering?onSteer!:onSend)();}
  return <section className="task-chat" aria-label="任务对话">
    {!compact&&<div className="panel-heading"><h2><MessageCircle size={15}/>任务对话</h2><span className="subtle">{task.artifacts.length} 份交付版本</span></div>}
    <div className="chat-history" ref={historyRef} onScroll={()=>{const el=historyRef.current;if(el){following.current=followsLatest(el.scrollTop,el.scrollHeight,el.clientHeight);if(following.current)setNewOutput(false);}}}>
      <div className="chat-message user"><span className="chat-label">你 · 初始目标</span><MessageBody key={task.id} content={task.prompt}/></div>
      {history.map(entry=>entry.item?entry.item.kind==='tool'?<details className="chat-tool" key={entry.id}><summary>{entry.item.status==='running'&&isActive(task.status)?<Loader2 className="spin" size={14}/>:entry.item.status==='completed'?<Check size={14}/>:<AlertCircle size={14}/>}<span>{entry.item.title}</span><small>{entry.item.status==='running'&&!isActive(task.status)?'状态需核对':({running:'执行中',completed:'已完成',failed:'失败',unknown:'需核对'})[entry.item.status]}</small></summary><div><Terminal size={14}/><span>{executorName}{entry.item.exitCode!==null&&` · 退出码 ${entry.item.exitCode}`}<small>仅显示执行状态；详细记录在任务详情中查看。</small></span></div></details>:<div className="chat-message assistant" key={entry.id}><span className="chat-label">{executorName}{entry.item.finalAnswer?' · 回复':''}</span>{entry.text&&<MessageBody content={entry.text} markdown/>}{entry.item.truncated&&<small className="chat-truncated">此消息超过展示上限，完整交付请打开文档。</small>}</div>:entry.artifact?<div className={`chat-message delivery ${entry.documentOnly?'document-only':''}`} key={entry.id}>
        {entry.documentOnly?<div className="chat-document"><FileText size={16}/><span><strong>交付版本 {task.artifacts.findIndex(a=>a.id===entry.artifact!.id)+1}</strong><small>{entry.artifact.name}</small></span><button className="chat-document-link" onClick={()=>onOpen(entry.artifact!)}>打开</button></div>:<><details open={entry.artifact.id===task.artifacts.at(-1)?.id}><summary><FileText size={14}/><strong>交付版本 {task.artifacts.findIndex(a=>a.id===entry.artifact!.id)+1}</strong><time>{new Date(entry.at).toLocaleString('zh-CN',{month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'})}</time></summary><MessageBody content={entry.artifact.content} markdown/></details><button className="chat-document-link" aria-label={`打开 ${entry.artifact.name}`} onClick={()=>onOpen(entry.artifact!)}>打开</button></>}
      </div>:<div className="chat-message user" key={entry.id}><span className="chat-label">你 · 补充信息</span><MessageBody content={entry.text}/>{entry.status&&<small className="chat-label direction-status">{entry.status}</small>}</div>)}
      {task.queue?.action.kind==='continue'&&<div className="chat-message user"><span className="chat-label">你 · 待执行补充</span><MessageBody key={task.queue.requestId} content={task.queue.action.text}/><small>已排队 · 尚未执行</small></div>}
      {liveOutput&&<div className="chat-message assistant"><span className="chat-label">{executorName} · 当前输出</span><MessageBody key={task.runId} content={liveOutput} markdown/></div>}
      {children}
      {task.conversationTruncated&&<p className="chat-attempt-notice">对话记录达到容量上限，部分消息未收录；交付文档仍可打开。</p>}
      {isActive(task.status)&&<div className="chat-progress" role="status">{task.status==='approval'?<MessageCircle size={14}/>:<Loader2 className="spin" size={14}/>} {task.status==='approval'?'等待你的答复':task.status==='cancelling'?'正在停止…':`${executorName} 正在处理`}</div>}
      {(task.status==='failed'||task.status==='interrupted'||task.status==='unknown')&&<p className="chat-attempt-notice">本轮{task.status==='failed'?'失败':task.status==='interrupted'?'已中断':'结果需核对'}，{task.artifacts.length?'已有交付版本保留':'对话记录已保留'}。补充记录表示已提交给工作台，不代表模型已成功处理。{failure&&<span className="chat-failure-detail">{failure}</span>}</p>}
    </div>
    {newOutput&&<button className="chat-latest" onClick={latest}><ArrowDown size={14}/>返回最新消息</button>}
    <div className="chat-composer-area"><p className="chat-composer-notice">{notice}</p>{retry&&available&&!task.archived&&<button type="button" className="chat-document-link" disabled={busy||!desktop} onClick={()=>onChange(retry.text)}>使用上次补充内容</button>}
      {!task.archived&&<form className="chat-composer" onSubmit={event=>{event.preventDefault();submit();}}>
        <textarea aria-label="补充信息继续交付" placeholder={steering?'补充方向…':'继续对话…'} rows={3} maxLength={2000} value={draft} disabled={busy||!desktop||!!task.queue} onChange={event=>onChange(event.target.value)} onKeyDown={event=>{if(shouldSendKey({key:event.key,ctrlKey:event.ctrlKey,metaKey:event.metaKey,isComposing:event.nativeEvent.isComposing})){event.preventDefault();submit();}}}/>
        <div><span>{draft.length}/2000 · ⌘ / Ctrl Enter 发送</span><div className="chat-send-actions">{canStop&&<button type="button" className="chat-stop" aria-label="停止生成" title="停止生成" disabled={busy||!desktop||task.status==='cancelling'} onClick={onStop}><Square size={14}/>{task.status==='cancelling'?'停止中':'停止'}</button>}<button className="primary-button compact" disabled={busy||!desktop||!available||!draft.trim()}>{busy?<Loader2 size={14} className="spin"/>:<Send size={14}/>}{steering?'发送补充方向':'发送并继续交付'}</button></div></div>
      </form>}
    </div>
  </section>;
}
