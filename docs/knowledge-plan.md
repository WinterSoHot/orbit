# Knowledge Library Execution Plan

Goal: deliver the standalone local library in [knowledge-design.md](knowledge-design.md). Architect A passed after persistence/CAS/blob/restore clarifications. Inline execution; no implementer workers. Baseline: /private/tmp/orbit-knowledge-before, no HEAD or synthetic commits.

## 1. Native library and integration

Files: knowledge.rs (data/store/network/import and real temporary-store tests), lib.rs (commands/native picker), store.rs (owned library + export v2), Cargo.toml/lock.

Interfaces: Library {documents,groups,error}; Document {id,kind,title,tags,url,content,revision,draft,versions,blobId,sizeBytes,createdAt,updatedAt}; Change {documentId,expectedRevision,sessionId,sequence,operation,content,versionId}. change checks/increments revision for all draft/commit/restore mutations. Metadata uses the same change command and CAS.

- [x] Tests first: import/reopen real PDF/MD, URL validation and inert text extraction; wrong UTF-8/nonregular/oversize/symlink private blob; damaged/unsupported/failed storage cannot overwrite; current visible draft backed up on restore; stale session/CAS/draft rejection; no-change version skipped, 30s automatic bucket preserves first prior text, 20 snapshots; smart rules AND + exact tag + time boundary; whole export contains PDF and all library records and rejects broken library.
- [x] Implement clone→actual UTF8 size validation→new temp write+sync→atomic rename→memory commit. Only NotFound initializes. Blob write+sync precedes metadata commit; failure best-effort removes new blob. Validate missing/truncated attachments at open and export. Orphans can occupy disk beyond logical capacity; no background deletion. Network/source reads happen outside locks, HTTP(S), no credentials, timeout20s/redirect5/decompressed4MiB.
- [x] Expose load/create/import/fetch/change/group/export/collect-artifact commands, using the store get method for document reads. Native import path originates only picker. Library owned by Store; export2 captures workspace→library snapshots then releases locks before immutable blob/base64 IO. Static complete export retains all providers/archives/tasks; no import feature.

## 2. Editor and library UI

Files: knowledge.ts (types/query/links), documentSession.ts (per-document serial transport with epoch fence), KnowledgeLibrary.tsx / KnowledgeEditor.tsx / PdfReader.tsx, library.css, App.tsx/bridge.ts/ArtifactEditor.tsx, package.json/lock, tauri.conf.json.

- [x] RED checks for filters/link navigation and real queue ordering: started draft must finish before restore; unstarted old jobs skipped; old response cannot reset newer local text; CAS conflict stops automatic retry, retains text; flush to switch uses current text. Every operation captures ID/session/edit sequence; response updates revision even while fence supersedes its display. Restore waits active request, then backs up visible text with latest revision.
- [x] Render three columns with existing system font/blue selection/neutral separators. Native-only mutations, empty browser preview. Type/tag/group filter with counts and search; editable metadata and reusable group form; add Markdown/web/import; collect deliveries; link copy, link insertion, backlinks; document saved/draft/failed states; keyboard save, visible error and focus. Flush on selection/filter navigation and knowledge exit; on error stay. Best-effort quit flush, only durable draft guaranteed on crash.
- [x] PDF.js lazy canvas reader from local bytes, one page/zoom, no JavaScript, worker-src self/blob allowed without object/frame/eval relaxation. Static web converted inert text; manual pasted body stays available. Full versions panel includes preview and restore; autosave2s and draft600ms through one queue; bucket first prior text retained.

## 3. Verification and delivery

- [x] Full npm tests+SSR, Rust suite, typecheck/build, native bundle. Each RED/failure triggers architect B before fixes. No user AppData edits or model calls. Web extraction tests are local fixtures, explicit HTTP transport test local temporary server if permissions permit.
- [x] Browser visual check of library and rendered editor/PDF using disposable test data, no shipped demo seeds; screenshot proof. Native picker clicks unavailable: Rust boundary tests + compiled IPC, report actual limits.
- [x] Incremental Diff to architect C, resolve blockers and re-review any changed code. Update README/progress and plan ledger after verified package.

Review focus: atomic failure preserving source; cross-session CAS + old draft after restore; restore while unsaved text exists; exact final storage capacity; dangerous file/URL/HTML boundary; incomplete attachments reject export; text change while save response arrives; native/browser disabled controls. No OCR/cloud sync/OS deep links.


Verification progress: Rust 86 passed / 5 optional model integrations ignored; Node 15 passed plus chat/settings/executor/knowledge rendered checks. Local HTTP fixture verifies exact 5/6 redirect boundaries, error status and decompressed 4MiB. Browser checked min native width, no horizontal library overflow, mutation guards; temporary in-memory desktop bridge checked editing→autosave→version preview→restore preserving backup, real two-page PDF canvas/zoom/navigation, internal document links and smart-group filtering. UI fixture is removed; initial native bundle passed; first C then requested the editor fixes recorded below.


C first review blocked two reproduced editor races. Fixed persistence decision using latest session body/draft/pending count with finally on all paths, without resetting the 2-second timer on draft responses. Added real-file completed/in-flight revert tests and count cleanup checks. Locked old editor body/metadata/restore/save/keyboard/link-insertion for the full Library action; internal flush stays available. PDF keeps canvas mounted and clears render error on each new page/scale. Controlled real-component checks now pass: completed/in-flight B reverted to A removes draft; idle B commits; fetch locks body/title and keyboard retry stays zero, failure unlocks, success retains pre-fetch input; exactly one page-2 canvas failure then page-1 returns visibly rendered with zero current error. Fresh tab no console warnings/errors; injected getContext restored. C re-review passed after the fixes; refreshed macOS debug bundle passed (71.67 MiB).


Completed: final 16 Node tests plus all SSR checks passed, production typecheck/build passed, unchanged Rust final suite 86 passed / 5 optional model integrations ignored. Architect C re-review passed with no blocking issues. Refreshed macOS Intel Orbit.app built successfully (71.67 MiB); no product code changed after the re-review. Native picker and whole-App quit/crash remain manually unverified. Completely quit the older Orbit and reopen this bundle to load the updated frontend and commands.
