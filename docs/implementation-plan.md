# Orbit Tauri Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement inline. User requests implementation; no further design approval needed. Read-only architect gates follow the parent AGENTS.md.

**Goal:** Build and launch a usable personal Agent desktop prototype with clearly separated demo and Codex modes.

**Architecture:** React owns demo runs; Rust owns real runs. Narrow Tauri IPC, owned child process, JSON snapshot, read-only SVG graph.

**Tech Stack:** Tauri 2, React 19, TypeScript, Vite, Rust, serde_json.

**Spec:** docs/design.md

## Global constraints

- Only create files under agent-workbench. Parent repository has no commits and unrelated untracked documents: do not stage or commit those files, do not create an impossible worktree from unborn HEAD.
- No automatic model calls, no subscription credentials read, no writable Codex sandbox.
- Real state stays in Rust; one live root run; bounded output; old approval never replayed.
- Signed/public distribution is out of scope; build a local .app if toolchain/network permits.

## Review focus

Repeated approval; late events after terminal state; cancel versus finish; corrupt/old snapshots; app exit and child cleanup.

### Task 1: State contract and runnable shell

- [ ] Create package/configuration and `src/model.ts` types, reducer, seed demo tasks.
- [ ] Test first: approval is consumed once, cancelled runs don't advance, old run events ignored, interrupted snapshots recover unknown.
- [ ] Run checks; on any failure invoke architect under rule B before code changes.

### Task 2: Desktop runtime

- [ ] Implement `src-tauri/src/model.rs`, `protocol.rs`, `runner.rs`, `lib.rs` and local JSON store.
- [ ] Confirm local initialize/thread/turn/approval fields against exported CLI schema.
- [ ] Rust owns task projection; stdout/stderr read concurrently; disconnect invalidates requests; exit cleans owned process group.
- [ ] Test protocol projection, unknown requests, stale approval/terminal handling; no inference invocation in checks.

### Task 3: Interactive workbench

- [ ] Implement `src/App.tsx`, `src/Graph.tsx`, `src/bridge.ts`, CSS.
- [ ] New task dialog, scenario/mode filters, node inspector, demo timers, approve/decline, steer/cancel, delivery preview, CLI settings/check.
- [ ] Browser preview has demo functionality only and labels it; desktop adds real backend.

### Task 4: Verification and review

- [ ] Frontend test/build; Rust test/build; no-cost CLI handshake; browser QA.
- [ ] Build macOS .app and launch where possible; document exact results and limitations.
- [ ] Architect final Diff review under rule C; address blockers and re-review changed final Diff.
- [ ] README with desktop/browser commands and authenticated CLI prerequisite.
