# Continue delivered tasks Implementation Plan

**Goal:** Chat more information after delivery and generate another version in the original Codex session.
**Architecture:** Store bounded user supplement records and reuse editable artifacts as delivery versions. Resume the existing thread in a new owned run, retaining prior results. Shared startup machinery isolates resume notifications until the new turn is confirmed.
**Tech Stack:** Existing React / TypeScript / Rust / Tauri; no new dependencies.

Architect A passed with notification isolation and result-capacity reservation required. Implement inline under the user's authorization. Existing checkout retained: unborn parent Git and unrelated documents.

## Constraints

- Non-archived terminal completed/failed/interrupted task with delivery and known thread/terminal anchor only; unknown cannot continue.
- taskId/runId/turnId/revision CAS under Store lock; preserve original prompt, edited delivery content and previous thread. Record previousTurnId for explicit retry after failure before new turn acceptance.
- Maximum 10 delivery versions, maximum 10 supplement attempts, 2000 characters each. Reject before starting when no next-result slot. Failed accepted attempts remain in history and count toward the limit; no automatic retries or eviction.
- Resume same thread, idle and last turn exactly the expected terminal anchor; preserve cwd and enforce read-only. Ignore connecting notifications, buffer bounded notifications/requests between turn/start and RPC3, then replay only after its new turn ID is confirmed. Never allow another turn/started to replace a known ID.
- Persistence errors before launch leave original state intact. Accepted run initialization errors visible; uncertain post-request transport failure is unknown. Existing lifecycle/owned-process cleanup and stale run protections remain.
- UI calls supplement records “补充信息” and artifacts “交付版本”; edited artifacts are not immutable assistant transcripts. Draft clears only on accepted invocation; failures retain text; archive remains read-only.

## Execution

- [x] RED tests for continuation preservation/CAS/retry/capacity and original thread request identity/notification isolation; architect B on expected failure.
- [x] Implement Task continuation metadata and preparation, Store atomic admission, shared Runtime launch, resume/read-only requests and bounded startup buffering; verify full Rust suite.
- [x] Add conversation history and composer, version navigation, per-task drafts and stale previous-run rejection; frontend tests/build and UI render verification.
- [x] Package macOS app, review incremental Diff through architect C, handle blockers and verify final checks before completion.

## Running-turn supplement visibility repair

User reported no effect; read-only CLI history proved the supplement “重点给出数据存储怎么设计” was received and followed by the 2782-character SQLite design already stored in the app. The same turn contained an earlier 4285-character final answer that the projector overwrote. Preserve final answers by item ID (20 items, 64k characters including separators and notice), rebuild at completion, clear aggregation after generating artifact; no-phase legacy messages remain supported. Record bounded steering text and pending/accepted/rejected/unknown confirmations, matching RPC/run/turn identities. Accepted means executor confirmation, not semantic adoption. Pending persistence precedes send; unclear transport results never auto-retry.

A amendment reviewed; RED final aggregation and missing direction helper cases diagnosed through architect B; process-test EPERM diagnosed as sandbox probe restriction and full suite rerun with approved read-only ps access. A mistaken textual replacement in receive was restored locally, preserving approval logic and startup helpers; full suite green thereafter.

Completed after final architect C re-review passed; Rust 52 passed / 4 optional ignored, frontend 5 plus SSR passed, explicit real CLI two-turn continuation passed, refreshed macOS bundle 40.33 MiB. C found completed-message revision regression; its root/child immediate-persistence RED was fixed and verified through the full suite before re-review.

Final CSS follow-up reviewed through B and C: compact chat Markdown height scoped to chat, short/long browser checks passed, final frontend+SSR and package rebuild successful.
