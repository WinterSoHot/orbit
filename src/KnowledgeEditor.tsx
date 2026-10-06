import {forwardRef,useEffect,useImperativeHandle,useRef,useState} from 'react';
import {ArrowDownToLine,Check,Pencil,SlidersHorizontal,Copy,Eye,History,Link2,RotateCcw,Save,X,Trash2,Folder,ExternalLink} from 'lucide-react';
import {changeDocument,desktop,exportDocument,readPdf,openLibraryLocation,savePdfReader,loadLibrary} from './bridge';
import {MarkdownBody} from './ArtifactEditor';
import {DocumentSession} from './documentSession';
import {backLinks,documentLink,kindLabels,linkedIds} from './knowledge';
import type {LibraryDocument,Collection} from './knowledge';
import {PdfReader} from './PdfReader';
import type {PdfReaderHandle} from './PdfReader';
import {pdfExcerpt,mergeReaderDocument} from './pdfReaderState';
import type {DocumentLocation,PdfReaderData,PdfAnnotation} from './pdfReaderState';
export interface EditorHandle {flush:()=>Promise<LibraryDocument>;beginMutation:()=>void;endMutation:()=>void}
const message=(e:unknown)=>e instanceof Error?e.message:String(e);
const formatDate=(n:number)=>new Date(n).toLocaleString('zh-CN',{month:'numeric',day:'numeric',hour:'2-digit',minute:'2-digit'});
export const KnowledgeEditor=forwardRef<EditorHandle,{document:LibraryDocument;documents:LibraryDocument[];locked?:boolean;collections?:Collection[];onOrganize?:(ids:string[])=>void;onLifecycle?:(operation:'trash'|'restore'|'purge')=>void;onSaved:(d:LibraryDocument)=>void;location?:DocumentLocation|null;onNavigate:(id:string,location?:DocumentLocation)=>void;onNotice:(message:string)=>void}>(function KnowledgeEditor({document:initial,documents,locked=false,collections=[],onOrganize,onLifecycle,onSaved,onNavigate,onNotice,location},ref){
 const reader=useRef<PdfReaderHandle>(null);
 const session=useRef(new DocumentSession(initial,changeDocument));
 const [doc,setDoc]=useState(initial),[body,setBody]=useState(initial.draft?.content??initial.content),[mode,setMode]=useState<'preview'|'edit'>('preview'),[organizing,setOrganizing]=useState(false),[membership,setMembership]=useState(initial.collectionIds||[]);
 const [title,setTitle]=useState(initial.title),[tags,setTags]=useState(initial.tags.join(', ')),[metadata,setMetadata]=useState(false),[versions,setVersions]=useState(false),[versionId,setVersionId]=useState<string|null>(null);
 const [error,setError]=useState(''),[stopped,setStopped]=useState(false),[pending,setPending]=useState(false),[saveStatus,setSaveStatus]=useState(initial.draft?'已恢复上次草稿':'已保存'),[pdf,setPdf]=useState<string|null>(null);
 const [resumeEpoch,setResumeEpoch]=useState(0);
 const bodyRef=useRef(body),paused=useRef(false),mutation=useRef(false),timers=useRef<ReturnType<typeof setTimeout>[]>([]),editSeq=useRef(0);
 const readOnly=initial.deletedAt!=null;
 const editorBusy=pending||locked,editingDisabled=editorBusy||readOnly;
 bodyRef.current=body;
 function clearTimers(){timers.current.forEach(clearTimeout);timers.current=[];}
 function accept(saved:LibraryDocument){setDoc(old=>mergeReaderDocument(old,saved));onSaved(saved);}
 function failed(reason:unknown){setError(message(reason));setSaveStatus('保存失败，当前文字保留');setStopped(true);}
 async function write(operation:'draft'|'automatic'|'commit',content:string){
  const captured=editSeq.current;const saved=await session.current.change(operation,content);
  if(saved){accept(saved);if(captured===editSeq.current){setSaveStatus(operation==='draft'?(saved.draft?'草稿已保存在本机':'已保存'):'已自动保存');setError('');}}
 }
 async function flush(){
  if(readOnly)return session.current.current();
  clearTimers();paused.current=true;setPending(true);const pdfHandle=reader.current;pdfHandle?.freeze();
  try{await pdfHandle?.flush();try{const current=await session.current.settle();if(bodyRef.current!==current.content||current.draft)await write('commit',bodyRef.current);setSaveStatus('已保存');return session.current.current();}catch(reason){failed(reason);throw reason;}}
  finally{paused.current=false;setPending(false);setResumeEpoch(v=>v+1);if(!mutation.current)pdfHandle?.unfreeze();}
 }
 useImperativeHandle(ref,()=>({flush,beginMutation:()=>{mutation.current=true;clearTimers();reader.current?.freeze();},endMutation:()=>{mutation.current=false;reader.current?.unfreeze();setResumeEpoch(v=>v+1);}}));
 useEffect(()=>{
  if(!desktop||readOnly||locked||pending||mutation.current||paused.current||stopped||!session.current.needsPersistence(body))return;
  timers.current=[setTimeout(()=>{if(mutation.current||readOnly)return;void write('draft',bodyRef.current).catch(failed);},600),setTimeout(()=>{if(mutation.current||readOnly)return;void write('automatic',bodyRef.current).catch(failed);},2000)];
  return clearTimers;
 },[body,doc.content,stopped,locked,readOnly,pending,resumeEpoch]);
 useEffect(()=>{
  const key=(event:KeyboardEvent)=>{if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'){event.preventDefault();if(!editingDisabled&&!mutation.current){session.current.retry();setStopped(false);void flush().catch(()=>{});}}};
  const leave=()=>{clearTimers();if(!desktop||readOnly||locked||mutation.current)return;void session.current.change('draft',bodyRef.current).catch(()=>{});};
  window.addEventListener('keydown',key);window.addEventListener('beforeunload',leave);
  return()=>{window.removeEventListener('keydown',key);window.removeEventListener('beforeunload',leave);};
 },[editingDisabled,locked,readOnly]);
 useEffect(()=>{if(initial.kind!=='pdf'||!desktop)return;let active=true;readPdf(initial.id).then(data=>{if(active)setPdf(data);}).catch(reason=>{if(active)setError(message(reason));});return()=>{active=false;};},[initial.id,initial.kind]);
 useEffect(()=>{if(location?.page||location?.annotation)setMode('preview');},[location]);
 async function changeMode(next:'preview'|'edit',afterFlush?:()=>void){if(mode===next){afterFlush?.();return;}const handle=reader.current;handle?.freeze();setPending(true);try{await handle?.flush();if(mutation.current)return;afterFlush?.();setMode(next);}catch{/* The reader owns its save error and recovery controls. */}finally{setPending(false);if(!mutation.current)handle?.unfreeze();}}
 async function reloadReader(){const library=await loadLibrary();if(library.error)throw Error(library.error);const latest=library.documents.find(d=>d.id===doc.id);if(!latest||latest.kind!=='pdf'||latest.deletedAt!=null)throw Error('文档已移除或进入回收站；未保存批注仍保留，可复制后关闭');return latest.pdfReader||{revision:0,page:1,scale:1,annotations:[]};}
 function readerSaved(value:PdfReaderData){const current={...session.current.current(),pdfReader:value};setDoc(old=>mergeReaderDocument(old,current));onSaved(current);}
 function excerpt(annotation:PdfAnnotation){if(editingDisabled||mutation.current)return;const next=bodyRef.current+pdfExcerpt(doc.title,doc.id,annotation);bodyRef.current=next;editSeq.current++;setBody(next);setSaveStatus('正在保存摘录…');onNotice('摘录已加入 Markdown 备注，链接可返回 PDF 原页');}
 async function restore(){
  if(!versionId||editingDisabled||mutation.current)return;clearTimers();paused.current=true;setPending(true);
  try{const saved=await session.current.restore(bodyRef.current,versionId);if(saved){accept(saved);editSeq.current++;setBody(saved.content);bodyRef.current=saved.content;setSaveStatus('已恢复历史，恢复前内容保留为新版本');setVersionId(null);setError('');}}
  catch(reason){failed(reason);}finally{paused.current=false;setPending(false);}
 }
 async function saveMetadata(){
  if(editingDisabled||mutation.current)return;
  try{const saved=await session.current.change('metadata',bodyRef.current,null,{title,tags:tags.split(/[,，]/).map(s=>s.trim()).filter(Boolean)});if(saved){accept(saved);setTitle(saved.title);setTags(saved.tags.join(', '));setMetadata(false);}}
  catch(reason){failed(reason);}
 }
 async function copyLink(){try{await navigator.clipboard.writeText(`[${doc.title.replace(/[\[\]]/g,'')} ](${documentLink(doc.id)})`.replace(' ]',']'));onNotice('文档链接已复制');}catch(reason){setError(`复制失败：${message(reason)}`);}}
 const backlinks=backLinks(documents,doc.id),outgoing=linkedIds(body),selectedVersion=doc.versions.find(v=>v.id===versionId);
 function insertLink(id:string){if(editingDisabled||mutation.current)return;const target=documents.find(d=>d.id===id);if(!target)return;void changeMode('edit',()=>{editSeq.current++;setBody(value=>`${value}\n\n[${target.title.replace(/[\[\]]/g,'')}](${documentLink(id)})\n`);setSaveStatus('正在保存草稿…');});}
 return <section className="knowledge-editor" aria-label="知识库文档">
  <header className="knowledge-document-header"><span className={`library-type ${doc.kind}`}>{kindLabels[doc.kind]}</span><h2 title={doc.title}>{doc.title}</h2><button aria-label="编辑标题和标签" className="icon-button" disabled={!desktop||editingDisabled} onClick={()=>setMetadata(v=>!v)}><SlidersHorizontal size={16}/></button></header>
  {readOnly&&<div className="knowledge-trash-banner"><Trash2 size={14}/><span>回收站 · 只读</span><small>恢复后可继续编辑</small></div>}
  <div className="knowledge-document-meta"><span>{formatDate(doc.updatedAt)}</span>{doc.tags.map(tag=><span className="library-tag" key={tag}>{tag}</span>)}</div>
  {doc.url&&<a className="knowledge-source" href={doc.url} target="_blank" rel="noopener noreferrer"><Link2 size={13}/>来源网页 · {doc.url}</a>}
  <div className="knowledge-editor-toolbar"><div className="document-tabs" role="tablist" aria-label="知识库文档模式"><button role="tab" aria-selected={mode==='preview'} onClick={()=>void changeMode('preview')}><Eye size={14}/>阅读</button><button role="tab" aria-selected={mode==='edit'} disabled={!desktop||editingDisabled} onClick={()=>void changeMode('edit')}><Pencil size={16}/>{doc.kind==='pdf'?'编辑备注':'编辑'}</button></div><button className="icon-button" aria-label="复制文档链接" onClick={()=>void copyLink()}><Link2 size={15}/></button><button className="icon-button" aria-label="版本历史" onClick={()=>{setVersions(v=>!v);setVersionId(null);}}><History size={16}/></button><button className="icon-button" aria-label="导出文档" disabled={!desktop||editorBusy} onClick={()=>{void flush().then(()=>exportDocument(doc.id)).then(path=>onNotice(`文档已导出：${path}`)).catch(reason=>setError(message(reason)));}}><ArrowDownToLine size={15}/></button>{doc.kind==='pdf'&&<><button className="icon-button" aria-label="打开 PDF 阅读副本" title="用默认应用打开 PDF 阅读副本" disabled={!desktop||editorBusy} onClick={()=>void openLibraryLocation(doc.id).then(()=>onNotice('已使用默认应用打开 PDF 阅读副本')).catch(e=>setError(message(e)))}><ExternalLink size={16}/></button><button className="icon-button" aria-label="在 Finder 中显示 PDF" title="在 Finder 中显示 PDF" disabled={!desktop||editorBusy} onClick={()=>void openLibraryLocation(doc.id,true).catch(e=>setError(message(e)))}><Folder size={16}/></button></>}{onLifecycle&&(readOnly?<><button className="icon-button" aria-label="恢复文档" title="恢复文档" disabled={!desktop||editorBusy} onClick={()=>onLifecycle('restore')}><RotateCcw size={16}/></button><button className="icon-button library-danger" aria-label="永久删除" title="永久删除" disabled={!desktop||editorBusy} onClick={()=>onLifecycle('purge')}><Trash2 size={16}/></button></>:<button className="icon-button" aria-label="移入回收站" title="移入回收站" disabled={!desktop||editorBusy} onClick={()=>onLifecycle('trash')}><Trash2 size={16}/></button>)}</div>
  <div className={`knowledge-content ${mode==='edit'?'editing':doc.kind==='pdf'?'pdf-reading':''}`}>
  {metadata&&<div className="knowledge-metadata-form"><label>标题<input disabled={editingDisabled} value={title} maxLength={128} onChange={e=>setTitle(e.target.value)}/></label><label>标签<input disabled={editingDisabled} placeholder="用逗号分隔，如：研究, 产品" value={tags} onChange={e=>setTags(e.target.value)}/></label><button className="primary-button compact" disabled={editingDisabled||!title.trim()} onClick={()=>void saveMetadata()}><Check size={14}/>保存信息</button></div>}
  {onOrganize&&<div className="knowledge-collections"><Folder size={14}/><span>{(doc.collectionIds||[]).map(id=>collections.find(c=>c.id===id)?.name).filter(Boolean).join(' · ')||'未分类'}</span><button disabled={!desktop||editingDisabled} onClick={()=>{setMembership(doc.collectionIds||[]);setOrganizing(v=>!v);}}>管理分类</button></div>}
  {organizing&&<div className="knowledge-membership-form"><strong>加入分类</strong><p>可加入多个分类，不会复制资料。</p>{collections.map(c=><label key={c.id}><input type="checkbox" disabled={editingDisabled} checked={membership.includes(c.id)} onChange={e=>setMembership(ids=>e.target.checked?[...ids,c.id]:ids.filter(id=>id!==c.id))}/>{c.name}</label>)}{!collections.length&&<p>先在左侧创建分类文件夹。</p>}<div><button className="secondary-button compact" disabled={editorBusy} onClick={()=>setOrganizing(false)}>取消</button><button className="primary-button compact" disabled={editingDisabled} onClick={()=>onOrganize?.(membership)}>保存分类</button></div></div>}
  {error&&<div className="knowledge-error" role="alert"><span>{error}</span><button className="secondary-button compact" onClick={()=>{void navigator.clipboard.writeText(body).then(()=>onNotice('当前草稿已复制')).catch(reason=>setError(message(reason)));}}><Copy size={13}/>复制草稿</button></div>}
  {versions&&<div className="knowledge-history"><div><strong>版本历史</strong><span>最多保留 20 个内容快照</span><button aria-label="关闭版本历史" className="icon-button" onClick={()=>{setVersions(false);setVersionId(null);}}><X size={14}/></button></div><div className="knowledge-history-list">{doc.versions.slice().reverse().map(v=><button key={v.id} className={versionId===v.id?'selected':''} onClick={()=>setVersionId(v.id)}><History size={12}/>{formatDate(v.at)}<span>{v.reason==='restore'?'恢复前备份':v.reason==='automatic'?'自动快照':'保存前版本'}</span></button>)}{!doc.versions.length&&<p>正文改变后会自动留下历史。</p>}</div>{selectedVersion&&<><MarkdownBody content={selectedVersion.content} onDocument={onNavigate}/><button className="secondary-button compact" disabled={!desktop||editingDisabled} onClick={()=>void restore()}><RotateCcw size={13}/>恢复此版本，保留当前内容</button></>}</div>}
  {doc.kind==='pdf'&&mode==='preview'&&(pdf?<PdfReader ref={reader} data={pdf} initial={doc.pdfReader} readOnly={readOnly} locked={editorBusy} location={location} persist={value=>savePdfReader(doc.id,value)} reload={reloadReader} onSaved={readerSaved} onExcerpt={excerpt}/>:<div className="library-loading">{desktop?'正在读取本地 PDF…':'在桌面 App 中读取 PDF'}</div>)}
  {mode==='edit'?<textarea className="knowledge-source-editor" aria-label={doc.kind==='pdf'?'PDF 备注源码':'知识库 Markdown 源码'} spellCheck={false} value={body} disabled={editingDisabled} onChange={e=>{editSeq.current++;setBody(e.target.value);setSaveStatus('正在保存草稿…');}}/>:<MarkdownBody content={body|| (doc.kind==='pdf'?'*尚无备注*':'*文档暂为空*')} onDocument={onNavigate}/>}
  <div className="knowledge-links"><div><strong>文档链接</strong><select aria-label="插入文档链接" value="" disabled={!desktop||editingDisabled} onChange={e=>insertLink(e.target.value)}><option value="">插入链接…</option>{documents.filter(d=>d.id!==doc.id).map(d=><option key={d.id} value={d.id}>{d.title}</option>)}</select></div>{outgoing.length>0&&<div><span>链接至</span>{outgoing.map(id=><button key={id} onClick={()=>onNavigate(id)}>{documents.find(d=>d.id===id)?.title||'文档不存在'}</button>)}</div>}<div><span>被引用 {backlinks.length}</span>{backlinks.map(d=><button key={d.id} onClick={()=>onNavigate(d.id)}>{d.title}</button>)}</div></div>
  </div>
  <footer className="knowledge-savebar"><span role="status">{readOnly?'回收站文档保留原内容和历史':locked?'正在处理文档操作…':pending?'正在保存…':saveStatus}</span><button className="secondary-button compact" disabled={!desktop||editingDisabled} onClick={()=>{session.current.retry();setStopped(false);void flush().catch(()=>{});}}><Save size={13}/>保存<span>⌘ S</span></button></footer>
 </section>;
});
