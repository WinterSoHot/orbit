import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, writeFile, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setImmediate } from 'node:timers/promises';
import { exportAfterSave } from './exportSnapshot.ts';

test('workspace export waits for a submitted save before creating its file', async()=>{
  const directory=await mkdtemp(join(tmpdir(),'orbit-snapshot-save-'));
  try {
    let finish!:()=>void;
    const save=new Promise<void>(resolve=>{finish=resolve;});
    const path=join(directory,'snapshot.json');
    const exported=exportAfterSave(save,async()=>{await writeFile(path,'{"saved":true}');return path;});
    const result=exported.then(path=>({path}),error=>({error}));
    await setImmediate();
    assert.deepEqual(await readdir(directory),[]);
    finish();
    assert.deepEqual(await result,{path});
    assert.equal(await readFile(path,'utf8'),'{"saved":true}');
  } finally {await rm(directory,{recursive:true,force:true});}
});

test('a failed submitted save prevents workspace export',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'orbit-snapshot-failed-'));
  try {
    await assert.rejects(exportAfterSave(Promise.reject(new Error('save failed')),async()=>{
      const path=join(directory,'snapshot.json');await writeFile(path,'should not exist');return path;
    }),/save failed/);
    assert.deepEqual(await readdir(directory),[]);
  } finally {await rm(directory,{recursive:true,force:true});}
});
