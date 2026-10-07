# Explicit delivery implementation plan

> Use superpowers:executing-plans inline, with checkbox tracking.

**Goal:** Separate ordinary replies from validated explicit delivery, with manual message capture.
**Architecture:** One shared delivery.rs parser/schema and Store-owned atomic receipts. Provider adapters capture authoritative final candidates; existing Artifact renders typed results. No MCP service or new dependency.
**Tech Stack:** Rust/serde, existing reqwest::Url, React/TypeScript.
**Spec:** docs/superpowers/specs/2026-10-07-explicit-delivery-design.md

## Review focus

- Final-message integrity: no phase-less, multi-message, mixed, truncated, failed, foreign or sub-agent fallback.
- Atomic write, idempotent original receipt, concurrent/late projection, unchanged old artifacts.
- Manual saved historical reply retains its historical sources; no manual or stale deliverables in current acceptance.
- Typed URL/result validation, unsupported file rejection, inert rendering; derives export without invoking links.
- Schema4 compatibility, bounded records, restart retention and browser read-only guards.

Pre-flight: parser produces DeliveryPacket used by Store; providers produce Candidate used by parser; Store receipts/current artifact IDs feed frontend and acceptance. These shared interfaces use one canonical typed payload.

- [x] 1. Add behavioral RED check that ordinary completed final reply creates zero artifacts. Implement bounded strict delivery packet/candidate capture and provider contract, Qoder root transcript, removing both automatic MD conversions. Review B on any failed check, including intended RED.
- [x] 2. Store-owned receipt/atomic commit, schema4, source-bound message manual-save IPC, ownership guards and current executor-delivery acceptance. Regression checks cover both executor packets, mixed/multiple/invalid/truncated declarations, duplicates/conflicts, write failure, source history and restart.
- [x] 3. Typed delivery cards/window, manual-save control, current-run empty/validation-error copy and acceptance guards. Reuse existing Markdown/PDF source navigation; link/result read-only with existing safe renderer and derived export. Node/SSR and isolated UI fixture verify desktop guards and typed cards.
- [x] 4. Run frontend tests/build and Rust library checks, build desktop App, update README, git diff --check, final architect C review against /private/tmp/orbit-before-explicit-delivery. Resolve blocking findings and re-review changed final Diff. No push/release requested.

Verification: Rust 129 passed / 6 explicitly ignored real-CLI checks; Node 32 and SSR passed; production frontend and debug desktop App built. Architect C passed after current-turn binding, terminal task cleanup, immutable completed messages/run-bound errors, and byte-bounded export names were fixed and rechecked. Isolated UI fixture verified manual capture and typed read-only result display; temporary page and tab removed.
