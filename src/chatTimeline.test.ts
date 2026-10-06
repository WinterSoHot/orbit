import test from 'node:test';
import assert from 'node:assert/strict';
import {chatTimeline,followsLatest,shouldSendKey} from './chatTimeline.ts';
import {canContinue} from './model.ts';
import type {Task,ChatItem} from './model.ts';
const item=(itemId:string,text:string,at:number):ChatItem=>({runId:'run',threadId:'thread',itemId,kind:'assistant',title:'',status:'completed',text,finalAnswer:true,truncated:false,exitCode:null,at});
const task:Task={id:'task',title:'Task',prompt:'Goal',provider:'codex',scene:'research',status:'completed',createdAt:0,startedAt:0,finishedAt:5,threadId:'thread',runId:'run',turnId:'turn',revision:1,phase:0,nodes:[],events:[],approvals:[],tokens:null,artifacts:[]};
test('timeline preserves item order and hides only a proven complete duplicate',()=>{
  const conversation=[item('first','A',1),item('second','B',1)];
  const artifact={id:'run-result',name:'result.md',kind:'markdown',content:'A\n\n---\n\nB',createdAt:4};
  const entries=chatTimeline({...task,conversation,artifacts:[artifact]});
  assert.deepEqual(entries.slice(0,2).map(x=>x.item?.itemId),['first','second']);
  assert.equal(entries[2].documentOnly,true);
  for(const override of [{content:'edited'}, {id:'unlinked'}]) assert.equal(chatTimeline({...task,conversation,artifacts:[{...artifact,...override}]}).find(x=>x.artifact?.id===({...artifact,...override}).id)?.documentOnly,false);
  assert.equal(chatTimeline({...task,conversation:[{...conversation[0],truncated:true},conversation[1]],artifacts:[artifact]}).at(-1)?.documentOnly,false);
});
test('continue requires a bound terminal session but does not require a document',()=>{
  assert(canContinue({...task,status:'interrupted'}));
  assert(!canContinue({...task,threadId:null}));
  assert(!canContinue({...task,status:'unknown'}));
  assert(!canContinue({...task,status:'approval'}));
  assert(!canContinue({...task,sessionRef:{provider:'qoder',protocol:'acp',id:'session'}}));
});
test('latest tracking and modified Enter respect reading position and IME',()=>{
  assert(followsLatest(901,1000,100));
  assert(!followsLatest(400,1000,100));
  const key={key:'Enter',ctrlKey:true,metaKey:false,isComposing:false};
  assert(shouldSendKey(key));
  assert(!shouldSendKey({...key,isComposing:true}));
  assert(!shouldSendKey({...key,ctrlKey:false}));
});
test('clock rollback cannot reorder messages, continuation input or linked delivery',()=>{
  const first=item('first','A',100),second={...item('second','B',50),runId:'next'};
  const entries=chatTimeline({...task,conversation:[first,second],supplements:[{runId:'next',previousTurnId:'turn',text:'Follow up',createdAt:1}],artifacts:[{id:'run-result',name:'a.md',kind:'markdown',content:'A',createdAt:2},{id:'next-result',name:'b.md',kind:'markdown',content:'B',createdAt:3}]});
  assert.deepEqual(entries.map(x=>x.item?.itemId||x.artifact?.id||x.text),['first','run-result','Follow up','second','next-result']);
});
test('new continuation stays after old conversation even before any new item or when full',()=>{
  const conversation=[item('first','A',100)];
  const supplement={runId:'next',previousTurnId:'turn',text:'Follow up',createdAt:1};
  const previous={id:'run-result',name:'a.md',kind:'markdown',content:'A',createdAt:2};
  const entries=chatTimeline({...task,conversation,supplements:[supplement],artifacts:[previous],conversationTruncated:true});
  assert.deepEqual(entries.map(x=>x.item?.itemId||x.artifact?.id||x.text),['first','run-result','Follow up']);
  const finished=chatTimeline({...task,conversation,supplements:[supplement],artifacts:[previous,{id:'next-result',name:'b.md',kind:'markdown',content:'B',createdAt:0}],conversationTruncated:true});
  assert.deepEqual(finished.map(x=>x.item?.itemId||x.artifact?.id||x.text),['first','run-result','Follow up','next-result']);
});
