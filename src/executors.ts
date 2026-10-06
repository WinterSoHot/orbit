import type { Capabilities } from './model';
export interface ExecutorDescriptor { id:string; name:string; protocol:string; description:string; permissionNote:string; capabilities:Capabilities }
// The browser has no native catalog; these are preview labels, never execution backends.
export const previewExecutors:ExecutorDescriptor[]=[
  {id:'codex',name:'Codex',protocol:'codex-app-server',description:'本机 CLI，可续交付、补充方向与查看子 Agent',permissionNote:'文件沙箱只读，拒绝权限提升；MCP 沿用本机设置',capabilities:{resume:true,steer:true,interrupt:true,input:true,agentHistory:true}},
  {id:'qoder',name:'Qoder',protocol:'acp-v1',description:'本机 CLI，续交付以连接确认的能力为准',permissionNote:'仅开放任务目录读取，拒绝工具权限请求；非系统沙箱',capabilities:{resume:false,steer:false,interrupt:true,input:false,agentHistory:false}},
];
export const executorName=(id:string,catalog:ExecutorDescriptor[])=>catalog.find(e=>e.id===id)?.name||id;
