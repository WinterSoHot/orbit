import type {Artifact,ChatItem,Task} from './model';
export type ChatEntry={id:string;at:number;text:string;status?:string;artifact?:Artifact;item?:ChatItem;documentOnly?:boolean};
export function chatTimeline(task:Task):ChatEntry[]{
  const messages=task.conversation||[],supplementRuns=(task.supplements||[]).map(s=>s.runId);
  const initialRuns=messages.map(m=>m.runId).filter(run=>!supplementRuns.includes(run));
  const runs=messages.length?[...new Set([...initialRuns,...supplementRuns])]:[];
  const inputs=(task.supplements||[]).map(s=>({runId:s.runId,entry:{id:`input:${s.runId}`,at:s.createdAt,text:s.text,status:s.sourceThreadId?'原会话被占用 · 已复制历史到续接分支':''}}));
  const directions=(task.directions||[]).map(d=>({runId:d.runId,entry:{id:`direction:${d.id}`,at:d.createdAt,text:d.text,status:({pending:'等待执行器确认',accepted:'执行器已确认 · 请在交付中核对采用情况',rejected:'执行器已拒绝',unknown:'确认未知 · 请勿重复发送'})[d.status]}}));
  const documents=task.artifacts.map(artifact=>{
    const receipt=task.deliverySubmissions?.find(r=>r.artifactIds.includes(artifact.id));
    const runId=receipt?.runId||runs.find(run=>artifact.id===`${run}-result`);
    const final=messages.filter(m=>m.runId===runId&&m.kind==='assistant'&&m.finalAnswer&&m.status==='completed');
    const documentOnly=!!receipt||artifact.kind!=='markdown'||final.length>0&&final.every(m=>!m.truncated&&!!m.text)&&final.map(m=>m.text).join('\n\n---\n\n')===artifact.content;
    return {runId,entry:{id:`artifact:${artifact.id}`,at:artifact.createdAt,text:'',artifact,documentOnly}};
  });
  const extras=[...inputs,...directions,...documents];
  const result:ChatEntry[]=extras.filter(x=>!x.runId||!runs.includes(x.runId)).map(x=>x.entry).sort((a,b)=>a.at-b.at);
  // Execution identity orders runs and messages. Wall-clock timestamps may move backward.
  for(const run of runs){
    result.push(...inputs.filter(x=>x.runId===run).map(x=>x.entry));
    const steering=directions.filter(x=>x.runId===run).map(x=>x.entry);
    let direction=0;
    for(const item of messages.filter(x=>x.runId===run)){
      while(direction<steering.length&&steering[direction].at<=item.at)result.push(steering[direction++]);
      result.push({id:`item:${item.runId}:${item.threadId}:${item.itemId}`,at:item.at,text:item.text,item});
    }
    result.push(...steering.slice(direction));
    result.push(...documents.filter(x=>x.runId===run).map(x=>x.entry));
  }
  return result;
}
export const followsLatest=(scrollTop:number,scrollHeight:number,clientHeight:number)=>scrollHeight-clientHeight-scrollTop<80;
export const shouldSendKey=(event:{key:string;ctrlKey:boolean;metaKey:boolean;isComposing:boolean})=>event.key==='Enter'&&(event.ctrlKey||event.metaKey)&&!event.isComposing;
