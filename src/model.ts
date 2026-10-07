import {canReviewAccept} from "./teams.ts";
export type Status = 'queued' | 'running' | 'approval' | 'cancelling' | 'interrupted' | 'completed' | 'failed' | 'unknown';
export type Scene = 'research' | 'coding' | 'writing';
export type Provider = string;
export interface Capabilities { resume:boolean; steer:boolean; interrupt:boolean; input?:boolean; agentHistory:boolean }
export interface SessionRef { provider:string; protocol:string; id:string; cwd?:string|null; metadata?:unknown; [key:string]:unknown }
export interface AgentNode { id: string; name: string; role: string; model: string; status: Status; summary: string; output: string; parentId: string | null; agentPath?: string | null; detailNotice?: string | null; detailTurnCount?: number; outputTruncated?: boolean }
export interface DeliveryReceipt {id:string;runId:string;turnId:string|null;threadId:string;itemId:string;origin:"executor"|"manual"|"workspace";artifactIds:string[]}
export interface ChatItem {sourceInputIds?:string[];runId:string;threadId:string;itemId:string;kind:'assistant'|'tool';title:string;status:'running'|'completed'|'failed'|'unknown';text:string;finalAnswer:boolean;truncated:boolean;exitCode:number|null;at:number}
export interface Activity { id: string; at: number; kind: string; agent: string; text: string }
export interface InputQuestion {id:string;header?:string;question:string;options?:{label:string;description:string}[]|null;isOther?:boolean;isSecret?:boolean}
export interface Approval { id: string; requestId: string; runId: string; turnId: string; title: string; description: string; kind: string; questionIds: string[]; questions?:InputQuestion[];questionError?:string|null }
export interface Artifact { sourceInputIds?:string[]; id: string; name: string; kind: string; content: string; createdAt: number }
export interface Supplement { sourceThreadId?:string|null; runId:string; previousTurnId:string; text:string; createdAt:number }
export interface Direction { id:string; runId:string; turnId:string; text:string; status:'pending'|'accepted'|'rejected'|'unknown'; createdAt:number }
export type QueueAction = {kind:"start"} | {kind:"continue";text:string;runId:string|null;turnId:string|null};
export interface QueueRequest {cancelRequested?:boolean;requestId:string;order:number;nextRunId:string;state:"pending"|"claimed";action:QueueAction;error:string|null}
export interface Acceptance {runId:string|null;turnId:string|null;artifactIds:string[]}
export interface Task {
  assignment?:import('./teams').AgentProfile|null;team?:import('./teams').TeamWorkflow|null;parentLink?:{parentId:string;planVersion:string;role:'worker'|'review';coding:boolean}|null;teamInput?:import('./teams').ExecutionInput|null;reviewSubmission?:import('./teams').ReviewReceipt|null;codeWorkspace?:import('./teams').CodeWorkspace|null;deliveryError?:string|null;explicitDelivery?:boolean;deliverySubmissions?:DeliveryReceipt[]; sourceInputs?:import("./taskSources").SourceInput[]; requestedModel?:string|null; conversation?:ChatItem[]; conversationTruncated?:boolean; queue?:QueueRequest|null; acceptance?:Acceptance|null; sessionRef?:SessionRef|null; capabilities?:Capabilities|null; directions?:Direction[]; supplements?:Supplement[]; supersededRunId?:string|null; id: string; title: string; prompt: string; scene: Scene; provider: Provider; status: Status; archived?: boolean; createdAt: number; startedAt: number | null; finishedAt: number | null; runId: string | null; threadId: string | null; turnId: string | null; revision: number; agentActivityIds?: string[]; phase: number; nodes: AgentNode[]; events: Activity[]; approvals: Approval[]; artifacts: Artifact[]; tokens: number | null }
export const statusLabels: Record<Status, string> = { queued: '待启动', running: '运行中', approval: '等待你', cancelling: '中断请求中', interrupted: '已中断', completed: '执行结束', failed: '失败', unknown: '需核对' };
export const isActive = (status: Status) => ['running', 'approval', 'cancelling'].includes(status);
export const isTerminal = (status: Status) => ['completed', 'failed', 'interrupted'].includes(status);

export function applyRuntime(task: Task, incoming: Task): Task {
  const nextRun = !!task.queue && task.queue.nextRunId===incoming.runId && task.runId!==incoming.runId;
  if(task.supersededRunId===incoming.runId || task.archived || task.id!==incoming.id || task.provider!==incoming.provider || (task.runId!==null && task.runId!==incoming.runId && !nextRun) || incoming.revision<=task.revision) return task;
  if(!nextRun && isTerminal(task.status) && incoming.status!==task.status) return task;
  return incoming;
}
export type BoardColumn='backlog'|'progress'|'attention'|'done';
export function currentDeliveryIds(task:Task):string[]{
  const receipts=task.deliverySubmissions||[];
  return task.explicitDelivery?receipts.filter(r=>(r.origin==='executor'||r.origin==='workspace'&&!!task.codeWorkspace?.artifactId&&r.artifactIds.includes(task.codeWorkspace.artifactId))&&r.runId===task.runId&&r.turnId===task.turnId).flatMap(r=>r.artifactIds):task.artifacts.filter(a=>!receipts.some(r=>r.origin==='manual'&&r.artifactIds.includes(a.id))).map(a=>a.id);
}
export function isAccepted(task:Task):boolean {
  const a=task.acceptance,ids=currentDeliveryIds(task);
  return canReviewAccept(task)&&task.status==='completed'&&!!a&&a.runId===task.runId&&a.turnId===task.turnId&&ids.length>0&&a.artifactIds.length===ids.length&&a.artifactIds.every((id,i)=>id===ids[i]);
}
export function boardColumn(task:Task):BoardColumn {
  if(isAccepted(task)&&!task.queue)return 'done';
  if(task.team){if(task.team.cancelled||task.team.error||['plan','revision','ready'].includes(task.team.phase))return 'attention';if(['work','summary','review'].includes(task.team.phase))return 'progress';}
  if(task.parentLink&&task.status==='completed'&&!task.queue&&(task.reviewSubmission||currentDeliveryIds(task).length))return 'done';
  if(task.status==='approval'||task.queue?.state==='claimed'||task.queue?.error||(!task.queue&&['failed','interrupted','unknown'].includes(task.status)))return 'attention';
  if(task.queue||isActive(task.status))return 'progress';
  return task.status==='queued'?'backlog':'attention';
}



export function canContinue(task:Task):boolean {
  return !task.team && !task.parentLink && !task.archived && !task.queue && isTerminal(task.status) && canResume(task) && !!task.runId && !!(task.turnId || task.supplements?.at(-1)?.previousTurnId) && task.artifacts.length<10 && (task.supplements?.length||0)<10;
}

export function taskCapabilities(task:Task):Capabilities {
  return task.capabilities || (task.provider==='codex'?{resume:true,steer:true,interrupt:true,input:true,agentHistory:true}:{resume:false,steer:false,interrupt:false,input:false,agentHistory:false});
}
function canResume(task:Task):boolean {
  if(task.sessionRef && task.sessionRef.provider!==task.provider)return false;
  return taskCapabilities(task).resume && (task.provider==='codex'?!!task.threadId:!!task.sessionRef?.id);
}
