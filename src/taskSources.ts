export interface SourceRequest {documentId:string;revision:number;mode:'excerpt'|'pages'|'annotation';text:string;pages:number[];readerRevision:number|null;annotationId:string|null}
export interface SourceSnapshot {id:string;documentId:string;revision:number;readerRevision:number|null;title:string;kind:string;url:string|null;text:string;pages:number[];annotationId:string|null}
export interface SourceInput {id:string;kind:'template'|'initial'|'continue'|'direction';text:string;sources:SourceSnapshot[];runId:string|null;turnId:string|null;status:string;createdAt:number}
export function parsePages(value:string):number[]{
 const pages=new Set<number>();
 for(const part of value.split(',').map(s=>s.trim())) {
  const m=part.match(/^([1-9]\d*)(?:\s*-\s*([1-9]\d*))?$/);if(!m)throw Error('页码格式如 1, 3-5');
  const first=Number(m[1]),last=Number(m[2]||m[1]);if(last<first||last>10000||last-first>=10)throw Error('请选择有效页码，每次最多 10 页');
  for(let page=first;page<=last;page++)pages.add(page);if(pages.size>10)throw Error('每次最多引用 10 个 PDF 页面');
 }return [...pages].sort((a,b)=>a-b);
}
export function checkSources(sources:SourceRequest[]):void {
 if(sources.length>8||sources.reduce((n,s)=>n+[...s.text].length,0)>32000||sources.reduce((n,s)=>n+s.pages.length,0)>10)throw Error('每次最多 8 项资料、10 个 PDF 页面、32000 字');
 if(sources.some(s=>!s.text.trim()||s.pages.some(p=>!Number.isInteger(p)||p<1||p>10000)))throw Error('资料没有可发送的文字，请选择批注、备注或粘贴摘录');
}
