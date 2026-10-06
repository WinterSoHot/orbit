import {writeFileSync,mkdirSync} from 'node:fs';import {build} from 'vite';
// Own synthetic PDF: outline and a rotated second page; no user library is read.
const objects=[
'<< /Type /Catalog /Pages 2 0 R /Outlines 8 0 R >>',
'<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>',
'<< /Type /Page /Parent 2 0 R /MediaBox [10 20 490 640] /Resources << /Font << /F1 7 0 R >> >> /Contents 4 0 R >>',
null,
'<< /Type /Page /Parent 2 0 R /MediaBox [10 20 490 640] /Rotate 90 /Resources << /Font << /F1 7 0 R >> >> /Contents 6 0 R >>',
null,
'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
'<< /Type /Outlines /First 9 0 R /Last 9 0 R /Count 1 >>',
'<< /Title (Rotated chapter) /Parent 8 0 R /Dest [5 0 R /Fit] >>'];
for(const [i,text] of [[3,'Orbit reader first page'],[5,'Rotated second page text']]){const content=`BT /F1 20 Tf 40 550 Td (${text}) Tj ET`;objects[i]=`<< /Length ${content.length} >>\nstream\n${content}\nendstream`;}
let pdf='%PDF-1.4\n',offsets=[0];objects.forEach((o,i)=>{offsets.push(Buffer.byteLength(pdf));pdf+=`${i+1} 0 obj\n${o}\nendobj\n`;});const xref=Buffer.byteLength(pdf);pdf+=`xref\n0 ${objects.length+1}\n0000000000 65535 f \n${offsets.slice(1).map(o=>String(o).padStart(10,'0')+' 00000 n ').join('\n')}\ntrailer\n<< /Size ${objects.length+1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
writeFileSync('tests/pdf-fixture.json',JSON.stringify(Buffer.from(pdf).toString('base64')));
const outDir=process.argv[2]||'/private/tmp/orbit-pdf-reader-ui';mkdirSync(outDir,{recursive:true});
await build({build:{outDir,emptyOutDir:true,rollupOptions:{input:['tests/pdf-reader.html','tests/pdf-editor.html']}}});
