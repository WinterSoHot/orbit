// Only the combination behavior required by PDF.js; native signals do the aborting.
export function combinedSignal(signals:Iterable<AbortSignal>):AbortSignal {
 const sources=[...new Set(signals)];
 for(const signal of sources)if(!(signal instanceof AbortSignal))throw new TypeError('Expected AbortSignal');
 const controller=new AbortController();
 const aborted=sources.find(s=>s.aborted);if(aborted){controller.abort(aborted.reason);return controller.signal;}
 const cleanup=()=>sources.forEach(signal=>signal.removeEventListener('abort',abort));
 const abort=(event:Event)=>{cleanup();controller.abort((event.target as AbortSignal).reason);};
 sources.forEach(signal=>signal.addEventListener('abort',abort,{once:true}));
 return controller.signal;
}
