import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applyRuntime, canContinue, boardColumn } from './model.ts';
import type { Task } from './model.ts';

function fixture(): Task {
  return { id: 'task-1', title: 'Research', prompt: 'Research', scene: 'research', provider: 'codex', status: 'approval', createdAt: 0, startedAt: 0, finishedAt: null, runId: 'run-1', threadId: null, turnId: 'turn-1', revision: 1, phase: 3, tokens: null,
    nodes: [{id:'lead',name:'Lead',role:'主 Agent',model:'CLI',status:'approval',summary:'',output:'',parentId:null}], events: [], artifacts: [],
    approvals: [{id:'approval-1',requestId:'approval-1',runId:'run-1',turnId:'turn-1',title:'Continue?',description:'',kind:'input',questionIds:[]}] };
}

test('late and stale real snapshots cannot overwrite current run', () => {
  const task = {...fixture(),provider:'codex' as const};
  assert.deepEqual(applyRuntime(task,{...task,runId:'old',revision:100,status:'completed'}),task);
  assert.deepEqual(applyRuntime(task,{...task,revision:0,status:'running'}),task);
  assert.equal(applyRuntime(task,{...task,revision:2,status:'completed'}).status,'completed');
});
test('terminal snapshots cannot regress and newer details are accepted', () => {
  const done={...fixture(),status:'completed' as const};
  assert.deepEqual(applyRuntime(done,{...done,revision:2,status:'running'}),done);
  assert.equal(applyRuntime(done,{...done,revision:2}).revision,2);
});

test('archived task cannot be changed by a late runtime event', () => {
  const task={...fixture(),status:'completed' as const,archived:true,artifacts:[{id:'doc',name:'doc.md',kind:'markdown',content:'saved delivery',createdAt:0}]};
  const next=applyRuntime(task,{...task,archived:false,revision:100,artifacts:[]});
  assert.equal(next.archived,true);
  assert.equal(next.artifacts[0].content,'saved delivery');
});

test('pending continuation rejects old run events and accepts new delivery', () => {
  const old={...fixture(),status:'completed' as const};
  const pending={...old,status:'queued' as const,runId:null,supersededRunId:old.runId};
  assert.equal(applyRuntime(pending,{...old,revision:100}).runId,null);
  assert.equal(applyRuntime(pending,{...old,runId:'run-2',status:'running',revision:2}).runId,'run-2');
});

test('chat continuation requires an ended session and reserves the next delivery slot', () => {
  const task={...fixture(),status:'completed' as const,threadId:'thread',artifacts:[{id:'doc',name:'v1.md',kind:'markdown',content:'result',createdAt:0}]};
  assert.equal(canContinue(task),true);
  assert.equal(canContinue({...task,status:'running'}),false);
  assert.equal(canContinue({...task,status:'unknown'}),false);
  assert.equal(canContinue({...task,archived:true}),false);
  assert.equal(canContinue({...task,artifacts:Array(10).fill(task.artifacts[0])}),false);
  assert.equal(canContinue({...task,status:'failed',turnId:null,supplements:[{runId:'run-1',previousTurnId:'last-known',text:'more',createdAt:0}]}),true);
});

test('executor snapshots update their own task and reject cross-executor events', () => {
  const task={...fixture(),provider:'qoder'};
  assert.equal(applyRuntime(task,{...task,revision:2,status:'running'}).revision,2);
  assert.deepEqual(applyRuntime(task,{...task,provider:'codex',revision:3}),task);
});

test('Qoder continuation depends on its own session and negotiated resume capability', () => {
  const task={...fixture(),provider:'qoder',status:'completed' as const,threadId:null,sessionRef:{provider:'qoder',protocol:'acp-v1',id:'session',cwd:'/tmp/task'},capabilities:{resume:true,steer:false,interrupt:true,agentHistory:false},artifacts:[{id:'doc',name:'v1.md',kind:'markdown',content:'result',createdAt:0}]};
  assert.equal(canContinue(task),true);
  assert.equal(canContinue({...task,capabilities:{...task.capabilities,resume:false}}),false);
  assert.equal(canContinue({...task,sessionRef:{...task.sessionRef,provider:'codex'}}),false);
});

test('board separates pending work, review and accepted deliveries', () => {
  const task={...fixture(),status:'completed' as const,artifacts:[{id:'doc',name:'v1.md',kind:'markdown',content:'result',createdAt:0}]};
  assert.equal(boardColumn(task),'attention');
  const accepted={...task,acceptance:{runId:'run-1',turnId:'turn-1',artifactIds:['doc']}};
  assert.equal(boardColumn(accepted),'done');
  assert.equal(boardColumn({...accepted,turnId:'turn-2'}),'attention');
  const queue={requestId:'request',order:1,nextRunId:'run-2',state:'pending' as const,action:{kind:'start' as const},error:null};
  assert.equal(boardColumn({...task,queue}),'progress');
  assert.equal(canContinue({...task,threadId:'thread',queue}),false);
  assert.equal(boardColumn({...task,queue:{...queue,state:'claimed'}}),'attention');
  assert.equal(boardColumn({...task,status:'queued',runId:null,artifacts:[]}),'backlog');
  const incoming={...task,queue:null,runId:'run-2',turnId:null,status:'running' as const,revision:3};
  assert.equal(applyRuntime({...task,queue},incoming).runId,'run-2');
  assert.equal(applyRuntime({...task,queue},{...incoming,runId:'unrelated'}).runId,'run-1');
});

test('manual and historical artifacts cannot fill an explicit current delivery',()=>{
  const task={...fixture(),status:'completed' as const,explicitDelivery:true,runId:'current',turnId:'turn',artifacts:[{id:'old',name:'old.md',kind:'markdown',content:'old',createdAt:0},{id:'manual',name:'reply.md',kind:'markdown',content:'reply',createdAt:1}],deliverySubmissions:[{id:'old',runId:'old-run',turnId:'turn',threadId:'thread',itemId:'reply',origin:'executor' as const,artifactIds:['old']},{id:'manual',runId:'current',turnId:null,threadId:'thread',itemId:'reply',origin:'manual' as const,artifactIds:['manual']}],acceptance:{runId:'current',turnId:'turn',artifactIds:['old','manual']}};
  assert.equal(boardColumn(task),'attention');
  const current={...task,deliverySubmissions:[...task.deliverySubmissions,{id:'new',runId:'current',turnId:'turn',threadId:'thread',itemId:'submitted',origin:'executor' as const,artifactIds:['new']}],artifacts:[...task.artifacts,{id:'new',name:'new.md',kind:'markdown',content:'new',createdAt:2}],acceptance:{runId:'current',turnId:'turn',artifactIds:['new']}};
  assert.equal(boardColumn(current),'done');
});
