import type {LibraryDocument} from './knowledge.ts';
export type PdfRect=[number,number,number,number];
export interface PdfAnnotation {id:string;page:number;rects:PdfRect[];text:string;comment:string;color:'yellow'|'blue'|'pink'}
export interface PdfReaderData {revision:number;page:number;scale:number;annotations:PdfAnnotation[]}
export interface DocumentLocation {id:string;page?:number;annotation?:string}
export const emptyReader=():PdfReaderData=>({revision:0,page:1,scale:1,annotations:[]});
const uuid='[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}';
export function parseDocumentLink(value:string):DocumentLocation|null {
 const match=value.match(new RegExp(`^orbit://document/(${uuid})(?:\\?([^#]+))?$`,'i'));if(!match)return null;
 const result:DocumentLocation={id:match[1].toLowerCase()},params=new URLSearchParams(match[2]);
 if([...params.keys()].some((key,i,keys)=>!['page','annotation'].includes(key)||keys.indexOf(key)!==i))return null;
 const page=params.get('page'),annotation=params.get('annotation');
 if(page!==null){if(!/^[1-9]\d*$/.test(page)||Number(page)>100000)return null;result.page=Number(page);}
 if(annotation!==null){if(!new RegExp(`^${uuid}$`,'i').test(annotation))return null;result.annotation=annotation.toLowerCase();}
 return result;
}
const escapeMarkdown=(s:string)=>s.replace(/([\\`*_[\]<>#])/g,'\\$1');
export function pdfExcerpt(title:string,id:string,a:PdfAnnotation):string {
 return `\n\n> ${escapeMarkdown(a.text).split(/\r?\n/).join('\n> ')}\n\n${a.comment?`${escapeMarkdown(a.comment)}\n\n`:''}[${escapeMarkdown(title)} · 第 ${a.page} 页](orbit://document/${id}?page=${a.page}&annotation=${a.id})\n`;
}
export function mergeReaderDocument<T extends Pick<LibraryDocument,'id'|'revision'|'pdfReader'>>(old:T,incoming:T):T {
 if(old.id!==incoming.id)return old;
 const base=incoming.revision>old.revision?incoming:old;
 const reader=(incoming.pdfReader?.revision??-1)>(old.pdfReader?.revision??-1)?incoming.pdfReader:old.pdfReader;
 return {...base,pdfReader:reader};
}
export class PdfReaderSession {
 private saved:PdfReaderData;private wanted:PdfReaderData;private tail:Promise<unknown>=Promise.resolve();private failure:unknown=null;
 private transport:(value:PdfReaderData)=>Promise<PdfReaderData>;
 constructor(initial:PdfReaderData,transport:(value:PdfReaderData)=>Promise<PdfReaderData>){this.saved=initial;this.wanted=initial;this.transport=transport;}
 current(){return this.saved;}
 async retry(){await this.tail;this.failure=null;return this.save(this.wanted);}
 async reload(read:()=>Promise<PdfReaderData>){await this.tail;const latest=await read();this.saved=latest;this.wanted=latest;this.failure=null;return latest;}
 desired(){return this.wanted;}
 async flush(){await this.tail;if(this.failure)throw this.failure;return this.saved;}
 save(patch:Partial<Omit<PdfReaderData,'revision'>>):Promise<PdfReaderData> {
  this.wanted={...this.wanted,...patch};
  const work=this.tail.then(async()=>{
   if(this.failure)throw this.failure;
   try{const result=await this.transport({...this.saved,...patch});this.saved=result;this.wanted={...this.wanted,revision:result.revision};return result;}
   catch(error){this.failure=error;throw error;}
  });
  this.tail=work.then(()=>{},()=>{});return work;
 }
}
