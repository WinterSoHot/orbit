// WK13 has native Streams/backpressure but lacks their async iterator.
export async function* streamValues<T>(this:ReadableStream<T>,options:{preventCancel?:boolean}={}) {
 const reader=this.getReader();let completed=false,failed=false;
 try {
  while(true){const result=await reader.read();if(result.done){completed=true;return;}yield result.value;}
 } catch(error){failed=true;throw error;}
 finally {try{if(!completed&&!failed&&!options.preventCancel)await reader.cancel();}finally{reader.releaseLock();}}
}
