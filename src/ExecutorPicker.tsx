import {useEffect} from 'react';
import {Check, Cpu, Loader2, RefreshCw, Terminal} from 'lucide-react';
import type {ExecutorDescriptor, ModelCatalogState} from './executors';
import type {Doctor} from './bridge';
import {modelAvailable} from './modelSelection';

export function ExecutorPicker({executors,value,onChange,model,onModelChange,catalog,desktop,onLoad,doctor,onCheck,checking=false}:{
  executors:ExecutorDescriptor[];value:string;onChange:(id:string)=>void;
  model:string|null;onModelChange:(id:string|null)=>void;catalog?:ModelCatalogState;desktop:boolean;
  onLoad:(id:string,refresh?:boolean)=>void;doctor?:Doctor;onCheck?:()=>void;checking?:boolean;
}) {
  const executor=executors.find(item=>item.id===value);
  useEffect(()=>{if(value)onLoad(value);},[value,onLoad]);
  return <section className="executor-browser" aria-label="执行器与模型">
    <div className="executor-tabs" role="group" aria-label="执行器提供者">
      {executors.map(item=><button type="button" key={item.id} aria-pressed={item.id===value} onClick={()=>onChange(item.id)}><Terminal size={16}/>{item.name}</button>)}
    </div>
    {executor?<div className="executor-provider">
      <header className="executor-provider-header"><div className="executor-provider-icon"><Terminal size={22}/></div><div className="executor-provider-info"><strong>{executor.name}</strong><p>{executor.description}</p><span className="executor-connection"><span className={`status-dot ${doctor?.initialized?'running':'unknown'}`}/>{!desktop?'浏览器预览':doctor?.initialized?'已连接本机 CLI':doctor?.available?'待检查连接':'本机 CLI'}{doctor?.version&&<small>{doctor.version}</small>}</span></div><div className="executor-provider-actions">{onCheck&&<button type="button" className="icon-button" disabled={!desktop||checking} title="检查连接" aria-label={`检查 ${executor.name} 连接`} onClick={onCheck}>{checking?<Loader2 size={16} className="spin"/>:<Terminal size={16}/>}</button>}<button type="button" className="icon-button" title="刷新模型" aria-label={`刷新 ${executor.name} 模型`} disabled={!desktop||catalog?.loading} onClick={()=>onLoad(value,true)}><RefreshCw size={16} className={catalog?.loading?'spin':''}/></button></div></header>
      <fieldset className="executor-model-list"><legend className="sr-only">{executor.name} 模型</legend>
        <ModelRow id="" name="CLI 默认" description="使用执行器当前默认模型" selected={model===null} onChange={()=>onModelChange(null)}/>
        {catalog?.models?.map(item=><ModelRow key={item.id} id={item.id} name={item.name} description={item.description} selected={model===item.id} recommended={item.isDefault} onChange={()=>onModelChange(item.id)}/>)}
      </fieldset>
      {model&&!modelAvailable(model,catalog?.models)&&<p className="executor-model-error" role="alert">当前选择：{model}。{catalog?.loading?'正在核对可用性…':'目录中未找到此模型，请刷新或重新选择。'}</p>}
      <div className="executor-catalog-status" role="status">{catalog?.loading?<><Loader2 size={13} className="spin"/>正在读取模型…</>:catalog?.error?<span className="executor-model-error">{catalog.error}</span>:!desktop?'在桌面 App 中读取本机可用模型':catalog?.models?`${catalog.models.length} 个可选模型`:'等待读取模型目录'}</div>
    </div>:<p className="subtle">执行器目录尚未加载。</p>}
  </section>;
}
function ModelRow({id,name,description,selected,recommended,onChange}:{id:string;name:string;description:string;selected:boolean;recommended?:boolean;onChange:()=>void}) {
  return <label className={`executor-model-row ${selected?'selected':''}`}><input type="radio" name="executor-model" value={id} checked={selected} onChange={onChange}/><span className="executor-model-check">{selected?<Check size={16}/>:<Cpu size={16}/>}</span><span className="executor-model-name"><strong>{name}{recommended&&<small>推荐</small>}</strong>{description&&<span>{description}</span>}</span><code>{id||'default'}</code></label>;
}
