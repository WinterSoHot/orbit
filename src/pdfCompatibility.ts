// PDF.js legacy omits these APIs on the macOS 13 WebView.
// Both reader and worker load core-js plus the narrow native Web API adapters.
import 'core-js/actual/promise/with-resolvers.js';
import 'core-js/actual/array-buffer/transfer-to-fixed-length.js';
import {streamValues} from './pdfStreamIterator';
if(typeof ReadableStream!=='undefined'&&!(ReadableStream.prototype as ReadableStream<unknown>&{[Symbol.asyncIterator]?:typeof streamValues})[Symbol.asyncIterator]) {
 Object.defineProperty(ReadableStream.prototype,Symbol.asyncIterator,{value:streamValues,writable:true,configurable:true});
}
import {combinedSignal} from './pdfAbortSignal';
if(typeof AbortSignal!=='undefined'&&!AbortSignal.any)Object.defineProperty(AbortSignal,'any',{value:combinedSignal,writable:true,configurable:true});
