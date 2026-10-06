import { useEffect, useRef } from 'react';
import { MessageCircle, Send, Loader2, FileText } from 'lucide-react';
import { canContinue, isActive, taskCapabilities } from './model';
import type { Artifact, Task } from './model';
import type { ReactNode } from 'react';
import { MessageBody } from './MessageBody';

type Props={compact?:boolean;task:Task;draft:string;busy:boolean;desktop:boolean;executorName?:string;registered?:boolean;onChange:(text:string)=>void;onSend:()=>void;onOpen:(artifact:Artifact)=>void;onSteer?:()=>void;children?:ReactNode};
export function TaskChat({compact=false,task,draft,busy,desktop,executorName=task.provider,registered=true,onChange,onSend,onOpen,onSteer,children}:Props) {
  const historyRef=useRef<HTMLDivElement>(null);
  const supplements=task.supplements||[];
  useEffect(()=>{const history=historyRef.current;if(history)history.scrollTop=history.scrollHeight;},[task.id,task.artifacts.length,supplements.length,task.directions?.length]);
  const history=[...supplements.map(s=>({id:s.runId,at:s.createdAt,text:s.text,artifact:null as Artifact|null,status:s.sourceThreadId?'原会话被占用 · 已复制历史到续接分支':''})),...(task.directions||[]).map(d=>({id:d.id,at:d.createdAt,text:d.text,artifact:null as Artifact|null,status:({pending:'等待执行器确认',accepted:'执行器已确认 · 请在交付中核对采用情况',rejected:'执行器已拒绝',unknown:'确认未知 · 请勿重复发送'})[d.status]})),...task.artifacts.map(a=>({id:a.id,at:a.createdAt,text:'',artifact:a,status:''}))].sort((a,b)=>a.at-b.at);
  const steering=!!onSteer && !task.archived && !task.queue && isActive(task.status) && task.status!=='cancelling' && taskCapabilities(task).steer;
  const available=registered&&(canContinue(task)||steering);
  const liveOutput=isActive(task.status)?task.nodes.find(node=>!node.parentId)?.output:undefined;
  const failure=task.status==='failed'?task.events.filter(event=>event.kind==='error').at(-1)?.text:undefined;
  const retry=task.status==='failed'&&!task.turnId?supplements.at(-1):undefined;
  const limit=task.artifacts.length>=10||supplements.length>=10;
  const notice=task.archived?'已归档 · 对话只读':!registered?'该执行器尚未接入，历史交付可预览和导出':task.queue?'请求在等待队列中，草稿已保留':steering?'补充方向会提交给当前执行器，采用情况以交付为准':task.status==='queued'?'目标已记录，点击启动开始任务':isActive(task.status)?'此执行器暂不支持运行中补充，草稿可保留到本轮结束':limit?'已达到 10 份交付或 10 次补充上限，请新建任务':available?'补充背景、调整要求，继续在原会话中交付':'当前执行器未确认可恢复此会话，请检查连接或新建任务';
  return <section className="task-chat" aria-label="任务对话">
    {!compact&&<div className="panel-heading"><h2><MessageCircle size={15}/>任务对话</h2><span className="subtle">{task.artifacts.length} 份交付版本</span></div>}
    <div className="chat-history" ref={historyRef}>
      <div className="chat-message user"><span className="chat-label">你 · 初始目标</span><MessageBody key={task.id} content={task.prompt}/></div>
      {history.map(item=>item.artifact?<div className="chat-message delivery" key={item.id}>
        <details open={item.id===task.artifacts.at(-1)?.id}><summary><FileText size={14}/><strong>交付版本 {task.artifacts.findIndex(a=>a.id===item.id)+1}</strong><time>{new Date(item.at).toLocaleString('zh-CN',{month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'})}</time></summary><MessageBody content={item.artifact.content} markdown/></details>
        <button className="chat-document-link" aria-label={`打开 ${item.artifact.name}`} onClick={()=>onOpen(item.artifact!)}>打开</button>
      </div>:<div key={item.id} className="chat-message user"><span className="chat-label">你 · 补充信息</span><MessageBody content={item.text}/>{item.status&&<small className="chat-label direction-status">{item.status}</small>}</div>)}
      {task.queue?.action.kind==='continue'&&<div className="chat-message user"><span className="chat-label">你 · 待执行补充</span><MessageBody key={task.queue.requestId} content={task.queue.action.text}/><small>已排队 · 尚未执行</small></div>}
      {liveOutput&&<div className="chat-message delivery"><span className="chat-label">{executorName} · 当前输出</span><MessageBody key={task.runId} content={liveOutput} markdown/></div>}
      {children}
      {isActive(task.status)&&<div className="chat-progress" role="status">{task.status==='approval'?<MessageCircle size={14}/>:<Loader2 className="spin" size={14}/>} {task.status==='approval'?'等待你的答复':`${executorName} 正在处理，交付生成后会出现在这里`}</div>}
      {(task.status==='failed'||task.status==='interrupted'||task.status==='unknown')&&<p className="chat-attempt-notice">本轮{task.status==='failed'?'失败':task.status==='interrupted'?'已中断':'结果需核对'}，已有交付版本保留。补充记录表示已提交给工作台，不代表模型已成功处理。{failure&&<span className="chat-failure-detail">{failure}</span>}</p>}
    </div>
    <div className="chat-composer-area"><p className="chat-composer-notice">{notice}</p>{retry&&available&&!task.archived&&<button type="button" className="chat-document-link" disabled={busy||!desktop} onClick={()=>onChange(retry.text)}>使用上次补充内容</button>}
      {!task.archived&&<form className="chat-composer" onSubmit={event=>{event.preventDefault();if(available&&!busy&&desktop&&draft.trim())(steering?onSteer!:onSend)();}}>
        <textarea aria-label="补充信息继续交付" placeholder="例如：预算控制在 500 元以内，补充比较表，并更新结论…" rows={3} maxLength={2000} value={draft} disabled={busy||!desktop||!!task.queue} onChange={event=>onChange(event.target.value)}/>
        <div><span>{supplements.length}/10 次补充 · {draft.length}/2000 字</span><button className="primary-button compact" disabled={busy||!desktop||!available||!draft.trim()}>{busy?<Loader2 size={14} className="spin"/>:<Send size={14}/>}{steering?'发送补充方向':'发送并继续交付'}</button></div>
      </form>}
    </div>
  </section>;
}
