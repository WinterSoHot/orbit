import {test} from 'node:test';
import assert from 'node:assert/strict';
import {parsePages,checkSources} from './taskSources.ts';

test('source selection uses physical pages and Unicode budgets without silent truncation',()=>{
 assert.deepEqual(parsePages('1, 3-5, 3'),[1,3,4,5]);
 assert.throws(()=>parsePages('0'));
 assert.throws(()=>parsePages('1-11'));
 assert.throws(()=>parsePages('5-3'));
 const item={documentId:'doc',revision:1,mode:'excerpt' as const,text:'😀'.repeat(32000),pages:[1],readerRevision:null,annotationId:null};
 assert.doesNotThrow(()=>checkSources([item]));
 assert.throws(()=>checkSources([{...item,text:item.text+'字'}]));
 assert.throws(()=>checkSources(Array(9).fill({...item,text:'text'})));
 assert.throws(()=>checkSources([{...item,text:' '}]));
});
