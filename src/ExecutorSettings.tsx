import {useEffect,useState} from 'react';
import type {ExecutorDescriptor,ModelCatalogState} from './executors';
import type {Doctor} from './bridge';
import {ExecutorPicker} from './ExecutorPicker';
export function ExecutorSettings({executors,doctors,checking,desktop,onCheck,catalogs,defaults,onModelChange,onLoad}:{executors:ExecutorDescriptor[];doctors:Record<string,Doctor>;checking:string|null;desktop:boolean;onCheck:(id:string)=>void;catalogs:Record<string,ModelCatalogState>;defaults:Record<string,string|null>;onModelChange:(provider:string,model:string|null)=>void;onLoad:(id:string,refresh?:boolean)=>void}) {
  const [provider,setProvider]=useState(executors[0]?.id||'');
  useEffect(()=>{if(!executors.some(e=>e.id===provider)&&executors.length)setProvider(executors[0].id);},[executors,provider]);
  const executor=executors.find(e=>e.id===provider),doctor=doctors[provider];
  const capabilities=doctor?.initialized?doctor.capabilities:executor?.capabilities;
  return <div className="executor-settings"><ExecutorPicker executors={executors} value={provider} onChange={setProvider} model={defaults[provider]??null} onModelChange={model=>onModelChange(provider,model)} catalog={catalogs[provider]} desktop={desktop} onLoad={onLoad} doctor={doctor} onCheck={()=>onCheck(provider)} checking={checking!==null}/>
    <p className="executor-default-note">模型选择作为 {executor?.name||'此执行器'} 新任务的默认值，已有任务保持原选择。</p>
    {doctor&&<p className="executor-permissions" role="status">{doctor.message}</p>}
    {executor&&<details className="executor-access-details"><summary>权限与能力</summary><p className="executor-permissions">{executor.permissionNote}</p><div className="executor-capabilities"><span>续交付：{capabilities?.resume?'支持':'待连接确认'}</span><span>运行中补充：{capabilities?.steer?'支持':'当前未开放'}</span><span>子 Agent 历史：{capabilities?.agentHistory?'支持':'当前未开放'}</span></div></details>}
  </div>;
}
