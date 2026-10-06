import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readModelDefaults,modelAvailable} from './modelSelection.ts';
test('model defaults remain provider scoped and missing models cannot be submitted',()=>{
  const choices=readModelDefaults({getItem:()=>'{"codex":"gpt-test","qoder":"Qwen-test","bad":42}'});
  assert.deepEqual(choices,{codex:'gpt-test',qoder:'Qwen-test'});
  assert(modelAvailable(null));
  assert(!modelAvailable(choices.codex, [{id:'Qwen-test',name:'Qwen',description:'',isDefault:false}]));
  assert(modelAvailable(choices.qoder,[{id:'Qwen-test',name:'Qwen',description:'',isDefault:false}]));
  assert.deepEqual(readModelDefaults({getItem:()=>'{bad json'}),{});
});
