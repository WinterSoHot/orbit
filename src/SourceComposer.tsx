import './sources.css';
import {useEffect,useRef,useState} from 'react';
import {createPortal} from 'react-dom';
import {BookOpen,ChevronRight,Search,X,Loader2,FileText} from 'lucide-react';
import {desktop,loadLibrary} from './bridge';
import type {LibraryDocument} from './knowledge';
import {kindLabels} from './knowledge';
import {checkSources,parsePages} from './taskSources';
import type {SourceRequest,SourceInput,SourceSnapshot} from './taskSources';
import type {DocumentLocation} from './pdfReaderState';
import {extractPdfPages} from './pdfSourceText';

export interface SourceSelection {title:string;request:SourceRequest}
export type DocumentNavigator=(id:string,location?:DocumentLocation)=>void;
export function SourceComposer({value,onChange,disabled=false}:{value:SourceSelection[];onChange:(value:SourceSelection[])=>void;disabled?:boolean}){
 const [open,setOpen]=useState(false);
 return <div className="source-composer"><button type="button" className="source-add" disabled={disabled||!desktop} onClick={()=>setOpen(true)}><BookOpen size={14}/>引用资料{value.length>0&&<span>{value.length}</span>}</button>
  {value.map((s,i)=><span className="source-chip" key={`${i}:${s.request.documentId}`}><FileText size={12}/><span title={s.title}>{s.title}{s.request.pages.length?` · ${s.request.pages.join(',')} 页`:''}</span><button type="button" aria-label={`移除引用 ${s.title}`} disabled={disabled} onClick={()=>onChange(value.filter((_,index)=>index!==i))}><X size={12}/></button></span>)}
  {open&&createPortal(<SourcePicker value={value} onChange={onChange} onClose={()=>setOpen(false)}/>,document.body)}
 </div>;
}
export function SourcePicker({value,onChange,onClose}:{value:SourceSelection[];onChange:(s:SourceSelection[])=>void;onClose:()=>void}){
 const [documents,setDocuments]=useState<LibraryDocument[]>([]),[query,setQuery]=useState(''),[selected,setSelected]=useState<string|null>(null),[error,setError]=useState(''),[loaded,setLoaded]=useState(false);
 const [mode,setMode]=useState<SourceRequest['mode']>('excerpt'),[pages,setPages]=useState('1'),[annotation,setAnnotation]=useState(''),[text,setText]=useState(''),[prepared,setPrepared]=useState(false),[busy,setBusy]=useState(false);
 const dialog=useRef<HTMLElement>(null),abort=useRef<AbortController|null>(null);
 const doc=documents.find(d=>d.id===selected);
 useEffect(()=>{let alive=true;loadLibrary().then(data=>{if(!alive)return;if(data.error)throw Error(data.error);setDocuments(data.documents.filter(d=>d.deletedAt==null));setLoaded(true);}).catch(e=>{if(alive){setError(String(e));setLoaded(true);}});return()=>{alive=false;abort.current?.abort();};},[]);
 useEffect(()=>{const old=document.activeElement as HTMLElement|null;dialog.current?.querySelector<HTMLInputElement>('input')?.focus();const key=(e:KeyboardEvent)=>{if(e.key==='Escape'){e.preventDefault();e.stopPropagation();onClose();}if(e.key==='Tab'){const controls=[...dialog.current!.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled)')];const first=controls[0],last=controls.at(-1);if(e.shiftKey&&document.activeElement===first||!e.shiftKey&&document.activeElement===last){e.preventDefault();(e.shiftKey?last:first)?.focus();}}};window.addEventListener('keydown',key,true);return()=>{window.removeEventListener('keydown',key,true);old?.focus();};},[onClose]);
 function choose(d:LibraryDocument){abort.current?.abort();setSelected(d.id);setMode(d.kind==='pdf'?'pages':'excerpt');setPages('1');setAnnotation('');setText(d.kind==='pdf'?'':d.content);setPrepared(d.kind!=='pdf');setError('');setBusy(false);}
 function switchMode(next:SourceRequest['mode']){abort.current?.abort();setMode(next);setText(next==='excerpt'?doc?.content||'':'');setPrepared(next==='excerpt');setBusy(false);setError('');}
 function selectedRequest():SourceRequest {
  if(!doc)throw Error('请选择资料');
  return {documentId:doc.id,revision:doc.revision,mode,text,pages:mode==='pages'?parsePages(pages):mode==='annotation'?[doc.pdfReader!.annotations.find(a=>a.id===annotation)!.page]:[],readerRevision:doc.pdfReader?.revision??null,annotationId:mode==='annotation'?annotation:null};
 }
 async function prepare(){if(!doc)return;const controller=new AbortController();abort.current?.abort();abort.current=controller;setBusy(true);setError('');setPrepared(false);
  try{const body=await extractPdfPages(doc.id,parsePages(pages),controller.signal);if(!controller.signal.aborted){setText(body);setPrepared(true);}}
  catch(e){if(!controller.signal.aborted)setError(e instanceof Error?e.message:String(e));}finally{if(abort.current===controller)setBusy(false);}
 }
 function add(){try{const request=selectedRequest();checkSources([...value.map(s=>s.request),request]);onChange([...value,{title:doc!.title,request}]);onClose();}catch(e){setError(e instanceof Error?e.message:String(e));}}
 return <div className="modal-backdrop source-backdrop" onClick={onClose}><section ref={dialog} className="source-picker" role="dialog" aria-modal="true" aria-label="引用知识库资料" onClick={e=>e.stopPropagation()}>
  <div className="modal-heading"><div><BookOpen size={18}/><h2>引用资料</h2></div><button type="button" className="icon-button" aria-label="关闭资料选择" onClick={onClose}><X size={18}/></button></div>
  <div className="source-picker-body"><label className="source-search"><Search size={15}/><input aria-label="搜索可引用资料" placeholder="搜索标题、标签和已保存正文" value={query} onChange={e=>setQuery(e.target.value)}/></label>
   {!doc?<><p className="source-caption">选择要交给 Agent 的内容，只使用已保存版本。</p><div className="source-document-list">{documents.filter(d=>`${d.title} ${d.tags.join(' ')} ${d.content}`.toLocaleLowerCase().includes(query.toLocaleLowerCase())).map(d=><button type="button" key={d.id} onClick={()=>choose(d)}><FileText size={16}/><span><strong>{d.title}</strong><small>{kindLabels[d.kind]}{d.tags.length?` · ${d.tags.join(', ')}`:''}</small></span><ChevronRight size={14}/></button>)}</div>{loaded&&!documents.length&&<p>知识库暂无资料，请先导入文档。</p>}{!loaded&&<p>正在读取资料…</p>}</>:<>
    <div className="source-preview-heading"><strong>{doc.title}</strong><button type="button" className="source-add" onClick={()=>{abort.current?.abort();setSelected(null);setBusy(false);}}>更换资料</button></div>
    {doc.kind==='pdf'&&<><label>引用范围<select aria-label="PDF 引用范围" value={mode} onChange={e=>switchMode(e.target.value as SourceRequest['mode'])}><option value="pages">指定页面</option><option value="annotation">已保存批注</option><option value="excerpt">Markdown 备注</option></select></label>
     {mode==='pages'&&<div className="source-pages"><label>页码<input aria-label="引用 PDF 页码" placeholder="例如 1, 3-5" value={pages} onChange={e=>{abort.current?.abort();setPages(e.target.value);setPrepared(false);setBusy(false);}}/></label><button type="button" className="secondary-button compact" disabled={busy} onClick={()=>void prepare()}>{busy?<Loader2 size={14} className="spin"/>:<FileText size={14}/>}读取选页</button></div>}
     {mode==='annotation'&&<label>批注<select aria-label="选择 PDF 批注" value={annotation} onChange={e=>{const a=doc.pdfReader?.annotations.find(a=>a.id===e.target.value);setAnnotation(e.target.value);setText(a?`${a.text}${a.comment?`\n\n批注：${a.comment}`:''}`:'');setPrepared(!!a);}}><option value="">选择已保存批注</option>{doc.pdfReader?.annotations.map(a=><option key={a.id} value={a.id}>第 {a.page} 页 · {a.text.slice(0,50)}</option>)}</select></label>}
    </>}
    <label className="source-preview">将发送的文字<textarea aria-label="资料摘录预览" rows={9} value={text} readOnly={mode!=='excerpt'} onChange={e=>setText(e.target.value)} placeholder={mode==='pages'?'先读取指定页面':'选择批注或从已保存正文中保留一段摘录'}/></label><p className="source-caption">{[...text].length} / 32000 字{mode==='excerpt'?' · 可缩小为已保存正文中的连续摘录':' · PDF 在本机提取，不发送原件'}</p>
   </>}
   {error&&<p className="document-error" role="alert">{error}</p>}
  </div><div className="modal-footer"><span>{value.length} / 8 项资料</span><button type="button" className="secondary-button" onClick={onClose}>取消</button><button type="button" className="primary-button" disabled={!doc||!prepared||busy||!text.trim()||value.length>=8} onClick={add}>添加引用</button></div>
 </section></div>;
}
export function SourceCards({inputs,onDocument}:{inputs:SourceInput[];onDocument?:DocumentNavigator}){
 const shown=inputs.filter(i=>i.sources.length&&i.kind!=='template');
 const rows=shown.length?shown:inputs.filter(i=>i.kind==='template');
 if(!rows.length)return null;
 const label:Record<string,string>={pending:'待确认',accepted:'执行器已确认',unknown:'是否送达需核对',rejected:'已拒绝',cancelled:'已撤销'};
 return <details className="source-history"><summary><BookOpen size={14}/>本次提供的资料<span>{rows.reduce((n,i)=>n+i.sources.length,0)}</span></summary><p>保存的是发送时摘录，不代表模型逐条采用。</p>{rows.map(input=><section key={input.id}><small>{input.kind==='template'?'初始目标 · 尚未启动':input.kind==='direction'?'运行中补充':input.kind==='continue'?'继续交付':'初始目标'} · {label[input.status]||input.status}</small>{input.sources.map(s=><details key={s.id} className="source-snapshot"><summary>{s.title}{s.pages.length?` · 第 ${s.pages.join(',')} 页`:''}</summary><p>{s.text}</p><button type="button" className="chat-document-link" disabled={!onDocument} onClick={()=>onDocument?.(s.documentId,sourceLocation(s))}>打开原文</button><small>资料版本 {s.revision}</small></details>)}</section>)}</details>;
}
export function sourceLocation(s:SourceSnapshot):DocumentLocation{return {id:s.documentId,...(s.pages[0]?{page:s.pages[0]}:{}),...(s.annotationId?{annotation:s.annotationId}:{})};}
