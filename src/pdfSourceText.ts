import './pdfCompatibility';
import workerUrl from './pdf.worker.ts?worker&url';
import {readPdf} from './bridge';

export async function extractPdfPages(documentId:string,pages:number[],signal:AbortSignal):Promise<string>{
 const data=await readPdf(documentId);if(signal.aborted)throw Error('已取消读取');
 const api=await import('pdfjs-dist/legacy/build/pdf.mjs');api.GlobalWorkerOptions.workerSrc=workerUrl;
 const loading=api.getDocument({data:Uint8Array.from(atob(data),c=>c.charCodeAt(0)),useWasm:false,useSystemFonts:true,cMapUrl:'/pdfjs/cmaps/',cMapPacked:true,standardFontDataUrl:'/pdfjs/standard_fonts/',disableAutoFetch:true,maxImageSize:8000000});
 const cancel=()=>{void loading.destroy().catch(()=>{});};signal.addEventListener('abort',cancel,{once:true});
 loading.onPassword=()=>{cancel();};
 try{
  const pdf=await loading.promise;if(signal.aborted)throw Error('已取消读取');if(pdf.numPages>10000||pages.some(p=>p>pdf.numPages))throw Error('页码超出此 PDF，请重新选择');
  const result:string[]=[];let used=0;
  for(const number of pages){
   if(signal.aborted)throw Error('已取消读取');const page=await pdf.getPage(number),content=await page.getTextContent();
   const lines:string[]=[];
   for(const item of content.items){if('str' in item){used+=[...item.str].length;if(used>32000)throw Error('所选页面超过 32000 字，请缩小范围');lines.push(item.str+(item.hasEOL?'\n':' '));}}
   const text=lines.join('').trim();page.cleanup();if(!text)throw Error(`第 ${number} 页没有可提取文字，请选择批注、备注或添加摘录`);
   result.push(`第 ${number} 页\n${text}`);
  }
  return result.join('\n\n');
 }finally{signal.removeEventListener('abort',cancel);await loading.destroy();}
}
