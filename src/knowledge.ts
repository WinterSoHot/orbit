import {parseDocumentLink} from './pdfReaderState.ts';
import type {PdfReaderData} from './pdfReaderState.ts';
export type DocumentKind='markdown'|'pdf'|'web';
export interface DocumentVersion {id:string;content:string;at:number;reason:string}
export interface LibraryDocument {id:string;kind:DocumentKind;title:string;tags:string[];url:string|null;content:string;revision:number;createdAt:number;updatedAt:number;draft:{content:string;at:number}|null;versions:DocumentVersion[];blobId:string|null;sizeBytes:number;stamp:{sessionId:string;sequence:number}|null;collectionIds?:string[];deletedAt?:number|null;pdfReader?:PdfReaderData|null}
export interface SmartGroup {id:string;name:string;revision:number;kind:DocumentKind|null;tag:string;keyword:string;updatedDays:number|null}
export interface Collection {id:string;name:string;parentId:string|null;revision:number}
export interface Library {documents:LibraryDocument[];groups:SmartGroup[];collections:Collection[];schemaVersion:number;error:string|null}
export interface DocumentChange {documentId:string;expectedRevision:number;sessionId:string;sequence:number;operation:'draft'|'commit'|'automatic'|'restore'|'metadata';content:string;versionId:string|null;title?:string;tags?:string[]}
export const kindLabels:Record<DocumentKind,string>={markdown:'Markdown',pdf:'PDF',web:'网页'};
export function matchesGroup(doc:LibraryDocument,group:Partial<SmartGroup>,now=Date.now()):boolean {
 const text=`${doc.title} ${doc.tags.join(' ')} ${doc.content}`.toLocaleLowerCase();
 return (!group.kind||doc.kind===group.kind)&&(!group.tag||doc.tags.includes(group.tag))&&(!group.keyword||text.includes(group.keyword.toLocaleLowerCase()))&&(group.updatedDays==null||doc.updatedAt>=now-group.updatedDays*86400000);
}
export function linkedIds(content:string):string[] {return [...new Set([...content.matchAll(/orbit:\/\/document\/[^\s)<>]+/gi)].map(m=>parseDocumentLink(m[0])?.id).filter((id):id is string=>!!id))];}
export function documentLink(id:string):string{return `orbit://document/${id}`;}
export function backLinks(documents:LibraryDocument[],target:string):LibraryDocument[]{return documents.filter(doc=>doc.id!==target&&linkedIds(doc.content).includes(target));}

export function pdfRenderScale(width:number,height:number,wanted:number):number {
 if(![width,height,wanted].every(n=>Number.isFinite(n)&&n>0))throw new Error('PDF 页面尺寸无效');
 // Leave rounding headroom for ceil(width) * ceil(height), not just float area.
 const scale=Math.min(wanted,4095/width,4095/height,Math.sqrt(7990000)/Math.sqrt(width)/Math.sqrt(height));
 if(!Number.isFinite(scale)||scale<=0)throw new Error('PDF 页面尺寸超出预览范围');
 return scale;
}
