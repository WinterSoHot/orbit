# Export All Data Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans inline; checkbox steps.

**Goal:** 设置页导出当前完整工作台数据到已设置目录。

**Architecture:** Store 取得一致 Workspace 快照并安全写 JSON；现有 Tauri IPC 和设置组件提供入口。复用单份导出 helper，等待真实保存 Promise，不取执行器生命周期锁。

**Tech Stack:** Rust serde/std，Tauri，React/TypeScript；无新依赖。

**Spec:** [export-all-design.md](export-all-design.md)

## Constraints and Review Focus

不修改用户 AppData、不调用模型、不导出外部 CLI 私有数据库；未保存草稿不包含，提交保存失败阻止导出。健康空空间允许；加载错误拒绝，原文保留。全 provider/归档/运行中/交付正文/未知会话字段全部保留。目录失效不回退，重复不覆盖；失败清理本次文件，快照 IO 不长期占 Store 锁。

## Task: End-to-end workspace snapshot export

Files: src-tauri/src/store.rs (WorkspaceExport + export_workspace + shared export helpers and tests), lib.rs (async IPC), src/bridge.ts, App.tsx (actual lastSave promise gate), ExportDirectorySettings.tsx (native button), exportSnapshot.ts/test (save sequencing), TaskChat.render-check.mjs; README/progress.

Interfaces: export_workspace() -> Result<String,String>, invoke('export_workspace'), exportAfterSave(Promise<unknown>|null, ()=>Promise<string>) -> Promise<string>.

- [x] Add full snapshot/default/custom/empty/broken-source tests and real-file save-failure gate checks; verify RED, rule B before fixes.
- [x] Implement shared export path and UI/IPC; run focused checks and then complete npm/Rust suite.
- [x] Build desktop bundle, inspect browser settings, record limits; submit incremental Diff to architect C and address blockers before completion.

Baseline: /private/tmp/orbit-export-all-before. Parent git has no HEAD, project remains untracked; preserve current workspace and no synthetic commits.

Verified: npm test (9 + SSR), full cargo test (72 passed / 5 optional ignored), npm run build and macOS debug bundle (44.97 MiB). Final architect C passed without blockers; existing path-open race and best-effort cleanup limits documented. Browser settings preview verified, native UI click unavailable; temporary stores only, no model or user AppData changes.
