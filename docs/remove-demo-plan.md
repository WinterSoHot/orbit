# Remove demo delivery Implementation Plan

**Goal:** Remove interactive demo tasks and sample deliveries; retain real Codex tasks and document features.

**Architecture:** Delete frontend simulation and its IPC persistence. Decode legacy snapshots with existing Rust string provider, precisely filter `demo`, then validate and recover retained tasks. Opening never rewrites a historical snapshot; the next normal save omits retired demo records.

**Tech Stack:** Existing Tauri / React / TypeScript / Rust; no dependencies.

Architect A passed 2026-10-04. User authorized removal; implementation stays in the existing checkout because parent Git is unborn and holds unrelated documents.

## Constraints and preserved paths

- `Task::recover` restores real histories; `watchRuntime` plus `applyRuntime` update events; Runtime/Store edit_artifact preserve document CAS; sync_agents retains child history.
- Only provider == demo is filtered; unknown providers and invalid real records fail closed. Never directly edit real AppData.
- Browser is an empty UI preview, with no task creation, model execution or localStorage reads/writes. Native creation remains explicit; it never auto-starts.

## Execution

- [x] Add temporary-directory Store regression for legacy mixed/demo-only history, unchanged bytes on open, real document editing/export/reopen, unknown/invalid real history write protection. Run the mixed regression RED, then architect B before implementation.
- [x] Remove seed tasks, reducer, generated sample content, demo save queue/IPC and new-task selection. Retain real runtime subscription, time display, approval and child output. Remove obsolete demo tests and styles; update current README.
- [x] Remove save_demo command/Store method. Filter exactly demo at load; validate save_task inputs. Verify real edit/export after normal persistence and all existing Store/runtime checks.
- [x] Run frontend tests/build and Rust full suite. Verify browser empty workspace, empty deliveries, native-only create sheet, no demo restoration after reload. Build updated macOS app, then architect C final incremental Diff review before completion.

## Authorized addition: delivered-task archive

Architect A passed for the addition. Add default-false archived flag; only completed tasks with artifacts and idle owned Runs can archive. Archive/delete hold the lifecycle lock throughout current-state checks, persist-before-commit and Run synchronization/removal. Archived tasks can preview/export/delete but cannot edit or restart. Delete only workbench metadata and embedded outputs, retaining Codex sessions, run directories and exported files, with explicit UI confirmation. Runtime publication and shutdown must update existing records only, reject stale/run-mismatched snapshots and preserve archived state; frontend ignores deleted IDs. Verify disk failure leaves Store/Run untouched, late snapshots cannot unarchive/resurrect, and restart retains the result.

Capacity: a 50-record workspace rejects new IDs, including after archiving; it never silently evicts delivered or running records. Existing-ID updates remain allowed. Runtime and Store both reject archive history synchronization.

Completed after final C passed; Rust 44 passed/3 optional ignored, frontend 3 passed, SSR document read-only smoke passed, typecheck/Vite and macOS app build passed (39.74 MiB). Browser checked removal and archive empty-state entry; native full-button automation is unavailable.
