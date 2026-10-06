// Browser-only fixture: exercise PDF/app styles in either load order without user data.
import React from 'react';
import {createRoot} from 'react-dom/client';
const pdfFirst=new URLSearchParams(location.search).has('pdf-first');
if(pdfFirst)await import('pdfjs-dist/web/pdf_viewer.css');
const {default:App}=await import('../src/App');
await import('../src/styles.css');
if(!pdfFirst)await import('pdfjs-dist/web/pdf_viewer.css');
createRoot(document.getElementById('root')!).render(<App/>);
