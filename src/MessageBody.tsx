import {useEffect,useId,useState} from 'react';
import {Copy,Check,ChevronDown,ChevronUp} from 'lucide-react';
import {MarkdownBody} from './ArtifactEditor';
import type {DocumentNavigator} from './SourceComposer';

export function MessageBody({content,markdown=false,onDocument}:{content:string;markdown?:boolean;onDocument?:DocumentNavigator}){
  const [expanded,setExpanded]=useState(false),[copied,setCopied]=useState(false),[error,setError]=useState('');
  useEffect(()=>{setCopied(false);setError('');},[content]);
  const id=useId(),long=content.length>1200||content.split('\n').length>16,folded=long&&!expanded;
  const shown=folded?content.slice(0,1200).split('\n').slice(0,16).join('\n'):content;
  async function copy(){setError('');setCopied(false);try{await navigator.clipboard.writeText(content);setCopied(true);}catch{setError('复制失败，请检查剪贴板权限');}}
  return <div className="message-body">
    <div id={id} className={`message-content ${folded?'folded':''}`}>{markdown?<MarkdownBody content={shown} inertLinks={folded} onDocument={onDocument}/>:<p>{shown}</p>}</div>
    <div className="message-tools">{long&&<button aria-expanded={expanded} aria-controls={id} onClick={()=>setExpanded(!expanded)}>{expanded?<ChevronUp size={13}/>:<ChevronDown size={13}/>} {expanded?'收起':'展开全部'}</button>}<button title="复制全文" aria-label="复制全文" onClick={()=>void copy()}>{copied?<Check size={13}/>:<Copy size={13}/>}<span>{copied?'已复制':'复制'}</span></button>{error&&<span role="alert">{error}</span>}</div>
  </div>;
}
