import test from 'node:test';import assert from 'node:assert/strict';import {combinedSignal} from './pdfAbortSignal.ts';
test('WK cancellation adapter retains reason and removes all source listeners',()=>{
 assert.equal(combinedSignal([]).aborted,false);
 const a=new AbortController(),b=new AbortController();let attached=0,removed=0;
 for(const signal of [a.signal,b.signal]){const add=signal.addEventListener.bind(signal),remove=signal.removeEventListener.bind(signal);signal.addEventListener=(...args)=>{attached++;add(...args);};signal.removeEventListener=(...args)=>{removed++;remove(...args);};}
 const signal=combinedSignal([a.signal,b.signal,a.signal]);assert.equal(attached,2);b.abort('reason');assert.equal(signal.reason,'reason');assert.equal(removed,2);a.abort('later');assert.equal(signal.reason,'reason');
 const early=new AbortController();early.abort('first');assert.equal(combinedSignal([early.signal,b.signal]).reason,'first');
 assert.throws(()=>combinedSignal([a.signal,{} as AbortSignal]),TypeError);assert.equal(attached,2);
});
