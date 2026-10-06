import type { ExecutorModel } from './executors';
export const modelDefaultsKey='orbit.executorModels';
export function readModelDefaults(storage?:Pick<Storage,'getItem'>):Record<string,string|null> {
  try {
    const data:unknown=JSON.parse(storage?.getItem(modelDefaultsKey)||'{}');
    if(!data||typeof data!=='object'||Array.isArray(data))return {};
    return Object.fromEntries(Object.entries(data).filter(([id,value])=>/^[a-z][a-z0-9_-]{0,63}$/.test(id)&&(value===null||typeof value==='string'&&value.length>0&&value.length<=256&&!/\s/.test(value))));
  } catch {return {};}
}
export function modelAvailable(value:string|null,models?:ExecutorModel[]):boolean {
  return value===null||!!models?.some(model=>model.id===value);
}
