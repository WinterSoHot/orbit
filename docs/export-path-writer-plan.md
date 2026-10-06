# Export folder and occupied-session continuation plan

**Goal:** Persist a user-selected native export folder and let an occupied completed Codex session continue in a clearly identified history branch.

**Architecture:** Native Rust dialog exposes only choose/reset/read commands. Store reuses atomic workspace persistence and the common artifact exporter. Runner handles the exact foreign-writer error once via source read, pinned fork and durable branch admission before sending the supplement.

**Constraints:** Existing records and edited/archive deliveries survive; never kill a foreign writer, modify user AppData during verification, or automatically resend a turn. The browser remains a preview. Default export protections and no-overwrite semantics remain. A canonical check detects directory changes at check time, not malicious concurrent replacement.

**Review A:** export_continuation_gate approved with required durable mapping-before-send, strict RPC stages, real CLI capability verification, offline-folder isolation and partial-file cleanup.

- [x] Verify own real CLI occupied writer and pinned fork preserve exact completed turn content with experimentalApi=true; stop if unsupported.
- [x] Add Store export-folder regression and runner conflict/stage regression; run RED and obtain rule B diagnosis before implementation.
- [x] Persist optional export directory; canonical existing writable folder, safe cancel/reset, no silent fallback. Restore records when custom folder disappears; clean owned probes/partial files.
- [x] Add Tauri Rust dialog commands and settings row with folder path, choose and restore; browser disabled.
- [x] Add ReadingSource/Forking startup stages, exact error guard, pinned lastTurnId, source and branch validation; persist sourceThreadId + new branch before turn/start. Ignore/reject duplicate and out-of-stage RPCs. Chat shows source branch and actual failure details; failed pre-start supplement can refill draft.
- [x] Verify invalid/moved paths, reopen, archived export, failed settings persist, unchanged task data; conflicting history/foreign source/duplicate RPC/branch mapping persist failure deny turn/start.
- [x] Run full Rust/frontend checks, real own-session fork continuation, build macOS app, inspect preview; obtain rule C final Diff review before completion.

Current evidence: full Rust 56 passed / 5 optional ignored; frontend 5 passed and rendered settings/chat checks passed. Ordinary actual CLI two-turn continuation passed. Occupied-writer Rust test passed after retaining both ChildStdin and the owned process: fixed completed history forked, source identity persisted, SECOND_OK delivered and FIRST_OK retained. Refreshed full Rust suite passed (56 passed / 5 optional ignored), macOS debug app bundle built successfully (43.70 MiB), settings preview inspected. Architect rule C passed. Packaged --doctor with desktop PATH passed initialization without a model call. Completed.
