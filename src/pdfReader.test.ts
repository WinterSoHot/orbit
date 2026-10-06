import test from 'node:test';
import assert from 'node:assert/strict';
import {parseDocumentLink,pdfExcerpt,mergeReaderDocument} from './pdfReaderState.ts';
import {PdfReaderSession} from './pdfReaderState.ts';
const id='12345678-1234-1234-1234-123456789abc',aid='87654321-4321-4321-4321-cba987654321';
const initial={revision:0,page:1,scale:1,annotations:[]};
test('PDF links validate locations and excerpts preserve literal text',()=>{
 assert.deepEqual(parseDocumentLink(`orbit://document/${id}?page=2&annotation=${aid}`),{id,page:2,annotation:aid});
 assert.deepEqual(parseDocumentLink(`orbit://document/${id}`),{id});
 for(const suffix of ['?page=0','?page=1.2','?page=NaN','?page=2&page=3','?annotation=bad','?foo=1','/extra'])assert.equal(parseDocumentLink(`orbit://document/${id}${suffix}`),null);
 const excerpt=pdfExcerpt('Name [x]',id,{id:aid,page:2,rects:[[0,0,1,1]],text:'hello\n[unsafe](https://x)',comment:'note *literal*',color:'yellow'});
 assert.ok(excerpt.includes(`?page=2&annotation=${aid}`));assert.ok(excerpt.includes('> hello\n>'));assert.ok(excerpt.includes('\\[unsafe\\]'));
});
test('reader CAS serializes latest state, blocks after failure and preserves body/reader revisions independently',async()=>{
 let calls:number[]=[];
 const session=new PdfReaderSession(initial,async value=>{calls.push(value.revision);return {...value,revision:value.revision+1};});
 await Promise.all([session.save({page:2}),session.save({scale:1.5})]);
 assert.deepEqual(calls,[0,1]);assert.equal((await session.flush()).page,2);assert.equal(session.current().scale,1.5);
 const failed=new PdfReaderSession(initial,async()=>{throw Error('disk full');});
 await assert.rejects(failed.save({page:3}),/disk full/);await assert.rejects(failed.flush(),/disk full/);assert.equal(failed.desired().page,3);
 const old={id,revision:3,pdfReader:{...initial,revision:5}},body={id,revision:4,pdfReader:{...initial,revision:2}};
 const merged=mergeReaderDocument(old,body);assert.equal(merged.revision,4);assert.equal(merged.pdfReader?.revision,5);
 const reversed=mergeReaderDocument(body,old);assert.equal(reversed.revision,4);assert.equal(reversed.pdfReader?.revision,5);
 assert.equal(mergeReaderDocument(old,{...body,id:'different'}).id,id);
});
test('reader reload retains failed input on read failure and abandons it only after successful reload',async()=>{
 for(const reason of ['CAS conflict','capacity rejected']){
  const session=new PdfReaderSession(initial,async value=>{if(value.revision===0)throw Error(reason);return {...value,revision:value.revision+1};});
  await assert.rejects(session.save({page:3}),new RegExp(reason));
  await assert.rejects(session.reload(async()=>{throw Error('read failed');}),/read failed/);
  assert.equal(session.desired().page,3);await assert.rejects(session.flush(),new RegExp(reason));
  const current={...initial,revision:7,page:2};await session.reload(async()=>current);
  assert.deepEqual(session.desired(),current);assert.deepEqual(await session.flush(),current);
  await session.save({scale:1.5});assert.equal(session.current().revision,8);
 }
});
