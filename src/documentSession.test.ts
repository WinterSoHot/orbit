import {test} from 'node:test';import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';import {tmpdir} from 'node:os';import {join} from 'node:path';import {setImmediate} from 'node:timers/promises';
import {DocumentSession} from './documentSession.ts';import type {LibraryDocument,DocumentChange} from './knowledge.ts';
function document():LibraryDocument{return {id:'doc',kind:'markdown',title:'note',tags:[],url:null,content:'original',revision:1,createdAt:0,updatedAt:0,draft:null,versions:[],blobId:null,sizeBytes:0,stamp:null};}
test('restore waits started persistence and skips queued older drafts',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'orbit-edit-order-'));try{
 let release!:()=>void;const pause=new Promise<void>(r=>{release=r;});let actual=document();const file=join(directory,'doc.json');
 const session=new DocumentSession(actual,async(change:DocumentChange)=>{
  assert.equal(change.documentId,'doc');assert.equal(change.expectedRevision,actual.revision);
  if(change.operation==='draft')await pause;
  actual={...actual,revision:actual.revision+1,content:change.operation==='restore'?'restored':actual.content,draft:change.operation==='draft'?{content:change.content,at:1}:null};
  await writeFile(file,JSON.stringify(actual));return actual;
 },'session');
 const outcome=(promise:ReturnType<DocumentSession['change']>)=>promise.then(value=>({value}),error=>({error}));
 const started=outcome(session.change('draft','first'));await setImmediate();const queued=outcome(session.change('draft','stale queued'));const restored=outcome(session.restore('visible newest text','version'));release();
 const [first,second,last]=await Promise.all([started,queued,restored]);assert.deepEqual(first,{value:null});assert.deepEqual(second,{value:null});assert.equal('value' in last?last.value?.content:null,'restored');assert.equal(JSON.parse(await readFile(file,'utf8')).revision,3);assert.equal(session.current().content,'restored');assert.equal(session.needsPersistence('restored'),false);
 }finally{await rm(directory,{recursive:true,force:true});}
});
test('persistence failure blocks later automatic work and preserves current revision',async()=>{
 let reject=true;const original=document();const session=new DocumentSession(original,async change=>{if(reject)throw new Error('disk failed');return {...original,revision:change.expectedRevision+1,content:change.content};},'session');
 await assert.rejects(session.change('commit','local unsaved'),/disk failed/);assert.equal(session.current().content,'original');
 reject=false;await assert.rejects(session.change('automatic','must not silently retry'),/disk failed/);session.retry();assert.equal((await session.change('commit','explicit retry'))?.content,'explicit retry');
});

test('a failed started request fences restore and keeps the last durable document',async()=>{
 let release!:()=>void;const pause=new Promise<void>(r=>{release=r;});let calls=0;
 const original=document();const session=new DocumentSession(original,async()=>{calls++;await pause;throw new Error('write failed');},'session');
 const outcome=(promise:ReturnType<DocumentSession['change']>)=>promise.then(value=>({value}),error=>({error}));
 const started=outcome(session.change('draft','new visible text'));await setImmediate();
 const queued=outcome(session.change('automatic','old queued text'));const restored=outcome(session.restore('newest visible text','version'));release();
 const results=await Promise.all([started,queued,restored]);assert('error' in results[0]);assert.deepEqual(results[1],{value:null});assert('error' in results[2]);assert.equal(calls,1);assert.equal(session.current(),original);await assert.rejects(session.settle(),/write failed/);assert.equal(session.needsPersistence('original'),false);
});

test('reverting to committed text clears completed and in-flight durable drafts',async()=>{
 for(const inFlight of [false,true]){
  const directory=await mkdtemp(join(tmpdir(),'orbit-revert-draft-'));
  try{
   const file=join(directory,'doc.json');let actual=document();let release!:()=>void;const pause=new Promise<void>(resolve=>{release=resolve;});
   const session=new DocumentSession(actual,async change=>{
    if(inFlight&&change.content==='withdrawn B')await pause;
    actual={...actual,revision:actual.revision+1,draft:change.content===actual.content?null:{content:change.content,at:1}};
    await writeFile(file,JSON.stringify(actual));return actual;
   },'session');
   assert.equal(session.needsPersistence('original'),false);
   const started=session.change('draft','withdrawn B');await setImmediate();
   assert.equal(session.needsPersistence('original'),true);
   if(inFlight){const cleared=session.change('draft','original');release();await Promise.all([started,cleared]);}
   else {release();await started;assert.equal(session.needsPersistence('original'),true);await session.change('draft','original');}
   assert.equal(session.needsPersistence('original'),false);const reopened=JSON.parse(await readFile(file,'utf8'));assert.equal(reopened.content,'original');assert.equal(reopened.draft,null);
  }finally{await rm(directory,{recursive:true,force:true});}
 }
});
