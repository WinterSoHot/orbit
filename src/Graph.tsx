import { Bot, Compass, Microscope, ShieldCheck, CircleHelp } from 'lucide-react';
import { statusLabels } from './model';
import type { AgentNode } from './model';

export function Graph({nodes,selected,onSelect}:{nodes:AgentNode[];selected:string|null;onSelect:(id:string)=>void}) {
  const roots=nodes.filter(n=>!n.parentId || !nodes.some(p=>p.id===n.parentId));
  const levels=new Map<string,number>();
  function depth(node:AgentNode,seen=new Set<string>()):number {
    if(seen.has(node.id)) return 0;
    seen.add(node.id); const parent=nodes.find(n=>n.id===node.parentId);
    return parent?Math.min(depth(parent,seen)+1,5):0;
  }
  nodes.forEach(n=>levels.set(n.id,depth(n)));
  const rows=new Map<number,AgentNode[]>();
  nodes.forEach(n=>{const d=levels.get(n.id)!;rows.set(d,[...(rows.get(d)||[]),n]);});
  const width=Math.max(650,...[...rows.values()].map(row=>row.length*196+48));
  const height=Math.max(360,(Math.max(0,...levels.values())+1)*165+62);
  const positions=new Map<string,{x:number;y:number}>();
  rows.forEach((row,level)=>row.forEach((node,i)=>positions.set(node.id,{x:width/2+(i-(row.length-1)/2)*196,y:52+level*165})));
  const icons=[Bot,Compass,Microscope,ShieldCheck];
  return <div className="graph-scroll"><div className="graph-canvas" style={{minWidth:width,height}}>
    <svg width={width} height={height} className="graph-lines" aria-hidden="true">
      <defs><pattern id="dots" width="18" height="18" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r="0.7" fill="#d9dce3"/></pattern></defs>
      <rect width="100%" height="100%" fill="url(#dots)"/>
      {nodes.map(node=>{
        const from=node.parentId?positions.get(node.parentId):null,to=positions.get(node.id)!;
        if(!from)return null;
        const d=`M ${from.x} ${from.y+132} C ${from.x} ${from.y+150}, ${to.x} ${to.y-24}, ${to.x} ${to.y}`;
        const active=node.status==='running';
        return <g key={node.id}><path d={d} className={active?'edge active':'edge'}/></g>;
      })}
    </svg>
    {nodes.map((node,i)=>{
      const p=positions.get(node.id)!,Icon=icons[i%icons.length]||CircleHelp;
      return <button key={node.id} className={`agent-node ${node.status} ${selected===node.id?'selected':''}`} style={{left:p.x-85,top:p.y}} onClick={()=>onSelect(node.id)} aria-pressed={selected===node.id} aria-label={`查看 ${node.name} · ${statusLabels[node.status]}`}>
        <span className="node-top"><span className={`node-icon tone-${i%4}`}><Icon size={16}/></span><span className={`status-dot ${node.status}`}/></span>
        <strong>{node.name}</strong><span className="node-role">{node.role}</span>
        <span className="node-bottom"><span>{statusLabels[node.status]}</span><span>{node.status==='running'?'•••':node.status==='completed'?'✓':'—'}</span></span>
      </button>;
    })}
    {!nodes.length&&<div className="graph-empty"><Bot size={34}/><strong>Agent 将在这里出现</strong><span>启动任务后，显示执行器实际观测到的关系。</span></div>}
    {roots.length>1&&<div className="graph-note">未确认父子关系的 Agent 独立显示</div>}
  </div></div>;
}
