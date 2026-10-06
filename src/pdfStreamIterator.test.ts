import test from 'node:test';import assert from 'node:assert/strict';
import {streamValues} from './pdfStreamIterator.ts';
test('WebKit stream adapter consumes text, cancels early and releases locks on errors',async()=>{
 let cancelled=0;const stream=new ReadableStream({start(c){c.enqueue('a');c.enqueue('b');c.close();},cancel(){cancelled++;}});
 const values=[];for await(const value of streamValues.call(stream))values.push(value);
 assert.deepEqual(values,['a','b']);assert.equal(stream.locked,false);assert.equal(cancelled,0);
 const early=new ReadableStream({start(c){c.enqueue('a');},cancel(){cancelled++;}});
 for await(const value of streamValues.call(early)){assert.equal(value,'a');break;}
 assert.equal(cancelled,1);assert.equal(early.locked,false);
 const retained=new ReadableStream({start(c){c.enqueue('x');},cancel(){cancelled++;}});
 for await(const value of streamValues.call(retained,{preventCancel:true})){void value;break;}
 assert.equal(cancelled,1);assert.equal(retained.locked,false);
 const badCancel=new ReadableStream({start(c){c.enqueue('x');},cancel(){throw Error('cancel failed');}});
 await assert.rejects(async()=>{for await(const value of streamValues.call(badCancel)){void value;break;}},/cancel failed/);assert.equal(badCancel.locked,false);
 const failed=new ReadableStream({start(c){c.error(Error('source failure'));}});
 await assert.rejects(async()=>{for await(const value of streamValues.call(failed))void value;},/source failure/);assert.equal(failed.locked,false);
});
