import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {createPortal} from 'react-dom';
import {Bot, CheckCircle2, FileText, MessageCircle, MoreHorizontal, PanelRight, ListFilter, Plus, Pause, Play, Search, Square, X} from 'lucide-react';
import {boardColumn,isAccepted,isActive,isTerminal,statusLabels,taskCapabilities} from './model';
import type {Task,Artifact,BoardColumn} from './model';
import type {QueueState} from './bridge';
import type {ExecutorDescriptor} from './executors';
import {executorName} from './executors';
import {TaskChat} from './TaskChat';
import {RealApprovalForm} from './ApprovalForm';
import {Graph} from './Graph';
import {MessageBody} from './MessageBody';

type Props={shortcutsEnabled?:boolean;tasks:Task[];selected?:Task;queue:QueueState;draft:string;busy:boolean;desktop:boolean;executors:ExecutorDescriptor[];onDuplicate?:(t:Task)=>void;onNew?:()=>void;onSelect:(t:Task)=>void;onChange:(s:string)=>void;onSend:()=>void;onSteer:()=>void;onOpen:(a:Artifact)=>void;onStart:(t:Task)=>void;onCancel:(t:Task)=>void;onAccept:(t:Task)=>void;onPause:(paused:boolean)=>void;onArchive:(t:Task)=>void;onStop:(t:Task)=>void;onSync:()=>void;onAnswer:(t:Task,id:string,answers:Record<string,string>)=>void};
const columns:{id:BoardColumn;label:string;hint:string}[]=[{id:'backlog',label:'待办',hint:'准备好后启动'},{id:'progress',label:'进行中',hint:'按顺序执行'},{id:'attention',label:'待介入',hint:'答复、核对或验收'},{id:'done',label:'已完成',hint:'已验收的交付'}];
export function TaskWorkspace(p:Props){
  const [collapsed,setCollapsed]=useState(false),[tab,setTab]=useState<'delivery'|'agents'|'logs'>('agents'),[context,setContext]=useState(false),[nodeId,setNodeId]=useState<string|null>(null),[drag,setDrag]=useState<string|null>(null),[notice,setNotice]=useState(''),[search,setSearch]=useState('');
  const searchRef=useRef<HTMLInputElement>(null),[searchRequest,setSearchRequest]=useState(0);
  useEffect(()=>{if(p.shortcutsEnabled===false)return;const key=(event:KeyboardEvent)=>{if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==='k'){event.preventDefault();setContext(false);setCollapsed(false);setSearchRequest(n=>n+1);}};window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);},[p.shortcutsEnabled]);
  useEffect(()=>{if(searchRequest&&!collapsed&&p.shortcutsEnabled!==false){const input=searchRef.current;const filter=input?.closest('details');if(filter)filter.open=true;input?.focus();}},[searchRequest,collapsed,p.shortcutsEnabled]);
  const [menu,setMenu]=useState<{id:string;x:number;y:number}|null>(null),menuRef=useRef<HTMLDivElement>(null),originRef=useRef<HTMLElement|null>(null);
  const menuTask=menu?p.tasks.find(t=>t.id===menu.id):undefined;
  function closeMenu(){setMenu(null);originRef.current?.focus();}
  useEffect(()=>{if(!menu)return;const key=(e:KeyboardEvent)=>{if(e.key==='Escape'){e.preventDefault();closeMenu();}};const outside=(e:PointerEvent)=>{if(!menuRef.current?.contains(e.target as Node))setMenu(null);};const scroll=(e:Event)=>{if(!(e.target instanceof Node)||!menuRef.current?.contains(e.target))setMenu(null);};window.addEventListener('keydown',key);window.addEventListener('pointerdown',outside);window.addEventListener('scroll',scroll,true);window.addEventListener('resize',scroll);return()=>{window.removeEventListener('keydown',key);window.removeEventListener('pointerdown',outside);window.removeEventListener('scroll',scroll,true);window.removeEventListener('resize',scroll);};},[menu]);
  useLayoutEffect(()=>{if(!menu||!menuRef.current)return;const box=menuRef.current.getBoundingClientRect();menuRef.current.style.left=`${Math.max(8,Math.min(menu.x,window.innerWidth-box.width-8))}px`;menuRef.current.style.top=`${Math.max(8,Math.min(menu.y,window.innerHeight-box.height-8))}px`;menuRef.current.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();},[menu?.id,menu?.x,menu?.y]);
  function openMenu(t:Task,x:number,y:number,origin:HTMLElement){originRef.current=origin;setMenu({id:t.id,x,y});}
  async function copyTask(t:Task){try{await navigator.clipboard.writeText(`${t.title}\n\n${t.prompt}`);setNotice('任务描述已复制');}catch{setNotice('复制失败，请检查剪贴板权限');}}
  const task=p.selected,node=task?.nodes.find(n=>n.id===nodeId)||task?.nodes[0];
  const showingDetails=!!task&&context,backRef=useRef<HTMLButtonElement>(null),boardRef=useRef<HTMLDivElement>(null),wasDetails=useRef(false);
  useEffect(()=>{setNodeId(null);},[task?.id]);
  useLayoutEffect(()=>{if(showingDetails)backRef.current?.focus();else if(wasDetails.current)(boardRef.current?.querySelector<HTMLButtonElement>('.board-card-select[aria-pressed=true]')||boardRef.current)?.focus();wasDetails.current=showingDetails;},[showingDetails]);
  const waiting=p.tasks.filter(t=>t.queue?.state==='pending').sort((a,b)=>a.queue!.order-b.queue!.order);
  const queuePosition=(t:Task)=>waiting.findIndex(q=>q.id===t.id)+1;
  const canAccept=(t:Task)=>!t.archived&&!t.queue&&t.status==='completed'&&t.artifacts.length>0&&!isAccepted(t);
  function drop(id:string,column:BoardColumn){
    const t=p.tasks.find(t=>t.id===id);setDrag(null);if(!t||!p.desktop||p.busy)return;
    if(boardColumn(t)===column)return;
    if(column==='done'&&canAccept(t))p.onAccept(t);
    else if(column==='progress'&&!t.queue&&!isActive(t.status)&&t.status==='queued')p.onStart(t);
    else if(column==='backlog'&&t.queue?.state==='pending'&&t.status==='queued')p.onCancel(t);
    else setNotice('该移动需要其他操作：启动请排队，继续交付请在对话中补充，完成请先验收。');
  }
  function taskActions(t:Task,full=false){return <>
    {full&&<button role="menuitem" onClick={()=>{p.onSelect(t);setContext(false);}}>查看对话</button>}
    {!t.queue&&!isActive(t.status)&&(full||t.status!=='queued')&&<button role="menuitem" disabled={!p.desktop||p.busy||!p.executors.some(e=>e.id===t.provider)} onClick={()=>p.onStart(t)}>{t.status==='queued'?'启动任务':'重新开始'}</button>}
    {t.queue&&(!isActive(t.status)||t.queue.state==='claimed')&&<button role="menuitem" disabled={!p.desktop||p.busy} onClick={()=>p.onCancel(t)}>{t.queue.state==='claimed'?'核对并撤销':'取消排队'}</button>}
    {(full&&isActive(t.status)||t.queue?.state==='claimed'&&!isActive(t.status))&&<button role="menuitem" disabled={!p.desktop||p.busy||t.status==='cancelling'||!taskCapabilities(t).interrupt} onClick={()=>p.onStop(t)}>{isActive(t.status)?'中断任务':'中断并核对'}</button>}
    {full&&canAccept(t)&&<button role="menuitem" disabled={!p.desktop||p.busy} onClick={()=>p.onAccept(t)}>验收交付</button>}
    {isAccepted(t)&&!t.queue&&<button role="menuitem" disabled={!p.desktop||p.busy} onClick={()=>p.onArchive(t)}>归档任务</button>}
    {p.onDuplicate&&<button role="menuitem" disabled={!p.desktop||p.busy} onClick={()=>p.onDuplicate?.(t)}>复制为新任务</button>}
    <button role="menuitem" onClick={()=>void copyTask(t)}>复制任务描述</button>
    <button role="menuitem" onClick={()=>{p.onSelect(t);setTab('agents');setContext(true);setCollapsed(false);}}>查看 Agent</button><button role="menuitem" onClick={()=>{p.onSelect(t);setTab('logs');setContext(true);setCollapsed(false);}}>运行记录</button>
  </>;}
  return <section className={`task-workspace ${collapsed?'board-collapsed':''}`} aria-label="任务工作台">
    {p.queue.paused&&p.queue.reason&&<div className="workflow-notice" role="status">{p.queue.reason}</div>}
    {notice&&<div className="workflow-notice" role="status">{notice}<button className="icon-button" aria-label="关闭移动提示" onClick={()=>setNotice('')}><X size={13}/></button></div>}
    <div className="workflow-layout">
      <aside className="conversation-pane">{task?<><div className="conversation-title"><div className="conversation-identity"><h2>{task.title}</h2><span>{executorName(task.provider,p.executors)}{task.requestedModel&&<small title="请求模型"> · {task.requestedModel}</small>}</span></div><div className="conversation-actions">
          {!task.queue&&task.status==='queued'&&<button className="primary-button compact" disabled={!p.desktop||p.busy||!p.executors.some(e=>e.id===task.provider)} onClick={()=>p.onStart(task)}><Play size={13}/>启动</button>}
          {isActive(task.status)&&<button className="secondary-button compact" disabled={!p.desktop||p.busy||task.status==='cancelling'||!taskCapabilities(task).interrupt} onClick={()=>p.onStop(task)}><Square size={12}/>中断</button>}
          {canAccept(task)&&<button className="primary-button compact" aria-label="验收交付" disabled={!p.desktop||p.busy} onClick={()=>p.onAccept(task)}><CheckCircle2 size={13}/>验收</button>}
          <details className="workspace-menu"><summary aria-label="任务更多操作" title="更多操作"><MoreHorizontal size={18}/></summary><div className="workspace-menu-items" onClick={event=>{if((event.target as HTMLElement).closest('button:not(:disabled)'))event.currentTarget.parentElement?.removeAttribute('open');}}>
            {taskActions(task)}
          </div></details>
          {collapsed&&<button className="icon-button" aria-label="展开看板" title="展开看板" onClick={()=>setCollapsed(false)}><PanelRight size={17}/></button>}
        </div></div>
        <TaskChat compact task={task} executorName={executorName(task.provider,p.executors)} registered={p.executors.some(e=>e.id===task.provider)} draft={p.draft} busy={p.busy} desktop={p.desktop} onChange={p.onChange} onSend={p.onSend} onSteer={p.onSteer} onStop={()=>p.onStop(task)} onOpen={p.onOpen}>{task.approvals.map(a=><div className="chat-approval" key={a.id}><strong>{a.title}</strong>{!a.questions?.length&&!a.questionError&&<p>{a.description}</p>}<RealApprovalForm approval={a} busy={p.busy||!p.desktop||!taskCapabilities(task).input} onAnswer={answers=>p.onAnswer(task,a.id,answers)}/></div>)}</TaskChat>
      </>:<div className="workspace-empty"><MessageCircle size={24}/><h3>选择一个任务</h3>{collapsed&&<button className="secondary-button" onClick={()=>setCollapsed(false)}>展开看板</button>}<p>从看板选择任务，或开始新的目标。</p>{p.onNew&&<button className="primary-button" onClick={p.onNew}><Plus size={14}/>新建任务</button>}</div>}</aside>
      {!collapsed&&<div className="board-pane"><div className="board-heading"><h2 title={showingDetails?task?.title:undefined}>{showingDetails?task?.title:'看板'}</h2>{showingDetails?<button ref={backRef} className="secondary-button compact" aria-label="返回看板" onClick={()=>setContext(false)}>返回看板</button>:<div className="board-controls">
        <span className={`queue-indicator ${p.queue.paused?'paused':''}`} title={p.queue.paused?'队列已暂停':`${waiting.length} 个等待`}/>
        <details className="workspace-menu board-filter"><summary aria-label="搜索和筛选任务" title="搜索与筛选"><ListFilter size={17}/></summary><div className="workspace-menu-items"><label className="board-search"><Search size={14}/><input ref={searchRef} aria-label="搜索任务" placeholder="搜索任务…" value={search} onChange={e=>setSearch(e.target.value)}/></label></div></details>
        <button className="icon-button" aria-label={p.queue.paused?'恢复队列':'暂停队列'} title={p.queue.paused?'恢复队列':`暂停队列 · ${waiting.length} 个等待`} disabled={!p.desktop||p.busy||(!p.queue.paused&&!waiting.length)||(p.queue.paused&&p.tasks.some(t=>t.queue?.state==='claimed'))} onClick={()=>p.onPause(!p.queue.paused)}>{p.queue.paused?<Play size={16}/>:<Pause size={16}/>}</button>
        <button className="icon-button" aria-label="收起看板" title="收起看板" onClick={()=>setCollapsed(true)}><PanelRight size={17}/></button>
      </div>}</div>
        <div ref={boardRef} tabIndex={-1} hidden={showingDetails} className="kanban-board" aria-label="四列任务看板">{columns.map(column=>{const items=p.tasks.filter(t=>boardColumn(t)===column.id&&(!search||`${t.title} ${t.prompt}`.toLowerCase().includes(search.toLowerCase())));return <section key={column.id} className={`kanban-column ${column.id} ${drag?'drop-ready':''}`} aria-label={column.label} onDragOver={event=>{if(drag){event.preventDefault();event.dataTransfer.dropEffect='move';}}} onDrop={event=>{event.preventDefault();drop(event.dataTransfer.getData('application/x-orbit-task')||drag||'',column.id);}}><div className="column-heading"><h3><i/>{column.label}</h3><span>{items.length}</span></div><div className="column-cards">{items.map(t=><article key={t.id} onContextMenu={event=>{event.preventDefault();openMenu(t,event.clientX,event.clientY,event.currentTarget.querySelector('button')!);}} onKeyDown={event=>{if(event.key==='ContextMenu'||event.shiftKey&&event.key==='F10'){event.preventDefault();const box=event.currentTarget.getBoundingClientRect();openMenu(t,box.left+10,box.top+10,event.currentTarget.querySelector('button')!);}}} className={`board-card ${task?.id===t.id?'selected':''}`} draggable={p.desktop&&!p.busy&&!t.archived} onDragStart={event=>{event.dataTransfer.setData('application/x-orbit-task',t.id);event.dataTransfer.effectAllowed='move';setDrag(t.id);}} onDragEnd={()=>setDrag(null)}>
          <button className="board-card-select" aria-pressed={task?.id===t.id} onClick={()=>{p.onSelect(t);setNotice('');setNodeId(null);setContext(false);}}><span className="board-card-meta"><span>{executorName(t.provider,p.executors)}</span></span><h4>{t.title}</h4><span className="board-card-state"><span className={`status-dot ${t.status}`}/>{t.queue?(t.queue.state==='claimed'?'启动待核对':t.queue.error?'排队需核对':`排队第 ${queuePosition(t)} 个`):isAccepted(t)?'已验收':t.status==='completed'&&t.artifacts.length?'交付待验收':statusLabels[t.status]}{!!t.nodes.length&&<span><Bot size={12}/>{t.nodes.length}</span>}</span></button>
        </article>)}{!items.length&&<div className="column-empty">暂无任务</div>}</div></section>})}</div>
        {task&&context&&<section className="task-context" aria-label={`${task.title}详情`}><div className="context-toolbar"><div className="context-tabs" role="tablist" aria-label="任务详情">{([['delivery','交付'],['agents','Agent'],['logs','运行记录']] as const).map(([id,label])=><button key={id} role="tab" aria-selected={tab===id} onClick={()=>setTab(id)}>{label}{id==='delivery'&&<span>{task.artifacts.length}</span>}{id==='agents'&&<span>{task.nodes.length}</span>}</button>)}</div></div>
          <div role="tabpanel" className="context-body">{tab==='delivery'&&<>{isAccepted(task)&&<p className="acceptance-note"><CheckCircle2 size={14}/>最新交付已验收。修改正文或继续交付后，需要重新验收。</p>}{task.artifacts.map((a,i)=><button className="context-artifact" key={a.id} onClick={()=>p.onOpen(a)}><FileText size={17}/><span><strong>交付版本 {i+1}</strong><small>{a.name}</small></span><span>打开文档</span></button>)}{!task.artifacts.length&&<p className="context-empty">交付生成后可在这里预览、编辑与导出。</p>}</>}
          {tab==='logs'&&<div className="context-logs">{task.events.slice().reverse().map(e=><div key={e.id}><time>{new Date(e.at).toLocaleTimeString('zh-CN')}</time><strong>{e.agent}</strong><span>{e.text}</span></div>)}{!task.events.length&&<p className="context-empty">排队启动后显示执行记录。</p>}</div>}
          {tab==='agents'&&<><div className="agent-context-toolbar"><span>{taskCapabilities(task).agentHistory?'实际观测到的协作关系':'此执行器暂未开放子 Agent 历史'}</span>{isTerminal(task.status)&&taskCapabilities(task).agentHistory&&task.threadId&&<button className="secondary-button compact" disabled={!p.desktop||p.busy||!!task.queue} onClick={p.onSync}>同步协作</button>}</div><Graph nodes={task.nodes} selected={node?.id||null} onSelect={setNodeId}/>{node&&<div className="agent-context-output"><strong>{node.name}</strong><span>{node.role} · {node.model}</span><p>{node.summary}</p>{node.output?<MessageBody key={`${task.id}:${node.id}`} content={node.output} markdown/>:<p>{node.detailNotice||'尚无正文，可在任务结束后同步协作。'}</p>}</div>}</>}
          </div></section>}
      </div>}
    </div>
    {menu&&menuTask&&createPortal(<div ref={menuRef} className="task-context-menu" role="menu" aria-label={`${menuTask.title}的操作`} style={{left:menu.x,top:menu.y}} onClick={event=>{if((event.target as HTMLElement).closest('button:not(:disabled)'))closeMenu();}}>{taskActions(menuTask,true)}</div>,document.body)}
  </section>;
}
