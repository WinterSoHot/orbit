import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createServer } from 'vite';

const server=await createServer({server:{middlewareMode:true,hmr:false,ws:false},optimizeDeps:{noDiscovery:true,include:[]},appType:'custom'});
try {
  const {TaskChat}=await server.ssrLoadModule('/src/TaskChat.tsx');
  const artifact={id:'v1',name:'v1.md',kind:'markdown',content:'# First version',createdAt:2};
  const task={id:'task',title:'Test',prompt:'Original goal',scene:'research',provider:'codex',status:'completed',archived:false,createdAt:0,startedAt:0,finishedAt:2,runId:'run',threadId:'thread',turnId:'turn',revision:10,phase:0,nodes:[],events:[],approvals:[],tokens:null,artifacts:[artifact],supplements:[{runId:'run',previousTurnId:'old',text:'More storage details',createdAt:1}],directions:[{id:'direction',runId:'old-run',turnId:'old-turn',text:'Offline first',status:'accepted',createdAt:0.5}]};
  const props={task,draft:'Update the comparison',busy:false,desktop:true,onChange:()=>{},onSend:()=>{},onOpen:()=>{}};
  const render=(overrides={})=>renderToStaticMarkup(React.createElement(TaskChat,{...props,...overrides}));
  const ready=render();
  assert(ready.includes('More storage details'));
  assert(ready.includes('Offline first'));
  assert(ready.includes('执行器已确认'));
  assert(ready.includes('First version'));
  assert(ready.includes('发送并继续交付'));
  assert(ready.includes('>打开</button>')&&!ready.includes('预览 / 编辑 / 导出'));
  assert(!ready.includes('disabled=""'));
  const archived=render({task:{...task,archived:true}});
  assert(!archived.includes('<textarea'));
  assert(!archived.includes('发送并继续交付'));
  assert(render({task:{...task,status:'running'}}).includes('disabled=""'));
  assert(render({desktop:false}).includes('disabled=""'));
  const failed=render({task:{...task,status:'failed',turnId:null,events:[{kind:'error',text:'thread thread already has an active writer'}]}});
  assert(failed.includes('already has an active writer'));
  assert(failed.includes('使用上次补充内容'));
  assert(render({task:{...task,supplements:[{...task.supplements[0],sourceThreadId:'source'}]}}).includes('已复制历史到续接分支'));
  const transcript=[{runId:'run',threadId:'thread',itemId:'progress',kind:'assistant',title:'',status:'completed',text:'Checking sources',finalAnswer:false,truncated:false,exitCode:null,at:1},{runId:'run',threadId:'thread',itemId:'command',kind:'tool',title:'执行命令',status:'completed',text:'',finalAnswer:false,truncated:false,exitCode:0,at:1.5},{runId:'run',threadId:'thread',itemId:'final',kind:'assistant',title:'',status:'completed',text:'# First version',finalAnswer:true,truncated:false,exitCode:null,at:2}];
  const native=render({task:{...task,conversation:transcript,artifacts:[{...artifact,id:'run-result'}]}});
  assert(native.includes('Checking sources')&&native.includes('执行命令')&&native.includes('退出码 0'));
  assert.equal((native.match(/First version/g)||[]).length,1);
  assert(render({task:{...task,conversation:transcript,conversationTruncated:true}}).includes('部分消息未收录'));
  assert(render({task:{...task,status:'running'},onStop:()=>{}}).includes('停止生成'));
  const {MessageBody}=await server.ssrLoadModule('/src/MessageBody.tsx');
  const message=text=>renderToStaticMarkup(React.createElement(MessageBody,{content:text,markdown:true}));
  assert(!message('a'.repeat(1200)).includes('展开全部'));
  const folded=message('a'.repeat(1201)+'[hidden](https://example.com) END');
  assert(folded.includes('展开全部'));assert(folded.includes('复制全文'));assert(folded.includes('aria-expanded="false"'));assert(!folded.includes(' END'));assert(!folded.includes('<a '));
  assert(message(Array.from({length:17},(_,i)=>`line${i}`).join('\n')).includes('展开全部'));
  const {TaskWorkspace}=await server.ssrLoadModule('/src/TaskWorkspace.tsx');
  const workspaceProps={tasks:[task,{...task,id:'backlog',status:'queued',artifacts:[],runId:null}],selected:task,queue:{paused:true,reason:'Restart requires review'},draft:'kept draft',busy:false,desktop:true,executors:[],onSelect:()=>{},onChange:()=>{},onSend:()=>{},onSteer:()=>{},onOpen:()=>{},onStart:()=>{},onCancel:()=>{},onAccept:()=>{},onPause:()=>{},onArchive:()=>{},onStop:()=>{},onSync:()=>{},onAnswer:()=>{}};
  const workspaceRender=(selected=task)=>renderToStaticMarkup(React.createElement(TaskWorkspace,{...workspaceProps,selected}));
  const workspace=workspaceRender();
  for(const label of ['待办','进行中','待介入','已完成','验收交付','恢复队列','Original goal','kept draft','任务更多操作'])assert(workspace.includes(label));
  assert(!workspace.includes('board-card-actions'));
  assert(!workspace.includes('场景'));
  assert(!workspace.includes('>调研<'));
  assert(!workspace.includes('>编程<'));
  assert(!workspace.includes('>写作<'));
  assert(!workspace.includes('class="task-context"'));
  assert(!workspace.includes('查看对话'));
  assert(workspaceRender({...task,status:'running',queue:null}).includes('中断'));
  assert(workspaceRender({...task,status:'unknown',queue:{requestId:'claimed',order:1,nextRunId:'run',error:null,state:'claimed',action:{kind:'start'}}}).includes('核对并撤销'));
  assert(workspaceRender({...task,queue:{requestId:'pending',order:1,nextRunId:'next',error:null,state:'pending',action:{kind:'start'}}}).includes('取消排队'));
  assert(render({task:{...task,status:'running'},onSteer:()=>{}}).includes('发送补充方向'));
  const {ExportDirectorySettings}=await server.ssrLoadModule('/src/ExportDirectorySettings.tsx');
  const folderProps={value:{directory:'/Users/test/交付 folder',custom:true},desktop:true,busy:false,onChoose:()=>{},onReset:()=>{},onExport:()=>{}};
  const folder=renderToStaticMarkup(React.createElement(ExportDirectorySettings,folderProps));
  assert(folder.includes('/Users/test/交付 folder'));
  assert(folder.includes('选择文件夹')&&folder.includes('恢复默认')&&folder.includes('自定义位置'));
  assert(folder.includes('导出全部数据'));
  const exportButton=markup=>[...markup.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(match=>match[0]).find(button=>button.includes('导出全部数据'));
  assert(!exportButton(folder).includes('disabled=""'));
  for(const guard of [{desktop:false},{busy:true},{value:null}]) {
    const guarded=renderToStaticMarkup(React.createElement(ExportDirectorySettings,{...folderProps,...guard}));
    assert(exportButton(guarded).includes('disabled=""'));
  }

  assert(!folder.includes('disabled=""'));
  assert(renderToStaticMarkup(React.createElement(ExportDirectorySettings,{...folderProps,desktop:false})).includes('disabled=""'));
  console.log('Export settings and chat render checks passed: saved path, browser guards, branch notice and retry draft.');
  console.log('TaskChat render checks passed: history, confirmations, archive and running/browser guards.');
  const {ExecutorPicker}=await server.ssrLoadModule('/src/ExecutorPicker.tsx');
  const {ExecutorSettings}=await server.ssrLoadModule('/src/ExecutorSettings.tsx');
  const executors=[{id:'codex',name:'Codex',description:'Code executor',permissionNote:'Read only',capabilities:{resume:true,steer:true,interrupt:true,agentHistory:true}}, {id:'qoder',name:'Qoder',description:'Qoder executor',permissionNote:'Restricted callbacks',capabilities:{resume:false,steer:false,interrupt:true,agentHistory:false}}, {id:'third',name:'Third executor',description:'Additional adapter',permissionNote:'Own policy',capabilities:{resume:false,steer:false,interrupt:true,agentHistory:false}}];
  const picker=renderToStaticMarkup(React.createElement(ExecutorPicker,{executors,value:'qoder',onChange:()=>{},model:'Qwen-test',onModelChange:()=>{},catalog:{models:[{id:'Qwen-test',name:'Qwen Test',description:'',isDefault:false}],loading:false,error:null},desktop:true,onLoad:()=>{}}));
  assert(picker.includes('Qoder')&&picker.includes('Third executor')&&picker.includes('checked=""')&&picker.includes('aria-pressed="true"'));
  assert(picker.includes('Qwen Test')&&!picker.includes('GPT Test'));
  const defaultPicker=renderToStaticMarkup(React.createElement(ExecutorPicker,{executors,value:'codex',onChange:()=>{},model:null,onModelChange:()=>{},desktop:true,onLoad:()=>{}}));
  assert((defaultPicker.match(/<input[^>]+>/g)??[]).some(tag=>tag.includes('name="executor-model"')&&tag.includes('value=""')&&tag.includes('checked=""')));
  const settings=renderToStaticMarkup(React.createElement(ExecutorSettings,{executors,doctors:{},checking:null,desktop:true,onCheck:()=>{},catalogs:{codex:{models:[{id:'gpt-test',name:'GPT Test',description:'',isDefault:true}],loading:false,error:null}},defaults:{codex:'gpt-test'},onModelChange:()=>{},onLoad:()=>{}}));
  assert(settings.includes('Qoder')&&settings.includes('Third executor')&&settings.includes('检查 Codex 连接')&&settings.includes('GPT Test')&&!settings.includes('Qwen Test'));
  const qoderTask={...task,provider:'qoder',status:'running',threadId:null,sessionRef:{provider:'qoder',protocol:'acp-v1',id:'s'},capabilities:executors[1].capabilities};
  const qoderChat=render({task:qoderTask,executorName:'Qoder'});
  assert(qoderChat.includes('Qoder 正在处理')&&!qoderChat.includes('Codex 正在处理'));
  assert(render({task:{...qoderTask,status:'completed'},executorName:'Qoder'}).includes('disabled=""'));
  console.log('Executor UI checks passed: catalog-driven third executor, Qoder label and capability guards.');
  const {MarkdownBody}=await server.ssrLoadModule('/src/ArtifactEditor.tsx');
  const link='orbit://document/00000000-0000-4000-8000-000000000002';
  const linked=renderToStaticMarkup(React.createElement(MarkdownBody,{content:`[Related document](${link})\n\n<img src="https://tracker.invalid/secret">\n\n![remote image](https://tracker.invalid/image)\n\n[unsafe](javascript:alert%281%29)`,onDocument:()=>{}}));
  assert(linked.includes('markdown-document-link')&&linked.includes('Related document'));
  assert(!linked.includes('<img')&&!linked.includes('tracker.invalid')&&!linked.includes('javascript:'));
  const stripped=renderToStaticMarkup(React.createElement(MarkdownBody,{content:`[Related document](${link})`}));
  assert(!stripped.includes('href="orbit:')&&!stripped.includes('markdown-document-link'));
  const {KnowledgeEditor}=await server.ssrLoadModule('/src/KnowledgeEditor.tsx');
  const doc={id:'00000000-0000-4000-8000-000000000001',kind:'markdown',title:'Research notes',tags:['research'],url:null,content:'# Committed body',revision:3,createdAt:1,updatedAt:2,draft:{content:`# Recovered visible draft\n\n[Related document](${link})`,at:3},versions:[{id:'snapshot',content:'Previous body',at:1,reason:'automatic'}],blobId:null,sizeBytes:0,stamp:null};
  const related={...doc,id:'00000000-0000-4000-8000-000000000002',title:'Related document',draft:null,content:''};
  const editor=renderToStaticMarkup(React.createElement(KnowledgeEditor,{document:doc,documents:[doc,related],onSaved:()=>{},onNavigate:()=>{},onNotice:()=>{}}));
  assert(editor.includes('Recovered visible draft')&&!editor.includes('Committed body'));
  assert(editor.includes('已恢复上次草稿')&&editor.includes('版本历史')&&editor.includes('Related document'));
  assert(editor.includes('aria-label="导出文档" disabled=""'));
  const trashEditor=renderToStaticMarkup(React.createElement(KnowledgeEditor,{document:{...doc,deletedAt:5},documents:[doc,related],onSaved:()=>{},onNavigate:()=>{},onNotice:()=>{},onLifecycle:()=>{},onOrganize:()=>{}}));
  assert(trashEditor.includes('回收站 · 只读'));
  assert(trashEditor.includes('恢复文档')&&trashEditor.includes('永久删除'));
  assert(!trashEditor.includes('aria-label="移入回收站"'));
  assert(trashEditor.includes('Recovered visible draft'));
  const {KnowledgeLibrary}=await server.ssrLoadModule('/src/KnowledgeLibrary.tsx');
  const library=renderToStaticMarkup(React.createElement(KnowledgeLibrary,{onNotice:()=>{}}));
  assert(library.includes('PDF 检索标题、标签和备注')&&library.includes('智能分组')&&library.includes('在 Orbit 桌面 App'));
  for(const label of ['导入文件','保存网页','新建 Markdown']) {
    const button=[...library.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(m=>m[0]).find(b=>b.includes(label));
    assert(button?.includes('disabled=""'));
  }
  assert(library.includes('回收站')&&library.includes('未分类')&&library.includes('分类文件夹')&&library.includes('存储位置'));
  console.log('Knowledge render checks passed: recovered drafts, internal links, inert Markdown and browser mutation guards.');
  const {default:App}=await server.ssrLoadModule('/src/App.tsx');
  const originalWindow=globalThis.window;
  try {
    const app=()=>renderToStaticMarkup(React.createElement(App));
    const expanded=app();
    assert(expanded.includes('aria-label="收起侧边栏"')&&expanded.includes('aria-controls="orbit-navigation"'));
    globalThis.window={localStorage:{getItem:key=>key==='orbit.sidebarCollapsed'?'true':null},matchMedia:()=>({matches:false})};
    const collapsed=app();
    assert(collapsed.includes('sidebar-collapsed')&&collapsed.includes('aria-label="展开侧边栏"'));
    for(const label of ['新建任务','任务工作台','待介入','归档','交付物','知识库','设置'])assert(collapsed.includes(`aria-label="${label}"`));
    globalThis.window.localStorage.getItem=()=>{throw Error('Storage unavailable');};
    assert(app().includes('aria-label="收起侧边栏"'));
  } finally {if(originalWindow===undefined)delete globalThis.window;else globalThis.window=originalWindow;}
  console.log('Sidebar render checks passed: persisted icon mode, accessible labels and storage fallback.');
  const {AttentionPanel}=await server.ssrLoadModule('/src/AttentionPanel.tsx');
  const {boardColumn}=await server.ssrLoadModule('/src/model.ts');
  const approval={id:'a',title:'Confirm scope',description:'Scope details',kind:'input',questionIds:['scope'],runId:'run',turnId:'turn',requestId:'request'};
  const pending={requestId:'queued',order:1,nextRunId:'next',state:'pending',action:{kind:'start'},error:null};
  const cases=[
    {...task,title:'Needs review',status:'unknown'},
    {...task,title:'Delivery waiting'},
    {...task,title:'Two requests',status:'approval',approvals:[approval,{...approval,id:'b',title:'Confirm budget'}]},
    {...task,title:'Claimed queue',queue:{...pending,state:'claimed'}},
    {...task,title:'Queue error',queue:{...pending,error:'Start failed'}},
    ...['failed','interrupted'].map(status=>({...task,title:status,status})),
    {...task,title:'No delivery',artifacts:[]},
    {...task,title:'Archived',archived:true},
    {...task,title:'Accepted',acceptance:{runId:task.runId,turnId:task.turnId,artifactIds:['v1']}},
    {...task,title:'Normal queue',queue:pending},
  ];
  const attention=cases.filter(t=>!t.archived&&boardColumn(t)==='attention');
  assert.equal(attention.length,8);
  const attentionRender=(overrides={})=>renderToStaticMarkup(React.createElement(AttentionPanel,{tasks:attention,executors,desktop:true,busy:false,onOpen:()=>{},onAnswer:()=>{},...overrides}));
  const attentionMarkup=attentionRender();
  for(const label of ['Needs review','Delivery waiting','Confirm scope','Confirm budget','启动待核对','排队需核对','交付待验收','No delivery'])assert(attentionMarkup.includes(label));
  for(const label of ['Archived','Accepted','Normal queue'])assert(!attentionMarkup.includes(label));
  assert.equal((attentionMarkup.match(/<button\b[^>]*>打开任务/g)||[]).length,8);
  assert.equal((attentionMarkup.match(/<form /g)||[]).length,2);
  const answeredTask={...cases[2],approvals:[{...approval,questionIds:[]}]};
  assert(!attentionRender({tasks:[answeredTask]}).includes('disabled=""'));
  for(const guard of [{desktop:false},{busy:true},{tasks:[{...answeredTask,capabilities:{input:false}}]}])assert(attentionRender({tasks:[answeredTask],...guard}).includes('disabled=""'));
  assert(attentionRender({tasks:[]}).includes('暂时不需要你介入'));
  console.log('Attention checks passed: board classification, task counts, all requests and answer guards.');
  const {RealApprovalForm}=await server.ssrLoadModule('/src/ApprovalForm.tsx');
  const card={...approval,questions:[{id:'scope',header:'Scope',question:'Local or cloud?',isOther:true,options:[{label:'Local',description:'Offline'},{label:'Cloud',description:'Online'}]}]};
  const cardRender=(overrides={})=>renderToStaticMarkup(React.createElement(RealApprovalForm,{approval:card,busy:false,onAnswer:()=>{},...overrides}));
  const cardMarkup=cardRender();
  for(const label of ['Local or cloud?','Local','Cloud','Offline','其他'])assert(cardMarkup.includes(label));
  assert(cardMarkup.includes('<fieldset')&&cardMarkup.includes('type="radio"')&&!cardMarkup.includes('checked=""')&&cardMarkup.includes('disabled=""'));
  assert(cardRender({busy:true}).includes('<fieldset disabled=""'));
  assert(cardRender({approval:{...card,questionError:'Malformed request'}}).includes('role="alert"'));
  assert(cardRender({approval:{...approval,questions:[{id:'scope',question:'Secret',isSecret:true}]}}).includes('type="password"'));
  assert(cardRender({approval}).includes('aria-label="答复 1"'));
  for(const id of ['constructor','toString','__proto__'])assert(cardRender({approval:{...approval,questionIds:[id],questions:[{id,question:'Reserved ID'}]}}).includes('Reserved ID'));
  console.log('Clarification card checks passed: no default selection, legacy text, secrets and error guards.');
} finally { await server.close(); }
