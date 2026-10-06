# Task Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 实现常驻任务对话、四列看板、人工验收与真实 FIFO 队列。
**Architecture:** 在既有 Store/Runtime 增加任务级持久请求与单 dispatcher。前端映射产品状态，不修改执行器协议；所有启动走唯一排队入口。
**Tech Stack:** Tauri 2, Rust, React 19, TypeScript, native HTML drag/drop。
**Spec:** ../specs/2026-10-05-task-workflow-design.md

## Global Constraints
- 保持 8 MiB/50 任务/10 份交付上限，无新依赖。
- lifecycle→queue control→Store；Store 锁不跨执行器调用。
- 保留苹果风格和知识库；本轮不实现 Chief。
- 不运行真实模型测试；不提交整个未跟踪父目录。按 AGENTS A/B/C/D 审查。

## Review Focus
- 崩溃发生在 claim/launch/clear 任一阶段时保留请求，不自动重试。
- 验收绑定最新 run/turn/正文，旧执行器高 revision 不能覆盖。
- 归档与排队互斥，撤销排队不能中断正在运行的任务。
- 错误和审批优先于排队标签，队列暂停不能被清理请求解除。
- Codex/Qoder 使用统一准入，但对话按各自能力启用。

### Task 1: Store 状态转移
**Files:** model.rs, store.rs, store queue tests.
**Interfaces:** enqueue(id,revision,action)->Task; claim_next()->Option<(Task,anchor)>; finish_claim(id,requestId,nextRunId,error)->Task; cancel_queued(id,revision)->Task; accept_task(id,revision,run,turn)->Task.
- [x] 先写 FIFO/CAS/claim 重启/旧回写/验收编辑失效测试，运行确认失败并执行规则 B。
- [x] 增加 QueueRequest 和 Acceptance，Store 原子校验与 schema v2 兼容；终态正文保持稳定。
- [x] 运行测试确认通过；保留旧归档兼容，更新新归档测试先验收。

### Task 2: Runtime 与桥接
**Files:** runtime.rs, lib.rs, bridge.ts, model.ts, model.test.ts.
**Interfaces:** QueueState{paused,reason}; Runtime start/continue enqueue；load_queue_state/set_queue_paused/cancel_queued/accept_task IPC；runtime-queue 和 runtime-task 事件。
- [x] 后台 dispatcher 唯一 claim/launch，失败暂停；重启已有请求暂停。请求清理失败不重复启动。
- [x] exact nextRunId 允许前端接受新运行；保留旧运行/跨provider/终态倒退保护。
- [x] 验证混合 provider 排队、恢复、暂停、写入失败保留数据。

### Task 3: 看板与常驻对话
**Files:** TaskWorkspace.tsx, TaskChat.tsx, App.tsx, workspace.css, SSR check.
**Interfaces:** TaskWorkspace 消费 tasks/selected/queue 与 start/cancel/accept/pause 回调，拖动只触发同名命令。
- [x] 四列映射、卡片选择、按钮等价拖动、对话折叠、详情标签（交付/Agent/日志）。
- [x] 常驻初始对话、运行正文与审批；active steer 与 terminal continue 统一 composer；queued 保留草稿禁发送。
- [x] SSR 与隔离 UI fixture 验证各列、草稿切换、验收及队列操作；保留归档/知识库/导出路径。

### Task 4: 验证与交付
- [x] npm test / npm run build；CARGO_HOME=/private/tmp/orbit-cargo MACOSX_DEPLOYMENT_TARGET=13.0 cargo test。
- [x] 浏览器隔离 fixture 视觉与交互检查，不改用户数据。
- [x] architect C 最终 Diff 和边界审查，阻塞问题修复后复审。
- [x] 打包 debug Orbit.app，核验结果并说明原生 UI 检查限制。

最终审查：architect C 复审通过。Rust 104 通过、5 个真实模型 smoke 忽略；前端 22 通过，SSR/build 和 debug App 打包通过。原生拖放尚未验证成功，按钮等价流程已检查。
