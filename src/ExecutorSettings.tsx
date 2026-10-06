import { CheckCircle2, Loader2, Terminal, Zap } from 'lucide-react';
import type { ExecutorDescriptor } from './executors';
import type { Doctor } from './bridge';
export function ExecutorSettings({executors,doctors,checking,desktop,onCheck}:{executors:ExecutorDescriptor[];doctors:Record<string,Doctor>;checking:string|null;desktop:boolean;onCheck:(id:string)=>void}) {
  return <div className="executor-settings">{executors.map(executor=>{
    const doctor=doctors[executor.id];
    const capabilities=doctor?.initialized?doctor.capabilities:executor.capabilities;
    return <section className="executor-setting" key={executor.id} aria-label={`${executor.name} 执行器设置`}>
      <div className="settings-line"><div className="executor-setting-title"><Terminal size={19}/><div><strong>{executor.name}</strong><p>{executor.description}</p></div></div><button className="secondary-button" disabled={!desktop||checking!==null} onClick={()=>onCheck(executor.id)}>{checking===executor.id?<Loader2 className="spin" size={15}/>:<Zap size={15}/>}检查连接</button></div>
      {doctor&&<div className={`doctor-result ${doctor.initialized?'success':''}`} role="status"><CheckCircle2 size={18}/><div><strong>{doctor.message}</strong><p>{doctor.version||'版本尚未确认'}</p><code>{doctor.path}</code></div></div>}
      <p className="executor-permissions">{executor.permissionNote}</p>
      <div className="executor-capabilities"><span>运行结束后续交付：{capabilities.resume?'支持':'待连接确认或不支持'}</span><span>运行中补充：{capabilities.steer?'支持':'当前未开放'}</span><span>子 Agent 历史：{capabilities.agentHistory?'支持':'当前未开放'}</span></div>
    </section>;
  })}</div>;
}
