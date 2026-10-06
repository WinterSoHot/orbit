// Real library/editor components with memory-only persistence; no user Store or executor.
import {createRoot} from 'react-dom/client';
import {applyTheme,initialTheme} from '../src/theme';
import type {LibraryDocument,DocumentChange} from '../src/knowledge';
import data from './pdf-fixture.json';
import '../src/styles.css';
applyTheme(initialTheme());
const note:LibraryDocument={id:'12345678-1234-4234-8234-123456789abc',kind:'markdown',title:'个人 AI 知识库工作台设计方案',content:'# 个人 AI 知识库工作台设计方案\n\n'+Array.from({length:35},(_,i)=>`## ${i+1}. 阅读与整理\n\n保持核心功能本地运行。文档支持标签、分类与链接，编辑留下历史版本。\n\n| 能力 | 说明 |\n| --- | --- |\n| 本地阅读 | 离线检索与查看资料 |\n\n`).join(''),revision:0,tags:['研究','交付'],url:null,createdAt:0,updatedAt:0,draft:null,versions:[],blobId:null,sizeBytes:0,stamp:null,collectionIds:['folder']};
let library={schemaVersion:3,error:null,collections:[{id:'folder',name:'Agent 工作台',parentId:null,revision:0}],groups:[],documents:[note,{...note,id:'11111111-1111-4111-8111-111111111111',title:'本地资料管理参考',content:'用于文档链接的资料。',collectionIds:[]},{...note,id:'22222222-2222-4222-8222-222222222222',kind:'pdf' as const,title:'PDF 阅读样本',content:'PDF 备注',blobId:'pdf',pdfReader:null}]};
let fail=false;
Object.assign(window,{isTauri:true,__TAURI_INTERNALS__:{invoke:async(command:string,args:Record<string,unknown>)=>{
 if(command==='load_library')return structuredClone(library);
 if(command==='read_pdf')return data;
 if(command==='change_document'){
  if(fail)throw Error('隔离测试：保存失败，草稿保留');
  const change=args.change as DocumentChange,index=library.documents.findIndex(d=>d.id===change.documentId),old=library.documents[index];
  if(old.revision!==change.expectedRevision)throw Error('Fixture CAS conflict');
  const next={...old,revision:old.revision+1,updatedAt:Date.now(),content:change.operation==='draft'?old.content:change.content,draft:change.operation==='draft'?{content:change.content,at:Date.now()}:null};
  library.documents[index]=next;return structuredClone(next);
 }
 if(command==='save_pdf_reader'){const old=library.documents.find(d=>d.id===args.documentId)!;old.pdfReader={...(args.value as object),revision:1} as never;return structuredClone(old.pdfReader);}
 throw Error('Unexpected fixture command '+command);
}}});
const {KnowledgeLibrary}=await import('../src/KnowledgeLibrary');
createRoot(document.getElementById('root')!).render(<div className="app-shell library-workspace"><aside className="orbit-sidebar"><div className="brand">Orbit</div><nav aria-label="主导航"><button className="nav-item">任务工作台</button><button className="nav-item active">知识库</button></nav></aside><main className="main-shell"><header className="topbar"><span>知识库 · 隔离测试数据</span><div><button onClick={()=>applyTheme('light')}>Light</button><button onClick={()=>applyTheme('dark')}>Dark</button><button onClick={()=>{fail=!fail;}}>切换保存失败</button></div></header><div className="page-content"><KnowledgeLibrary onNotice={()=>{}}/></div></main></div>);
