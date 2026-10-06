import {test} from 'node:test';
import assert from 'node:assert/strict';
import {matchesGroup,linkedIds,pdfRenderScale} from './knowledge.ts';
import type {LibraryDocument} from './knowledge.ts';
const d:LibraryDocument={id:'a',kind:'markdown',title:'研究笔记',tags:['研究','Rust'],url:null,content:'Atomic STORAGE',revision:1,createdAt:0,updatedAt:1000,draft:null,versions:[],blobId:null,sizeBytes:0,stamp:null};
test('smart group applies all conditions and exact tags at the time boundary',()=>{
 assert(matchesGroup(d,{kind:'markdown',tag:'研究',keyword:'storage',updatedDays:1},86401000));
 assert(!matchesGroup(d,{tag:'研'}));assert(!matchesGroup(d,{kind:'pdf'}));assert(!matchesGroup(d,{updatedDays:1},86401001));assert(matchesGroup(d,{}));
});
test('stable links deduplicate valid IDs and ignore dangerous or incomplete targets',()=>{
 assert.deepEqual(linkedIds('[note](orbit://document/123e4567-e89b-12d3-a456-426614174000) [again](orbit://document/123e4567-e89b-12d3-a456-426614174000) orbit://document/../../bad javascript:alert(1)'),['123e4567-e89b-12d3-a456-426614174000']);
});

test('large PDF pages fit the canvas dimension and pixel budget',()=>{
 const scale=pdfRenderScale(20000,30000,2);assert(20000*scale<=4096);assert(30000*scale<=4096);assert(Math.ceil(20000*scale)*Math.ceil(30000*scale)<=8000000);assert.equal(pdfRenderScale(612,792,1),1);assert.throws(()=>pdfRenderScale(Infinity,100,1));
});
