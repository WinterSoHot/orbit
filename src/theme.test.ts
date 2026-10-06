import assert from 'node:assert/strict';
import test from 'node:test';
import {readTheme,saveTheme,themeKey} from './theme.ts';
test('theme respects valid saved preference, defaults to system and tolerates storage errors',()=>{
  let value:string|null='dark';const storage={getItem:()=>value,setItem:(key:string,next:string)=>{assert.equal(key,themeKey);value=next;}};
  assert.equal(readTheme(storage),'dark');assert(saveTheme('light',storage));assert.equal(readTheme(storage,true),'light');
  value='invalid';assert.equal(readTheme(storage,true),'dark');assert.equal(readTheme(storage,false),'light');
  const denied={getItem:()=>{throw Error('denied');},setItem:()=>{throw Error('denied');}};
  assert.equal(readTheme(denied,true),'dark');assert.equal(saveTheme('dark',denied),false);
});
