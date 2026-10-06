import type { ExecutorDescriptor } from './executors';
export function ExecutorPicker({executors,value,onChange}:{executors:ExecutorDescriptor[];value:string;onChange:(id:string)=>void}) {
  return <fieldset className="executor-picker"><legend>执行器</legend>{executors.map(executor=><label className={`executor-choice ${executor.id===value?'selected':''}`} key={executor.id}>
    <input type="radio" name="executor" value={executor.id} checked={executor.id===value} onChange={()=>onChange(executor.id)}/>
    <span><strong>{executor.name}</strong><small>{executor.description}</small></span>
  </label>)}{!executors.length&&<p className="subtle">执行器目录尚未加载，请稍后重试。</p>}</fieldset>;
}
