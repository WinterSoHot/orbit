// Isolated integration fixture: native IPC is replaced before importing the bridge.
import {useState} from 'react';
import {createRoot} from 'react-dom/client';
import type {LibraryDocument} from '../src/knowledge';
import type {SourceSelection} from '../src/SourceComposer';
import type {SourceInput} from '../src/taskSources';
import data from './pdf-fixture.json';
import '../src/styles.css';
const id='12345678-1234-1234-1234-123456789abc';
const markdown:LibraryDocument={id,kind:'markdown',title:'本地知识库设计',tags:['离线'],url:null,content:'# 目标\n核心功能完全本地，AI 可选云端。',revision:3,createdAt:0,updatedAt:0,draft:null,versions:[],blobId:null,sizeBytes:0,stamp:null,collectionIds:[],deletedAt:null,pdfReader:null};
const pdf:LibraryDocument={...markdown,id:'11111111-1111-4111-8111-111111111111',kind:'pdf',title:'PDF 选页验证',content:'已保存的 PDF 备注',blobId:'11111111-1111-4111-8111-111111111111',pdfReader:{revision:1,page:1,scale:1,annotations:[{id:'87654321-4321-4321-4321-cba987654321',page:1,rects:[[40,540,200,570]],text:'Fixture annotation',comment:'Saved comment',color:'yellow'}]}};
Object.assign(window,{isTauri:true,__TAURI_INTERNALS__:{invoke:async(command:string)=>{
 if(command==='load_library')return {documents:[markdown,pdf],collections:[],groups:[],schemaVersion:3,error:null};
 if(command==='read_pdf')return data;
 throw Error(`Unexpected fixture command ${command}`);
}}});
const {SourceComposer,SourceCards}=await import('../src/SourceComposer');
const {applyTheme}=await import('../src/theme');applyTheme(location.search.includes('dark')?'dark':'light');
function App(){
 const [sources,setSources]=useState<SourceSelection[]>([]),[inputs,setInputs]=useState<SourceInput[]>([]),[location,setLocation]=useState('');
 return <main style={{padding:32,maxWidth:760,margin:'auto'}}><h1>资料引用验证</h1><p>此页面只使用测试资料，不读取用户数据、不调用模型。</p>
  <SourceComposer value={sources} onChange={setSources}/><button className="primary-button" disabled={!sources.length} onClick={()=>{setInputs([{id:'fixture-input',kind:'initial',text:'分析资料',runId:'fixture-run',turnId:'fixture-turn',status:'accepted',createdAt:0,sources:sources.map((s,i)=>({id:`fixture-${i}`,documentId:s.request.documentId,revision:s.request.revision,readerRevision:s.request.readerRevision,title:s.title,kind:s.request.documentId===id?'markdown':'pdf',url:null,text:s.request.text,pages:s.request.pages,annotationId:s.request.annotationId}))}]);setSources([]);}}>保存测试输入</button>
  <SourceCards inputs={inputs} onDocument={(id,where)=>setLocation(`${id} · page=${where?.page||'-'} · annotation=${where?.annotation||'-'}`)}/><output>{location}</output>
 </main>;
}
createRoot(document.getElementById('root')!).render(<App/>);
