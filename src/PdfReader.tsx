import './pdfCompatibility';
import 'pdfjs-dist/legacy/web/pdf_viewer.css';
import {forwardRef,useEffect,useImperativeHandle,useRef,useState} from 'react';
import {ChevronLeft,ChevronRight,ZoomIn,ZoomOut,List,LayoutGrid,Search,Highlighter,MessageSquare,Trash2,Copy,Maximize2,Minimize2,ArrowDownToLine} from 'lucide-react';
import type {PDFDocumentProxy} from 'pdfjs-dist';
import type {PDFPageView} from 'pdfjs-dist/types/web/pdf_page_view';
import type {PDFViewer,PDFLinkService,EventBus} from 'pdfjs-dist/types/web/pdf_viewer';
import {emptyReader,PdfReaderSession} from './pdfReaderState';
import type {PdfReaderData,PdfAnnotation,PdfRect,DocumentLocation} from './pdfReaderState';
import workerUrl from './pdf.worker.ts?worker&url';
export interface PdfReaderHandle {flush:()=>Promise<void>;freeze:()=>void;unfreeze:()=>void}
interface Props {data:string;initial?:PdfReaderData|null;readOnly?:boolean;locked?:boolean;location?:DocumentLocation|null;persist?:(value:PdfReaderData)=>Promise<PdfReaderData>;reload?:()=>Promise<PdfReaderData>;onSaved?:(value:PdfReaderData)=>void;onExcerpt?:(annotation:PdfAnnotation)=>void}
type Outline={title:string;dest:string|unknown[];depth:number};
const colors={yellow:'#f6d657',blue:'#83b7ef',pink:'#efa1bd'};
const message=(e:unknown)=>e instanceof Error?e.message:String(e);
async function validateAnnotations(pdf:PDFDocumentProxy,annotations:PdfAnnotation[]){for(const a of annotations){if(a.page>pdf.numPages)throw Error('批注页码超出 PDF；已存数据保留');const p=await pdf.getPage(a.page),box=p.view;if(a.rects.some(r=>r[0]<box[0]-.1||r[1]<box[1]-.1||r[2]>box[2]+.1||r[3]>box[3]+.1))throw Error('批注坐标超出 PDF；已存数据保留');}}
export const PdfReader=forwardRef<PdfReaderHandle,Props>(function PdfReader({data,initial,readOnly=false,locked=false,location,persist,reload,onSaved,onExcerpt},ref){
 const container=useRef<HTMLDivElement>(null),viewerElement=useRef<HTMLDivElement>(null);
 const runtime=useRef<{viewer:PDFViewer;links:PDFLinkService;bus:EventBus}|null>(null);
 const session=useRef(new PdfReaderSession(initial||emptyReader(),async value=>persist?persist(value):({...value,revision:value.revision+1})));
 const callbacks=useRef({onSaved,onExcerpt});callbacks.current={onSaved,onExcerpt};
 const [reloading,setReloading]=useState(false),[confirmReload,setConfirmReload]=useState(false);
 const recovery=useRef<Promise<void>|null>(null),gate=useRef({readOnly,locked});gate.current={readOnly,locked};
 const frozen=useRef(false),disabled=useRef(readOnly||locked);disabled.current=readOnly||locked||!!recovery.current;
 const [pdf,setPdf]=useState<PDFDocumentProxy|null>(null),[ready,setReady]=useState(false),[page,setPage]=useState(initial?.page||1),[scale,setScale]=useState(initial?.scale||1);
 const [annotations,setAnnotations]=useState<PdfAnnotation[]>(initial?.annotations||[]),annotationRef=useRef(annotations);annotationRef.current=annotations;
 const [error,setError]=useState(''),[saveError,setSaveError]=useState(''),[status,setStatus]=useState(''),[sidebar,setSidebar]=useState<'none'|'outline'|'thumbs'|'annotations'>('none'),[outline,setOutline]=useState<Outline[]>([]),[expanded,setExpanded]=useState(false);
 const [query,setQuery]=useState(''),[matches,setMatches]=useState({current:0,total:0}),[selection,setSelection]=useState<Omit<PdfAnnotation,'id'|'comment'|'color'>|null>(null),[activeAnnotation,setActiveAnnotation]=useState('');
 const annotationTimer=useRef<ReturnType<typeof setTimeout>|null>(null),annotationsDirty=useRef(false);
 const timer=useRef<ReturnType<typeof setTimeout>|null>(null),position=useRef({page:initial?.page||1,scale:initial?.scale||1}),positionDirty=useRef(false);
 function clearTimer(){if(timer.current)clearTimeout(timer.current);timer.current=null;if(annotationTimer.current)clearTimeout(annotationTimer.current);annotationTimer.current=null;}
 async function save(patch:Partial<Omit<PdfReaderData,'revision'>>){
  setStatus('正在保存…');
  try{const result=await session.current.save(patch);callbacks.current.onSaved?.(result);setStatus('已保存在本机');setSaveError('');}
  catch(e){setSaveError(message(e));setStatus('保存失败，输入保留');throw e;}
 }
 async function reloadSaved(){
  if(!reload||disabled.current||frozen.current)return;disabled.current=true;setReloading(true);clearTimer();
  const work=(async()=>{try{const latest=await session.current.reload(async()=>{const value=await reload();if(!pdf)throw Error('PDF 尚未就绪');await validateAnnotations(pdf,value.annotations);return value;});annotationsDirty.current=false;positionDirty.current=false;annotationRef.current=latest.annotations;setAnnotations(latest.annotations);position.current={page:latest.page,scale:latest.scale};zoom(latest.scale);go(latest.page);setSelection(null);setActiveAnnotation('');callbacks.current.onSaved?.(latest);setSaveError('');setStatus('已载入保存版本');setConfirmReload(false);}catch(e){setSaveError(message(e));}finally{recovery.current=null;disabled.current=gate.current.readOnly||gate.current.locked;setReloading(false);}})();
  recovery.current=work;await work;
 }
 async function flush(){await recovery.current;clearTimer();if(annotationsDirty.current&&!readOnly){annotationsDirty.current=false;await save({annotations:annotationRef.current});}if(positionDirty.current&&!readOnly){positionDirty.current=false;await save(position.current);}await session.current.flush();}
 useImperativeHandle(ref,()=>({flush,freeze:()=>{frozen.current=true;clearTimer();},unfreeze:()=>{frozen.current=false;}}));
 function remember(){if(disabled.current||frozen.current)return;positionDirty.current=true;if(timer.current)clearTimeout(timer.current);timer.current=setTimeout(()=>{positionDirty.current=false;void save(position.current).catch(()=>{});},650);}
 function paint(){
  const viewer=runtime.current?.viewer;if(!viewer)return;
  viewerElement.current?.querySelectorAll<HTMLElement>('.page').forEach(element=>{
   const number=Number(element.dataset.pageNumber),view=viewer.getPageView(number-1) as PDFPageView|undefined;if(!view?.viewport||!element.querySelector('canvas'))return;
   let overlay=element.querySelector<HTMLDivElement>('.orbit-pdf-highlights');if(!overlay){overlay=document.createElement('div');overlay.className='orbit-pdf-highlights';element.append(overlay);}
   overlay.replaceChildren();
   for(const annotation of annotationRef.current.filter(a=>a.page===number))for(const rect of annotation.rects){
    const r=[...view.viewport.convertToViewportPoint(rect[0],rect[1]),...view.viewport.convertToViewportPoint(rect[2],rect[3])],node=document.createElement('span');
    Object.assign(node.style,{left:`${Math.min(r[0],r[2])/view.viewport.width*100}%`,top:`${Math.min(r[1],r[3])/view.viewport.height*100}%`,width:`${Math.abs(r[2]-r[0])/view.viewport.width*100}%`,height:`${Math.abs(r[3]-r[1])/view.viewport.height*100}%`,background:colors[annotation.color]});
    node.dataset.annotation=annotation.id;overlay.append(node);
   }
  });
 }
 useEffect(()=>{paint();},[annotations,ready]);
 useEffect(()=>{
  let active=true,initialized=false;const abort=new AbortController();let loading:{destroy:()=>Promise<void>}|undefined;setPdf(null);setReady(false);setError('');
  void (async()=>{
   const api=await import('pdfjs-dist/legacy/build/pdf.mjs');if(!active)return;api.GlobalWorkerOptions.workerSrc=workerUrl;
   const ui=await import('pdfjs-dist/legacy/web/pdf_viewer.mjs');if(!active||!container.current||!viewerElement.current)return;
   const bus=new ui.EventBus(),links=new ui.PDFLinkService({eventBus:bus,ignoreDestinationZoom:true});links.externalLinkEnabled=false;
   const find=new ui.PDFFindController({eventBus:bus,linkService:links});
   const viewer=new ui.PDFViewer({container:container.current,viewer:viewerElement.current,eventBus:bus,linkService:links,findController:find,textLayerMode:1,annotationMode:1,annotationEditorMode:-1,enableAutoLinking:false,enableDetailCanvas:false,removePageBorders:true,maxCanvasPixels:2000000,maxCanvasDim:4096,imageResourcesPath:'/pdfjs/images/',...{abortSignal:abort.signal}});
   links.setViewer(viewer);runtime.current={viewer,links,bus};
   bus.on('pagesinit',()=>{if(!active)return;const current=session.current.current();viewer.currentScale=current.scale;viewer.currentPageNumber=Math.min(current.page,viewer.pagesCount);if(viewer.pagesCount>200)viewer.scrollMode=ui.ScrollMode.PAGE;setPage(viewer.currentPageNumber);setScale(viewer.currentScale);position.current={page:viewer.currentPageNumber,scale:viewer.currentScale};initialized=true;setReady(true);});
   bus.on('pagechanging',({pageNumber}:{pageNumber:number})=>{if(!active)return;setPage(pageNumber);position.current={page:pageNumber,scale:viewer.currentScale||1};if(initialized)remember();});
   bus.on('scalechanging',({scale:newScale}:{scale:number})=>{if(!active)return;setScale(newScale);position.current={page:viewer.currentPageNumber,scale:Math.min(4,Math.max(.25,newScale))};if(initialized)remember();paint();});
   bus.on('pagerendered',({error:renderError}:{error?:unknown})=>{if(!active)return;if(renderError)setError(`页面渲染失败：${message(renderError)}`);paint();});
   bus.on('textlayerrendered',({error:textError}:{error?:unknown})=>{if(active&&textError)setError(`文字层无法读取：${message(textError)}`);});
   bus.on('updateviewarea',()=>{if(active)paint();});
   bus.on('updatefindmatchescount',({matchesCount}:{matchesCount:{current:number;total:number}})=>{if(active)setMatches(matchesCount);});
   bus.on('updatefindcontrolstate',({state,matchesCount}:{state:number;matchesCount:{current:number;total:number}})=>{if(active)setMatches(state===1?{current:0,total:0}:matchesCount);});
   const task=api.getDocument({data:Uint8Array.from(atob(data),c=>c.charCodeAt(0)),useSystemFonts:true,useWasm:false,cMapUrl:'/pdfjs/cmaps/',cMapPacked:true,standardFontDataUrl:'/pdfjs/standard_fonts/',stopAtErrors:true,maxImageSize:8000000,disableAutoFetch:true});loading=task;
   task.onPassword=()=>{if(active)setError('此 PDF 已加密，请用系统阅读器打开');void task.destroy().catch(()=>{});};
   const document=await task.promise;if(!active)return;if(document.numPages>10000)throw Error('PDF 超过 10000 页，请使用系统阅读器');
   // Bounds are validated against the actual PDF before any stored overlay is shown.
   await validateAnnotations(document,annotationRef.current);
   if(!active)return;setPdf(document);links.setDocument(document);viewer.setDocument(document);
   const items=await document.getOutline();if(!active)return;const rows:Outline[]=[];
   const visit=(nodes:NonNullable<typeof items>,depth:number)=>{if(depth>8)return;for(const node of nodes){if(rows.length>=1000)return;if(node.dest)rows.push({title:node.title,dest:node.dest,depth});visit(node.items,depth+1);}};if(items)visit(items,0);setOutline(rows);
  })().catch(reason=>{if(active)setError(`PDF 无法读取：${message(reason)}`);});
  return()=>{active=false;clearTimer();abort.abort();const current=runtime.current;runtime.current=null;current?.viewer.setDocument(null);current?.links.setDocument(null);void loading?.destroy().catch(()=>{});};
 },[data]);
 useEffect(()=>{if(!ready||!location)return;const annotation=annotationRef.current.find(a=>a.id===location.annotation);go(annotation?.page||location.page||1);setActiveAnnotation(annotation?.id||'');if(annotation)setSidebar('annotations');},[location,ready]);
 function go(number:number){const viewer=runtime.current?.viewer;if(viewer&&ready)viewer.scrollPageIntoView({pageNumber:Math.max(1,Math.min(viewer.pagesCount,number))});}
 function zoom(value:number){if(runtime.current)runtime.current.viewer.currentScale=Math.max(.25,Math.min(4,value));}
 function find(text:string,again=false,previous=false){setQuery(text);runtime.current?.bus.dispatch('find',{source:container.current,type:again?'again':'',query:text,caseSensitive:false,entireWord:false,highlightAll:true,findPrevious:previous});}
 function captureSelection(){
  const selected=window.getSelection();if(!selected||selected.isCollapsed||!selected.rangeCount)return;
  const element=(node:Node|null)=>node?.nodeType===Node.ELEMENT_NODE?node as Element:node?.parentElement;
  const start=element(selected.anchorNode)?.closest<HTMLElement>('.page'),end=element(selected.focusNode)?.closest<HTMLElement>('.page');
  if(!start||!viewerElement.current?.contains(start))return;
  if(start!==end){setSelection(null);setStatus('请在同一页内选择文字后高亮');return;}
  const number=Number(start.dataset.pageNumber),view=runtime.current?.viewer.getPageView(number-1) as PDFPageView|undefined;if(!view?.viewport)return;
  const text=selected.toString().trim();if(!text)return;const bounds=start.getBoundingClientRect(),rects:PdfRect[]=[];
  for(const rect of Array.from(selected.getRangeAt(0).getClientRects())){
   const left=Math.max(bounds.left,rect.left),top=Math.max(bounds.top,rect.top),right=Math.min(bounds.right,rect.right),bottom=Math.min(bounds.bottom,rect.bottom);if(right-left<1||bottom-top<1)continue;
   const [x1,y1]=view.viewport.convertToPdfPoint((left-bounds.left)*view.viewport.width/bounds.width,(top-bounds.top)*view.viewport.height/bounds.height),[x2,y2]=view.viewport.convertToPdfPoint((right-bounds.left)*view.viewport.width/bounds.width,(bottom-bounds.top)*view.viewport.height/bounds.height);
   const box=view.pdfPage.view as number[],r:PdfRect=[Math.max(box[0],Math.min(x1,x2)),Math.max(box[1],Math.min(y1,y2)),Math.min(box[2],Math.max(x1,x2)),Math.min(box[3],Math.max(y1,y2))];
   if(r[2]>r[0]&&r[3]>r[1]&&!rects.some(a=>a.every((v,i)=>Math.abs(v-r[i])<.2)))rects.push(r);
  }
  if(new TextEncoder().encode(text).length>16384||rects.length>256){setStatus('选区过大，请缩小选区');setSelection(null);return;}
  if(rects.length)setSelection({page:number,rects,text});
 }
 useEffect(()=>{const listener=()=>captureSelection();document.addEventListener('selectionchange',listener);return()=>document.removeEventListener('selectionchange',listener);},[]);
 function changeAnnotations(next:PdfAnnotation[],delayed=false){if(disabled.current||frozen.current||saveError)return;setAnnotations(next);annotationRef.current=next;if(annotationTimer.current)clearTimeout(annotationTimer.current);annotationsDirty.current=true;if(delayed){annotationTimer.current=setTimeout(()=>{annotationsDirty.current=false;void save({annotations:annotationRef.current}).catch(()=>{});},400);}else {annotationsDirty.current=false;void save({annotations:next}).catch(()=>{});}}
 function highlight(){if(!selection||disabled.current||frozen.current||saveError)return;if(annotations.length>=500){setStatus('每篇 PDF 最多 500 条批注');return;}const annotation:PdfAnnotation={...selection,id:crypto.randomUUID(),comment:'',color:'yellow'};changeAnnotations([...annotationRef.current,annotation]);setActiveAnnotation(annotation.id);setSidebar('annotations');setSelection(null);window.getSelection()?.removeAllRanges();}
 const near=pdf?Array.from({length:Math.min(5,pdf.numPages)},(_,i)=>Math.max(1,Math.min(pdf.numPages-4,page-2))+i):[];
 return <div className={`pdf-reader${expanded?' pdf-reader-expanded':''}`} aria-label="PDF 阅读器">
  <div className="pdf-controls"><button aria-label="PDF 目录" title="目录" aria-pressed={sidebar==='outline'} onClick={()=>setSidebar(s=>s==='outline'?'none':'outline')}><List size={16}/></button><button aria-label="PDF 缩略图" title="附近页面缩略图" aria-pressed={sidebar==='thumbs'} onClick={()=>setSidebar(s=>s==='thumbs'?'none':'thumbs')}><LayoutGrid size={16}/></button><button aria-label="PDF 批注" title="批注" aria-pressed={sidebar==='annotations'} onClick={()=>setSidebar(s=>s==='annotations'?'none':'annotations')}><MessageSquare size={16}/></button><span className="pdf-divider"/><button aria-label="PDF 上一页" disabled={!ready||page<=1} onClick={()=>go(page-1)}><ChevronLeft size={16}/></button><label className="pdf-page-input"><input aria-label="PDF 页码" type="number" min={1} max={pdf?.numPages||1} value={page} disabled={!ready} onChange={e=>{const n=Number(e.target.value);if(Number.isInteger(n)&&n>0)go(n);}}/><span>/ {pdf?.numPages||'…'}</span></label><button aria-label="PDF 下一页" disabled={!ready||page>=(pdf?.numPages||1)} onClick={()=>go(page+1)}><ChevronRight size={16}/></button><span className="pdf-controls-space"/><button aria-label="缩小 PDF" disabled={!ready||scale<=.25} onClick={()=>zoom(scale-.25)}><ZoomOut size={16}/></button><button className="pdf-scale" aria-label="PDF 适合宽度" disabled={!ready} onClick={()=>{const width=container.current?.clientWidth||600;const view=runtime.current?.viewer.getPageView(page-1) as PDFPageView|undefined;if(view?.viewport)zoom(scale*(width-36)/view.viewport.width);}}>{Math.round(scale*100)}%</button><button aria-label="放大 PDF" disabled={!ready||scale>=4} onClick={()=>zoom(scale+.25)}><ZoomIn size={16}/></button><button aria-label={expanded?'收起 PDF 阅读器':'展开 PDF 阅读器'} onClick={()=>setExpanded(v=>!v)}>{expanded?<Minimize2 size={16}/>:<Maximize2 size={16}/>}</button></div>
  <div className="pdf-find"><Search size={14}/><input aria-label="搜索 PDF 正文" placeholder="搜索 PDF 正文" value={query} disabled={!ready} onChange={e=>find(e.target.value)} onKeyDown={e=>{if(e.key==='Enter')find(query,true,e.shiftKey);}}/><span>{query?`${matches.current} / ${matches.total}`:'可选择文字并复制'}</span><button aria-label="上一个搜索结果" disabled={!query} onClick={()=>find(query,true,true)}><ChevronLeft size={14}/></button><button aria-label="下一个搜索结果" disabled={!query} onClick={()=>find(query,true)}><ChevronRight size={14}/></button><button className="pdf-highlight-button" disabled={!selection||readOnly||locked||reloading||!!saveError} onMouseDown={e=>e.preventDefault()} onClick={highlight}><Highlighter size={14}/>高亮</button></div>
  {error&&<p className="document-error" role="alert">{error}</p>}
  {saveError&&<div className="pdf-save-error" role="alert">{saveError}<button disabled={locked||readOnly||reloading} onClick={()=>{void session.current.retry().then(result=>{callbacks.current.onSaved?.(result);setSaveError('');setStatus('已重新保存');}).catch(e=>setSaveError(message(e)));}}>重新保存</button><button onClick={()=>void navigator.clipboard.writeText(JSON.stringify(session.current.desired(),null,2)).catch(e=>setStatus(message(e)))}><Copy size={13}/>复制未保存批注</button>{reload&&<button disabled={locked||readOnly||reloading} onClick={()=>setConfirmReload(true)}>重新载入保存版本</button>}{confirmReload&&<div role="alertdialog" aria-label="放弃未保存批注确认"><p>重新载入会放弃本地未保存批注。请先复制需要保留的内容；读取失败时仍保留当前输入。</p><button disabled={reloading} onClick={()=>setConfirmReload(false)}>取消，保留输入</button><button disabled={locked||readOnly||reloading} onClick={()=>void reloadSaved()}>{reloading?'正在读取…':'放弃修改并重新载入'}</button></div>}</div>}
  <div className="pdf-workspace">
   {sidebar!=='none'&&<aside className="pdf-sidebar" aria-label="PDF 阅读导航"><div className="pdf-sidebar-title">{sidebar==='outline'?'目录':sidebar==='thumbs'?'附近页面':'批注'}<span>{sidebar==='annotations'?annotations.length:''}</span></div>
    {sidebar==='outline'&&(outline.length?outline.map((item,i)=><button className="pdf-outline-row" key={i} style={{paddingLeft:12+item.depth*12}} onClick={()=>void runtime.current?.links.goToDestination(item.dest as string|number[]).catch(e=>setError(message(e)))}>{item.title}</button>):<p>此 PDF 没有目录</p>)}
    {sidebar==='thumbs'&&pdf&&<><button className="pdf-outline-row" onClick={()=>go(Math.max(1,page-5))}>前 5 页</button>{near.map(n=><button key={n} className={`pdf-thumbnail${page===n?' selected':''}`} onClick={()=>go(n)}><Thumbnail pdf={pdf} page={n}/><span>第 {n} 页</span></button>)}<button className="pdf-outline-row" onClick={()=>go(Math.min(pdf.numPages,page+5))}>后 5 页</button></>}
    {sidebar==='annotations'&&(annotations.length?annotations.map(a=><div className={`pdf-annotation${activeAnnotation===a.id?' selected':''}`} key={a.id}><button className="pdf-annotation-text" onClick={()=>{go(a.page);setActiveAnnotation(a.id);}}><span style={{background:colors[a.color]}}/>第 {a.page} 页<blockquote>{a.text}</blockquote></button><textarea aria-label={`第 ${a.page} 页批注评论`} placeholder="添加评论…" value={a.comment} disabled={readOnly||locked||reloading||!!saveError} maxLength={4000} onChange={e=>changeAnnotations(annotationRef.current.map(v=>v.id===a.id?{...v,comment:e.target.value}:v),true)}/><div><select aria-label="高亮颜色" value={a.color} disabled={readOnly||locked||reloading||!!saveError} onChange={e=>changeAnnotations(annotationRef.current.map(v=>v.id===a.id?{...v,color:e.target.value as PdfAnnotation['color']}:v))}><option value="yellow">黄色</option><option value="blue">蓝色</option><option value="pink">粉色</option></select><button title="加入 Markdown 备注" aria-label="批注加入 Markdown 备注" disabled={!onExcerpt||readOnly||locked||reloading||!!saveError} onClick={()=>callbacks.current.onExcerpt?.(a)}><ArrowDownToLine size={13}/></button><button aria-label="删除 PDF 批注" disabled={readOnly||locked||reloading||!!saveError} onClick={()=>changeAnnotations(annotationRef.current.filter(v=>v.id!==a.id))}><Trash2 size={13}/></button></div></div>):<p>选中一页中的文字，再点击「高亮」。扫描页需 OCR 后才能选字。</p>)}
   </aside>}
   <div className="pdf-view-host"><div className="orbit-pdf-container" ref={container} onPointerUp={captureSelection}><div ref={viewerElement} className="pdfViewer"/></div>{!ready&&!error&&<div className="pdf-reader-loading">正在读取 PDF…</div>}</div>
  </div><div className="pdf-reader-status"><span>{readOnly?'回收站 · 批注只读':status||'批注单独保存 · PDF 原件不变'}</span><span>{pdf&&pdf.numPages>200?'长文档 · 分页阅读':'连续阅读'} · 保存页码与缩放</span></div>
 </div>;
});
function Thumbnail({pdf,page}:{pdf:PDFDocumentProxy;page:number}){
 const canvas=useRef<HTMLCanvasElement>(null);
 useEffect(()=>{let active=true;let task:{cancel:()=>void}|undefined;void pdf.getPage(page).then(p=>{if(!active||!canvas.current)return;const unit=p.getViewport({scale:1}),viewport=p.getViewport({scale:Math.min(100/unit.width,140/unit.height)}),target=canvas.current;target.width=Math.ceil(viewport.width);target.height=Math.ceil(viewport.height);const context=target.getContext('2d');if(!context)return;task=p.render({canvas:target,canvasContext:context,viewport});return (task as ReturnType<typeof p.render>).promise;}).catch(()=>{});return()=>{active=false;task?.cancel();};},[pdf,page]);
 return <canvas ref={canvas} aria-label={`PDF 第 ${page} 页缩略图`}/>;
}
