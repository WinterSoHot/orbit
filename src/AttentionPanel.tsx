import {teamPhaseLabel} from "./teams";
import {ArrowUpRight,Bell,CheckCircle2} from 'lucide-react';
import {executorName} from './executors';
import type {ExecutorDescriptor} from './executors';
import {currentDeliveryIds,statusLabels,taskCapabilities} from './model';
import type {Task} from './model';
import {RealApprovalForm} from './ApprovalForm';

export function AttentionPanel({tasks,executors,busy,desktop,onOpen,onAnswer}:{tasks:Task[];executors:ExecutorDescriptor[];busy:boolean;desktop:boolean;onOpen:(task:Task)=>void;onAnswer:(task:Task,id:string,answers:Record<string,string>)=>void}) {
  return <section className="collection-panel"><div className="panel-heading"><h2>待介入任务 <span>{tasks.length}</span></h2><span className="subtle">答复、核对或验收</span></div>{tasks.length?tasks.map(task=>{
    const state=task.team?teamPhaseLabel(task):task.queue?.state==='claimed'?'启动待核对':task.queue?.error?'排队需核对':task.status==='completed'&&currentDeliveryIds(task).length?'交付待验收':statusLabels[task.status];
    return <article className="attention-item" key={task.id}><div className="attention-item-icon"><Bell size={20}/></div><div className="attention-item-content"><span className="provider-label">{executorName(task.provider,executors)}</span><div className="attention-task-heading"><h3>{task.title}</h3><button className="secondary-button compact" onClick={()=>onOpen(task)}>打开任务<ArrowUpRight size={13}/></button></div><p>{state} · {task.approvals.length?'请处理以下请求。':'打开任务查看交付与运行记录，继续处理。'}</p>{task.approvals.map(approval=><div className="attention-request" key={approval.id}><h4>{approval.title}</h4>{!approval.questions?.length&&!approval.questionError&&<p>{approval.description}</p>}<RealApprovalForm approval={approval} busy={busy||!desktop||!taskCapabilities(task).input} onAnswer={answers=>onAnswer(task,approval.id,answers)}/></div>)}</div></article>;
  }):<div className="large-empty"><CheckCircle2 size={42}/><h2>暂时不需要你介入</h2><p>需要答复、核对或验收的任务会自动汇集到这里。</p></div>}</section>;
}
