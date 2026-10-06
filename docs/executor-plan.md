# Unified Executors Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans in this session. Steps use checkbox syntax.

**Goal:** 用户选择 Codex/Qoder 分发任务，业务层通过同一执行器契约扩展。

**Architecture:** Runtime 与 Store 管理任务业务，Executor 适配协议与进程。Codex 复用既有运行逻辑，Qoder 使用 Rust ACP v1，不引入 Node sidecar。

**Tech Stack:** Tauri 2、Rust std/serde、React 19、TypeScript。

**Spec:** [executor-architecture.md](executor-architecture.md)

## Global Constraints

- 保留所有 Codex 交付、子代理、active-writer 修复和导出设置；不更改用户 AppData 或自动重试用户模型任务。
- 全局一个根任务；只清理自有进程；模型等待不占共享锁。
- 协议版本和能力每次握手核对；未知 provider 历史不丢失，执行拒绝。
- 无新通用框架依赖。沿用工作目录开发：父仓库尚无 HEAD，全项目未追踪，避免创建提交或迁移现有工作台路径。

## Review Focus

- 旧 Codex 缺失新增字段、未知执行器记录及会话 metadata 均保留。
- 新旧运行与不同 provider 的快照不能相互覆盖。
- Qoder 加载重放、能力改变、错误会话 ID 不能生成错误交付。
- 取消通知未确认、断管及进程清理失败不能显示成功或放行新任务。
- 文件读取拒绝跨任务、符号链接越界、非 UTF-8、过大文件，权限请求拒绝不留待处理。

### Task 1: Shared contract and durable task model

Files: `src-tauri/src/executor.rs`, `runtime.rs`, `process.rs`, `runner.rs`, `model.rs`, `store.rs`, `lib.rs`; `src/model.ts`, `src/model.test.ts`.

Interfaces: `Executor` exposes descriptor/doctor/launch/ensure_idle/steer/interrupt/reply/sync_agents/refresh/forget/shutdown. Shared `Runtime` uses the same Tauri command surface plus `list_executors` and provider on create/doctor. `SessionRef` carries provider/protocol/id/cwd/metadata. `Capabilities` gates continuation, steering, interrupt and history.

- [x] Add assertions for Qoder snapshots, provider mismatch, old Codex continuation, Qoder persistence and unknown-provider history before implementation; run and record RED. Rule B architect diagnosis before changes after any failure.
- [x] Implement contract, shared dispatch/admission, owned-process reuse and additive migration; registered provider required at creation/launch, provider immutable in Store.
- [x] Run frontend model and existing Rust tests; expected all pass, optional model tests remain ignored.

### Task 2: Qoder ACP adapter

Files: `src-tauri/src/qoder.rs`; protocol fixtures in Rust module tests.

Interfaces: `QoderExecutor::new(Arc<Store>)` implements Executor; `AcpSession::receive(Task,Value,cwd)` translates handshake/session/update/prompt result into outbound requests and normalized task changes. `read_text` enforces canonical task root and bounded UTF-8 reads.

- [x] Add state-machine assertions for initialize/new/prompt, conditional load, history replay, foreign session, cancelled/stopReason, permission denial, size limits and read scope before implementation.
- [x] Implement real stdio path discovery and launch, bounded reader, initialization/interrupt timers, same-task cwd recovery, durable turn/input association and owned process cleanup.
- [x] Exercise a separate stdio ACP fixture process through production command/receive/write functions; expected complete flow and original/new delivery separation. Check missing executable diagnosis without inference.

### Task 3: Executor selection and final verification

Files: `src/executors.ts`, `ExecutorPicker.tsx`, `ExecutorSettings.tsx`, `App.tsx`, `TaskChat.tsx`, `bridge.ts`, `styles.css`, existing SSR checks; `README.md`, `docs/progress.md`.

Interfaces: backend descriptors populate new-task selector and settings; `taskCapabilities` gates all provider-specific actions; generic provider labels throughout task/delivery/chat.

- [x] SSR-check picker accepts a third descriptor, settings identify both backends, Qoder continuation requires advertised capabilities and no unsupported steer/history controls appear.
- [x] Implement UI and generic copy while preserving Apple-style layout and export folder settings.
- [x] Run npm test/build, full Rust suite and macOS debug App build; inspect browser layout. Do not run live user model tasks. Report absent Qoder CLI as unverified end-to-end boundary.
- [x] Submit final Diff to architect under rule C, address blockers and re-review any changes before marking complete.

## Ledger

- Rule A: architect approved candidate with lock scope, re-handshake, cancellation and same-cwd recovery constraints, 2026-10-04.
- Baseline snapshot: `/private/tmp/orbit-executor-before`; existing project not committed.
- Qoder route changed from SDK preference to ACP to preserve independent desktop distribution without Node; rich SDK-specific child events remain outside initial capability set.

- RED/GREEN: provider routing / persistence and permission-option assertions failed as intended; rule B reviewed the paths before implementation. Protocol tests were added before the adapter, but initial failure was missing module wiring rather than an assertion RED; recorded as such.
- Rule B also diagnosed sandbox ps restrictions and blocking stdin/FIFO risks. Owned process tests reran with reviewed escalation; independent bounded writer, regular-file openat validation and task-directory ownership checks added.
- Current verification: npm 7 model tests + chat/settings/catalog SSR passed; full Rust suite 67 passed / 5 optional integrations ignored; npm build passed. Separate stdio fixture verifies two versions without history replay contamination.
- Qoder discovery checked adapter search roots, including PATH: no executable found. Live CLI / model verification remains unavailable.

- Browser: settings exposes independent Codex / Qoder connection checks; new-task radio selection verified. Screenshots executor-settings-preview.png / executor-picker-preview.png; browser console errors absent. Desktop invocation remains native-only.

- Final C initially blocked two proven regressions (cancel watchdog skipped on failed/stale publication; replaced root symlink redirected reads). Rule B confirmed RED; fixed pre-publication timeout scheduling and pinned root FD with same-handle owner check. Both cancellation publication branches and root replacement verified GREEN. Related final Diff C re-review passed.
- Final Rust suite: 69 passed / 5 optional ignored. Final macOS debug bundle passed (44.54 MiB); packaged Codex initialize passed with restricted desktop PATH, no model call. No behavioral code changes after C approval.

Status: complete for shared executor dispatch and ACP client integration. Real Qoder CLI/model integration remains unverified because the CLI is absent.
