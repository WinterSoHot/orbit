import {cpSync,mkdirSync,rmSync} from 'node:fs';
import {createRequire} from 'node:module';import {dirname,join} from 'node:path';
const root=dirname(createRequire(import.meta.url).resolve('pdfjs-dist/package.json'));
const output=new URL('../public/pdfjs/',import.meta.url);rmSync(output,{recursive:true,force:true});mkdirSync(output,{recursive:true});
for(const directory of ['cmaps','standard_fonts','wasm','web/images'])cpSync(join(root,directory),new URL(directory==='web/images'?'images/':`${directory}/`,output),{recursive:true});
