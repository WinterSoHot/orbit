import type {Task} from './model.ts';
export interface AgentProfile{id:string;name:string;provider:string;model:string|null;role:string;revision:number}
export interface Project{repo:string;commonDir:string;base:string;target:string}
export interface Assignment{agent:AgentProfile;goal:string;coding:boolean}
export interface PlanDraft{title:string;goal:string;criteria:string;coordinatorId:string;reviewerId:string;workers:{agentId:string;goal:string;coding:boolean}[];project:Project|null}
export interface Plan{version:string;goal:string;criteria:string;coordinator:AgentProfile;reviewer:AgentProfile;workers:Assignment[];project:Project|null}
export interface ReviewPacket{schemaVersion:number;submissionId:string;inputVersion:string;verdict:'pass'|'changes'|'unable';summary:string;findings:string[]}
export interface ReviewReceipt{packet:ReviewPacket;runId:string;turnId:string;itemId:string}
export interface CodeEvidence{base:string;tree:string;commit:string;diff:string;complete:boolean}
export interface CodeWorkspace{project:Project;directory:string;snapshot:CodeEvidence|null;snapshots?:CodeEvidence[];artifactId:string|null}
export interface ExecutionInput{version:string;plan:string;context?:string;items:{taskId:string;runId:string;turnId:string;artifactId:string;kind:string;name:string;content:string}[];code:CodeEvidence|null}
export interface TeamWorkflow{plan:Plan;phase:string;confirmed:string|null;children:string[];summaryInput:ExecutionInput|null;reviewTask:string|null;reviewInput:ExecutionInput|null;review:ReviewReceipt|null;error:string|null;cancelled:boolean;integration:CodeEvidence|null;gitOperation:{id:string;kind:string;old:string;new:string;target:string;state:string}|null}
export const phaseLabels:Record<string,string>={plan:'确认计划',work:'并行分工',summary:'汇总成果',review:'独立评审',revision:'需要修订',ready:'等待验收',cancelled:'已取消'};
export const teamPhaseLabel=(t:Task)=>phaseLabels[t.team?.phase||'']||'';
export const canReviewAccept=(t:Task)=>!t.team||t.team.phase==='ready'&&!t.team.cancelled&&t.team.review?.packet.verdict==='pass';
export const rootTasks=(tasks:Task[])=>tasks.filter(t=>!t.parentLink||!tasks.some(p=>p.id===t.parentLink!.parentId));
export function defaultProfiles():AgentProfile[]{return [{id:'orbit-coordinator',name:'协调者',role:'整合各项成果，核对目标与验收标准，明确分歧及限制。'},{id:'orbit-researcher',name:'分析师',role:'独立分析分配的问题，提供依据，明确无法验证的部分。'},{id:'orbit-reviewer',name:'评审者',role:'独立核对完整材料和验收标准，不修改作者成果。'}].map(a=>({...a,provider:'codex',model:null,revision:0}));}
