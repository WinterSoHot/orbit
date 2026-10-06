import {parseDocumentLink} from './pdfReaderState';
import type {DocumentLocation} from './pdfReaderState';
import { useEffect, useRef, useState } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ArrowDownToLine, Code2, Eye, FileText, Loader2, Save, BookOpen, X } from 'lucide-react';
import type { AgentNode, Artifact } from './model';

export function ArtifactEditor({artifact,readOnly=false,onClose,onSave,onDownload,onCollect}:{artifact:Artifact;readOnly?:boolean;onClose:()=>void;onSave:(expected:string,content:string)=>Promise<Artifact>;onDownload:(file:Artifact)=>void;onCollect?:(file:Artifact)=>void}) {
  const [saved,setSaved]=useState(artifact),[draft,setDraft]=useState(artifact.content);
  const [mode,setMode]=useState<'preview'|'edit'>('preview'),[saving,setSaving]=useState(false),[error,setError]=useState(''),[confirm,setConfirm]=useState(false);
  const pending=useRef(false),dialog=useRef<HTMLElement>(null);
  const dirty=draft!==saved.content;
  function close(){if(pending.current)return;if(dirty)setConfirm(true);else onClose();}
  async function save(closeAfter=false){
    if(pending.current||readOnly)return;
    if(!dirty){if(closeAfter)onClose();return;}
    pending.current=true;setSaving(true);setError('');
    try{const result=await onSave(saved.content,draft);setSaved(result);setDraft(result.content);setConfirm(false);if(closeAfter)onClose();}
    catch(reason){setError(reason instanceof Error?reason.message:String(reason));}
    finally{pending.current=false;setSaving(false);}
  }
  useEffect(()=>{
    const previous=document.activeElement instanceof HTMLElement?document.activeElement:null;
    dialog.current?.querySelector<HTMLButtonElement>('button')?.focus();
    return()=>previous?.focus();
  },[]);
  useEffect(()=>{
    const key=(e:KeyboardEvent)=>{
      if(e.key==='Tab'){
        const controls=Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled),a[href],textarea:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex="0"]')||[]).filter(el=>el.offsetParent!==null);
        const first=controls[0],last=controls[controls.length-1];
        if(!first){e.preventDefault();return;}
        if(!dialog.current?.contains(document.activeElement)||(e.shiftKey&&document.activeElement===first)||(!e.shiftKey&&document.activeElement===last)){
          e.preventDefault();(e.shiftKey?last:first).focus();
        }
      }
      if(e.key==='Escape'){e.preventDefault();e.stopPropagation();if(confirm)setConfirm(false);else close();}if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='s'){e.preventDefault();void save();}};
    window.addEventListener('keydown',key,true);return()=>window.removeEventListener('keydown',key,true);
  });
  return <div className="modal-backdrop" onClick={close}>
    <section ref={dialog} className="document-modal" role="dialog" aria-modal="true" aria-label="Markdown 文档" onClick={e=>e.stopPropagation()}>
      <div className="modal-heading"><div><FileText size={19}/><h2>{artifact.name}</h2></div><button className="icon-button" aria-label="关闭文档" disabled={saving} onClick={close}><X size={19}/></button></div>
      <div className="document-toolbar">{!readOnly&&<div className="document-tabs" role="tablist" aria-label="文档模式"><button role="tab" aria-selected={mode==='preview'} onClick={()=>setMode('preview')}><Eye size={15}/>预览</button><button role="tab" aria-selected={mode==='edit'} onClick={()=>setMode('edit')}><Code2 size={15}/>编辑</button></div>}<span className={dirty?'document-dirty':''}>{readOnly?'归档交付 · 只读':saving?'正在保存…':dirty?'有未保存修改':'已保存版本'}</span>{!readOnly&&<button className="secondary-button compact" disabled={!dirty||saving} onClick={()=>void save()}>{saving?<Loader2 size={14} className="spin"/>:<Save size={14}/>}保存</button>}</div>
      {mode==='edit'?<textarea className="document-source" aria-label="Markdown 源码" spellCheck={false} value={draft} disabled={saving} onChange={e=>setDraft(e.target.value)}/>:<MarkdownBody content={draft}/>}
      {error&&<div className="document-error" role="alert">{error}</div>}
      {confirm&&<div className="document-close-confirm" role="alertdialog" aria-label="未保存修改"><div><strong>保存修改后关闭？</strong><span>你的修改尚未保存到本地。</span></div><button className="secondary-button compact" disabled={saving} onClick={()=>setConfirm(false)}>继续编辑</button><button className="secondary-button compact" disabled={saving} onClick={onClose}>丢弃并关闭</button><button className="primary-button compact" disabled={saving} onClick={()=>void save(true)}>保存并关闭</button></div>}
      <div className="modal-footer"><span>{readOnly?'Markdown · 归档交付':'Markdown · ⌘ / Ctrl S 保存'}</span>{onCollect&&<button className="secondary-button" disabled={dirty||saving} onClick={()=>onCollect(saved)}><BookOpen size={15}/>存入知识库</button>}<button className="primary-button" disabled={dirty||saving} onClick={()=>onDownload(saved)}><ArrowDownToLine size={15}/>下载文件</button></div>
    </section>
  </div>;
}

