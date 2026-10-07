# Task knowledge implementation plan

> **For agentic workers:** Use superpowers:executing-plans inline. Steps use checkbox syntax.

**Goal:** Explicitly select knowledge documents, PDF pages or annotations for tasks, preserve each submitted input and navigate its sources from delivery.

**Architecture:** A shared Rust source packet builds executor input. Store owns append-only input snapshots; existing queue and direction identities bind each packet to its actual run. React reuses PDF.js, Markdown document links and existing library collection.

**Tech Stack:** Existing React/TypeScript, Rust/serde and PDF.js; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-07-task-knowledge-design.md`

## Constraints and review focus

- At most 8 sources, 10 PDF pages, 32,000 Unicode scalar values per submission. Reject overflow, empty PDF text and version conflicts; no OCR or automatic RAG.
- SourceInput immutable identity/text/sources/kind; metadata binds run/turn and receipt status. At most 64 packets per task; existing 8 MiB workspace ceiling remains and rejects failed writes atomically.
- Initial packet template survives reruns; each run gets its own packet identity. Continue packets persist atomically with enqueue, keyed by nextRunId; direction packets share the existing direction request identity and save before send.
- Store preserves current source history against stale executor snapshots. New direction packets may append only alongside their matching pending direction; immutable existing packets may not be replaced.
- PDF front-end extracts selected physical pages. Backend validates document/revision/reader revision/page range and size; client excerpts are not claimed as independently verified PDF text.
- Document navigation flushes existing edits; missing/deleted sources show saved excerpts. Collected Markdown embeds source excerpts and links so task deletion cannot erase provenance.
- Verify consecutive continuations, queue write failures, late projection, unknown direction receipt, deleted/changed source documents and navigation with unsaved edits.

## Tasks

- [x] 1. Add Rust source request/snapshot/input types and bounded resolver/formatter. Regression checks cover Unicode/page limits, changes/deletion, and immutable supplied input. Add schema v3 with old record defaults; persist source packets at creation, queue claim/continue and direction save. Run Rust library tests.
- [x] 2. Route Codex initial/resume/steer and Qoder initial/resume through shared input formatting; preserve receipt state and source history in Store projection. Tests assert actual protocol payload and stale projection protection; collect delivery with its provided source excerpts.
- [x] 3. Add compact source selector and preview, PDF.js page extraction and saved annotation selection. Wire new task and per-task chat drafts, clear choices only after successful submission. Node checks cover page range and Unicode limits; SSR checks cover controls and browser/read-only guards.
- [x] 4. Render supplied-source snapshots and route valid document links through existing library location navigation. Preserve editor save protection and missing-document fallback. Verify production UI using isolated in-memory transport, PDF fixtures and Light/Dark.
- [x] 5. Run frontend tests/build and Rust tests, build desktop App, update README/spec and perform AGENTS rule C final architect review. Resolve blockers and re-review any modified final Diff. No publishing in this task.

Rule A reviewed by task_knowledge_architect; its six ownership, binding, PDF boundary, navigation and provenance requirements are incorporated above.


## Verification outcome

- Frontend: 30 Node tests and all SSR checks passed; TypeScript/Vite production build passed.
- Rust: 120 passed, 6 real-model checks intentionally ignored; final source-link change additionally passed the collection/persistence regression.
- Isolated UI fixture: Markdown continuous excerpt, PDF.js extraction of rotated physical page 2, saved annotation preview, supplied-source snapshot and page-location callback verified. Light and Dark layouts inspected; no user library read or model calls.
- Final macOS debug App bundled successfully after the link fix at `src-tauri/target/debug/bundle/macos/Orbit.app`.
- Architect rule C passed after fixing Qoder confirmation, queue cancellation/recovery, independent-start source boundary, changed-source navigation notice and clickable collected-source links. `git diff --check` passed.
- Sources are explicit saved excerpts, not automatic retrieval or proof of model usage. PDF extraction uses the existing compatibility/worker path; no OCR or new dependencies. Changes remain local; no publishing requested.
