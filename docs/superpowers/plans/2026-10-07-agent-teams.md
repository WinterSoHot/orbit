# Agent Teams and Delivery Gates Implementation Plan

> Use superpowers:executing-plans inline; checkbox tracking and this file's ledger preserve progress.

**Goal:** Independent Agent profiles, real parallel tasks, parent aggregation, plan/review gates and controlled Git delivery.
**Architecture:** Extend existing Task/Store and executor ownership maps; Store owns workflow and immutable inputs. Shared Runtime reserves up to three runs, adapters own processes. Git uses native plumbing and owned worktrees.
**Tech Stack:** Rust/serde/libc, Git CLI, React/TypeScript; no new dependency.
**Spec:** docs/superpowers/specs/2026-10-07-agent-teams-review-design.md

## Global constraints

- One parent level, 1–3 required children, at most 3 actual/reserved runs. Waiting parents do not occupy a slot.
- Keep 50 tasks, 8 MiB storage, 10 artifacts/receipts per task; fail atomically on capacity exhaustion.
- Immutable plan/config/input versions; typed review bound to exact input and trusted task/run; no ordinary-reply approval.
- Restart pauses all pending workflow work. Unknown/uncleaned processes retain ownership.
- Coding only with Codex owned worktree sandbox and empty complete MCP server list; Qoder/reviewer remain read-only.
- Git no hooks/filters/fsmonitor/external diff/textconv/signing at every command. Merge only CAS of an unchecked-out ref, never push or update user files.
- User's previous uncommitted features remain. No real model tasks for QA.

## Review focus

- Start reservation cancellation/shutdown, closed process cleanup and wrong-session writer.
- Changed profile or edited adopted artifact cannot silently alter frozen plans/review validity.
- Partial children, missing delivery, replay/foreign review and restart cannot advance parent incorrectly.
- Capacity/disk failure leaves no partial child/review creation or dangling protected references.
- Git executable extensions, symlinks/FIFO, conflicting paths, stale target and partial operation persistence cannot overwrite user files.

## Tasks

- [x] 1. Ownership/concurrency. Add executor ownership snapshots, confirmed-idle cleanup, per-task admission; QueueControl tracks reserved/observed runs. Store claim accepts reserved live claims but refuses recovery claims. Dispatcher launches outside shared lock while preserving reservation through cancel/shutdown. Check independent same-provider tasks, 3-slot capacity, same task/session rejection and cleanup failure.
- [x] 2. Team model/Store. Add team.rs AgentProfile, frozen plan, parent/worker/review roles, bounded input package. Store schema5 persists profiles and teams atomically; confirm/revise/cancel plan, protected references, children/result version tracking, capacity failures. Store-owned workflow advance generates unique summary/review steps only from explicit delivery; restart pending stays paused.
- [x] 3. Typed reviewer. Strict orbit-review packet via existing Candidate, task-specific instruction; Store receipts bind inputVersion/current run. Both providers accept authoritative final candidates; pass/changes/unable separate from process failure. Changes invalidate review/acceptance. Check replay/mixed/invalid/truncated/wrong-input and failed write.
- [x] 4. Git delivery. Add coding.rs controlled command/FD read, project preflight, owned worktree, plumbing immutable snapshot/integration with overlap rejection, bounded real Diff, operation records, reviewed unchecked target CAS. Freeze sandbox through start/resume/fork/turn; verify MCP empty and temp/network exclusions before turn. Tests use temporary repositories and hook/filter sentinels, executable/deleted/symlink/FIFO files, target moved and checked-out rejection, retry after write failure.
- [x] 5. UI/IPC. Agent profiles and team creation reuse ExecutorPicker; TaskChat displays plan confirmation, child progress/links, review result, retry/cancel/revise actions. Git preview and user-confirmed merge expose actual evidence and limitations. TaskWorkspace/Attention share workflow state. Keep Apple Light/Dark, native controls, accessibility, no nested scroll layout.
- [x] 6. Verification. Run complete offline Rust, frontend tests/build, isolated UI fixture, native App build; inspect final current-task Diff, architect C, fix blockers and re-review. Update README and design with implemented boundaries; no push/release.

## Interfaces

Task holds frozen assignment, optional TeamWorkflow, optional parent link, optional ExecutionInput, optional CodeWorkspace. Workspace holds AgentProfile list. Store team operations produce Workspace snapshots; Runtime emits every changed task. Adapters expose Ownership(task/run/session/live/cleanup). Review shares authoritative delivery Candidate but has a distinct validated packet; reviewers cannot submit ordinary deliverables as a verdict. Git records are platform-only, immutable snapshots supply review input. No adapter writes team/config/Git metadata.

## Ledger

- A: team_workflow_architect approved common core; team_git_gate approved Git after reservation, cleanup, review transport, plumbing and CAS constraints.
- Ruling: implement on codex/agent-teams-gates in current checkout — previous features are uncommitted and required by current preview; preserved baseline snapshot /private/tmp/orbit-before-agent-teams. No mixed changes discarded or automatically committed.
- Baseline: frontend 32 passed. Rust required explicit CARGO_HOME=/private/tmp/orbit-cargo; sandbox process/port checks pending B-reviewed escalation.
- Ruling: user's “继续” authorizes executing reviewed design; adopt previously recommended programming scope. Coding actions affect only user-selected owned worktrees; actual merge still requires explicit UI confirmation.

- B: baseline environment and intentional RED checks diagnosed; per-task admission now retains global inspection cleanup protection. FIFO omission fixed with bounded nofollow FD directory check before capture.
- C first audit: six blockers found (archived typed reviewer validation, cancel/shutdown handoff, stale snapshot authority, retry history, implicit Git transport). Fixed in shared boundaries; successful handoff cannot erase durable cancel intent, adapter spawn/register gates share shutdown, snapshot Store/runtime both require the current confirmed plan.
- C recheck: final-save failure now ends the launch reservation independently of disk success; durable claims remain recoverable and uncleaned adapter processes retain ownership. Failed summary can retry without repeating workers. Final readonly architect audit passed with no remaining completion blocker.
- Final validation: Rust 143 passed / 0 failed / 6 real CLI/model checks ignored; frontend 33 tests plus SSR team/ordinary regression passed; TypeScript/Vite and native debug App build succeeded (83.30 MiB). Isolated Light/Dark/plan/child/dialog/Diff UI checked and fixture removed. No real model execution, commit, push or release performed.