export function MarkdownBody({content,onDocument,inertLinks=false}:{inertLinks?:boolean;content:string;onDocument?:(id:string,location?:DocumentLocation)=>void}) {
 const target=parseDocumentLink;
 return <article className="markdown-body" aria-label="Markdown 预览"><Markdown skipHtml remarkPlugins={[remarkGfm]} urlTransform={url=>/^https?:\/\//i.test(url)||url.startsWith('#')||(onDocument&&target(url))?url:''} components={{img:({alt})=><span className="markdown-image">[图片：{alt||'未命名'}]</span>,a:({href,children})=>inertLinks?<span>{children}</span>:href&&target(href)&&onDocument?<button className="markdown-document-link" onClick={()=>onDocument(target(href)!.id,target(href)!)}>{children}</button>:href?<a href={href} target={href.startsWith('#')?undefined:'_blank'} rel="noopener noreferrer">{children}</a>:<span>{children}</span>}}>{content}</Markdown></article>;
}

export function AgentOutput({node,onClose}:{node:AgentNode;onClose:()=>void}) {
  const dialog=useRef<HTMLElement>(null);
  useEffect(()=>{
    const previous=document.activeElement instanceof HTMLElement?document.activeElement:null;
    dialog.current?.querySelector<HTMLButtonElement>('button')?.focus();
    const key=(event:KeyboardEvent)=>{
      if(event.key==='Escape'){event.preventDefault();event.stopPropagation();onClose();}
      if(event.key==='Tab'){
        const controls=Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled),a[href]')||[]).filter(el=>el.offsetParent!==null);
        const first=controls[0],last=controls[controls.length-1];
        if(!first){event.preventDefault();return;}
        if(!dialog.current?.contains(document.activeElement)||(event.shiftKey&&document.activeElement===first)||(!event.shiftKey&&document.activeElement===last)){
          event.preventDefault();(event.shiftKey?last:first).focus();
        }
      }
    };
    window.addEventListener('keydown',key,true);
    return()=>{window.removeEventListener('keydown',key,true);previous?.focus();};
  },[]);
  return <div className="modal-backdrop" onClick={onClose}><section ref={dialog} className="document-modal agent-output-modal" role="dialog" aria-modal="true" aria-label={`${node.name} 输出`} onClick={e=>e.stopPropagation()}>
    <div className="modal-heading"><div><FileText size={19}/><h2>{node.name} 输出</h2></div><button className="icon-button" aria-label="关闭 Agent 输出" onClick={onClose}><X size={19}/></button></div>
    <div className="document-toolbar"><span>{node.detailTurnCount?`子会话历史 · ${node.detailTurnCount} 轮输出`:'只读输出'}</span>{node.agentPath&&<code>{node.agentPath}</code>}</div>
    {node.detailNotice&&<div className="agent-output-notice" role="status">{node.detailNotice}</div>}
    <MarkdownBody content={node.output}/>
    <div className="modal-footer"><span>{node.outputTruncated?'输出已截断':'来自 Agent 的文本输出'} · 不影响交付文档</span><button className="secondary-button" onClick={onClose}>关闭</button></div>
  </section></div>;
}
