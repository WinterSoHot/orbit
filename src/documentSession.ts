import type {DocumentChange,LibraryDocument} from './knowledge.ts';
export class DocumentSession {
 private document:LibraryDocument;
 private transport:(change:DocumentChange)=>Promise<LibraryDocument>;
 private sessionId:string;
 private sequence=0;private epoch=0;private pendingWork=0;private tail:Promise<unknown>=Promise.resolve();private failure:unknown=null;
 constructor(doc:LibraryDocument,transport:(change:DocumentChange)=>Promise<LibraryDocument>,sessionId=crypto.randomUUID()){this.document=doc;this.transport=transport;this.sessionId=sessionId;}
 current(){return this.document;}
 needsPersistence(content:string){return content!==this.document.content||this.document.draft!==null||this.pendingWork>0;}
 async settle(){await this.tail;if(this.failure)throw this.failure;return this.document;}
 retry(){this.epoch++;this.failure=null;}
 change(operation:DocumentChange['operation'],content:string,versionId:string|null=null,metadata?:{title:string;tags:string[]}):Promise<LibraryDocument|null>{
 const epoch=this.epoch,sequence=++this.sequence;this.pendingWork++;
 const work=this.tail.then(async()=>{
  if(epoch!==this.epoch)return null;
  if(this.failure)throw this.failure;
  try {
   const saved=await this.transport({documentId:this.document.id,expectedRevision:this.document.revision,sessionId:this.sessionId,sequence,operation,content,versionId,...metadata});
   this.document=saved;
   return epoch===this.epoch?saved:null;
  }catch(error){this.failure=error;throw error;}
 });
 const result=work.finally(()=>{this.pendingWork--;});
 this.tail=result.then(()=>{},()=>{});
 return result;
}
 restore(content:string,versionId:string){this.epoch++;return this.change('restore',content,versionId);}
}
