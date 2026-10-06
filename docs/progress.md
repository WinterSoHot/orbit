# Implementation ledger — docs/implementation-plan.md

Status: complete for the local Tauri prototype scope.

- Task 1: shared task contract and frontend state; 6 frontend checks passed.
- Task 2: Rust-authoritative runtime, persistence, controls, observed Agent graph; 16 Rust checks passed, plus the separately invoked initialize-only integration check.
- Task 3: demo creation → exploration → scope confirmation → review → completion → text preview verified through the UI. Node details and desktop window dimensions verified; console errors absent.
- Task 4: macOS Intel debug Orbit.app built, 39.13 MiB. Bundled binary initialized Codex successfully with PATH restricted to system directories. Final architect C review passed after cleanup failure admission gate was fixed.

Rulings:

- New subdirectory without commits/worktree: parent Git has unborn HEAD and unrelated documents; existing content preserved.
- JSON snapshots and read-only SVG replace SQLite/ReactFlow at this prototype scale, following architect A.
- User cancel awaits provider confirmation. Process lifecycle cleanup force-kills the owned group; failed cleanup remains explicit and blocks new real roots. Confirmed provider completion remains completed.
- Native desktop UI automation is unavailable. The UI was verified in the browser preview; the actual bundled binary was verified via its no-window --doctor path.
- No model task was invoked by verification. Real inference, sub-agent event coverage, and external tool effects remain unverified end-to-end. macOS developer signing/notarization and cross-platform distribution are outside this prototype.

## Model compatibility repair

- Observed user run: provider rejected inherited gpt-6.1-sol for ChatGPT account authentication; the prior UI omitted turn.error.
- Startup now queries the CLI catalog and explicitly requests its visible default model. Pagination is bounded and rejects repeated cursors. The actual model is read from thread/start response.
- Error messages are extracted, conservatively redacted, bounded, and shown in task details and events. No global CLI configuration was modified; the user research task was not automatically retried.
- Final checks: frontend 6 passed; Rust 21 passed, 2 optional integration checks ignored in the normal suite; production helper real inference check manually passed once (gpt-5.6-sol → completed → OK, no tools).
- Final C review passed after opaque credential redaction was repaired. Updated macOS Intel Orbit.app built successfully, 39.22 MiB. Fully quit and reopen the bundle to use the new executable.

## Markdown preview and editing

Status: complete for existing deliverables.

- Added rendered Markdown preview (GFM tables/lists/code), source editing, local save, and export of the confirmed saved revision. Document close/backdrop/Escape protect dirty drafts; keyboard focus stays inside the editor and returns to its opener.
- Store edits use terminal-task validation, exact artifact ownership, content CAS, UTF-8 size limits, and persist-before-commit. Demo snapshots cannot replace equal/newer revisions. Closed Runtime snapshots synchronize edits so shutdown retains them.
- Verified frontend 8 tests and Rust 25 tests passed; 2 optional real CLI checks remained ignored. Browser verified preview, save/reopen, dirty discard, Cmd-S, and 24 Tab/Shift-Tab moves inside the dialog. Raw HTML/iframe, image fetches, and executable links were excluded from rendered output.
- Final architect C passed after focus containment was fixed. Updated macOS Intel debug Orbit.app built successfully, 39.34 MiB. Browser Markdown preview remains open; native App must be reopened to load the new bundle.
- Native entire-App termination is outside document-dialog dirty confirmation; save before quitting. No model call was needed for this feature.

## Observed sub-Agent display repair

Status: complete for Codex hosted sub-Agent activity and completed-run history synchronization.

- The user's knowledge-workbench run actually created product, technical, and challenger children. Its CLI history contained 12 subAgentActivity records, which the previous decoder omitted.
- Live events now project observed child identities, paths, statuses, and parent relationships. Completed native tasks expose “同步协作” to replay the selected run's history through initialize/thread/read without starting a model turn. Unknown child models and absent output remain explicit.
- Synchronization retains the latest edited deliverables and root/child output, verifies run/turn identity, and persists before committing. Independent bounded activity IDs prevent rolled-out display logs from allowing old starts to regress completed children. Unconfirmed inspection-process cleanup remains tracked and blocks new admission.
- Verification: frontend 8 passed; Rust 32 passed with 3 optional integrations ignored in the normal suite; actual CLI read-only history integration separately passed, restoring 4 completed nodes with unchanged delivery. Frontend typecheck/build and macOS Intel debug bundle build passed, 39.79 MiB.
- Final architect C review passed after the cleanup-retention and persistent-deduplication fixes. Native button interaction remains untested because native UI automation is unavailable. Fully quit and reopen the updated Orbit.app, select the completed task, and click “同步协作” to recover its existing graph without rerunning the task.

## macOS-inspired interface redesign

Status: complete for the existing frontend surfaces.

- Applied frontend-design: neutral light sidebar, white content, system typography, blue selection/action, compact status summary, simpler task rows, clearer Agent cards and inspector. Removed the promotional hero, oversized metric cards, fake profile/dropdown/menu affordances, and continuous graph animation.
- Existing task handlers, synchronization, persistence, approval forms, and Markdown editing remain in place. Focus rings, reduced motion, responsive sheets, and local graph scrolling are retained. The sidebar scrolls in low windows and its save notice reflects persistence failure.
- Browser verified 1480×960 desktop, 1060×720 minimum native size, 1060×600 low window, and 375×812 narrow layout. All four demo Agent nodes and three edges fit at the default native size; node selection updates details. Document preview/edit/dirty-close/discard, new-task sheet, collections, and settings were checked. Validation drafts were discarded; no real tasks were started.
- Final frontend tests: 8 passed. Typecheck, Vite production build, and updated macOS Intel debug Orbit.app bundle passed (39.79 MiB). Console errors absent. Final architect C passed after correcting save-state copy and low-window sidebar access.
- Design: apple-ui-design.md; incremental diff: apple-ui.diff; browser screenshot: apple-ui-preview.jpg. Native App interactions remain unavailable to automation; fully quit and reopen the rebuilt bundle to load the redesigned interface.

## Child Agent output details

Status: complete for completed Codex tasks and read-only child history inspection.

- Root collaboration records contain child identities and status but do not contain child answers. “同步协作” now reads confirmed child histories using one owned CLI process and a shared deadline, validates exact child ID/parent/path, and merges under the current Store run/turn guard without changing root delivery or edited documents.
- Inspector now exposes path, output-round count, read notices, and a read-only “查看完整输出” Markdown sheet. Child historical rounds are labeled explicitly; no unsupported mapping to individual root turns is asserted. Model names remain unprovided when absent from the CLI response.
- Failed, empty, foreign, or structurally incomplete reads preserve latest child output. Repeated synchronization is idempotent. The 64,000-character output cap has a visible truncation notice; the 8 MiB budget applies to cumulative accepted history JSON content rather than total process memory or wire traffic.
- Latest verification: Rust 37 passed, 3 optional integrations ignored in the normal suite; frontend 8 passed; typecheck/build passed. Separately invoked actual CLI read-only integration passed with product/technical/challenger each having two rounds, nonempty output, unchanged delivery, and unchanged revision on repeated synchronization. No model task started and real AppData was not directly edited.
- Browser verified child selection, full-output open, Tab focus containment, Escape/restore focus, and existing document preview/edit entry. Screenshot child-output-preview.jpg uses demo content. Native button interactions remain unavailable to automation.
- Final architect C passed after rejecting malformed message bodies and adding missing/null/non-string body and message-ID regressions. Final macOS Intel debug Orbit.app rebuilt successfully, 39.95 MiB. Fully quit and reopen the bundle, then synchronize the existing completed task to populate its child details without rerunning it.

## Demo removal and delivered-task archive

Status: complete for real delivered tasks.

- Removed interactive demos, seed tasks, generated sample deliveries, simulation reducer/timer, demo IPC persistence and browser localStorage access. Native history precisely filters legacy demo providers without rewriting files on open; unknown/corrupt records remain protected. Real documents and runtime subscriptions remain.
- Completed tasks with artifacts can be archived into a separate sidebar list. Archived documents are read-only, exportable and removable after confirmation. Deletion removes workbench records/embedded output while retaining Codex sessions, run directories and exported files. Archived tasks cannot start, edit or synchronize history.
- Store changes persist candidates before committing. Runtime archive/delete hold the lifecycle lock, reject non-idle owned Runs, then synchronize or remove all matching Run snapshots. Runtime publication/shutdown only update existing current-run records; archived/missing/stale records are rejected. Terminal document edits are preserved. Frontend suppresses late deleted-ID updates and refreshes authoritative state after a failed delete.
- Workspace capacity is 50 including archives; full capacity rejects new tasks rather than silently evicting records. Existing records can still update.
- Final checks: Rust 44 passed, 3 optional integrations ignored; frontend 3 passed after deleting demo-only tests; archived/read-write Markdown SSR smoke passed; typecheck, Vite build and macOS Intel debug bundle passed (39.74 MiB). Browser confirmed old demos no longer appear, new-task creation is native-only, and archive entry/empty state exists. Screenshot archive-preview.jpg shows browser preview rather than native data. No model calls or direct AppData edits.
- Final architect C passed after removing automatic capacity truncation and blocking archived synchronization at both Runtime/Store. Fully quit and reopen the bundle to load the update. Native archive/delete buttons are not automated.

## Chat continuation and running supplement repair

Status: complete after final logic/style C reviews and refreshed macOS bundle.

- Delivered non-archived tasks can chat more information; Store admission checks current run/turn/revision atomically before launching. Original thread resumed in a fresh owned run; previous saved/edited versions retained. Startup failure remains explicit and versions survive. Archive stays read-only.
- Runtime startup isolates history events and buffers early notifications/requests until the new turn is confirmed. Prior run/turn cannot contaminate current state. Version/supplement limits reject before inference; no eviction or implicit retry.
- Actual user history showed the steering input had worked: “重点给出数据存储怎么设计” led to the saved 2782-character storage answer. It also exposed a discarded earlier 4285-character final answer. Current-turn item-ID aggregation now retains multiple final answers and protects completion from later commentary/delta, with a 64k-character budget including separators and a visible truncation notice.
- Running-direction text is saved before sending, with pending/accepted/rejected/unknown state and exact RPC/run/turn confirmation. Accepted is transport confirmation, not a claim of semantic adoption. Unconfirmed input is not auto-retried; restart invalidates pending confirmations.
- Rust full suite: 52 passed / 4 optional ignored. New explicit real-CLI continuation test separately passed (16.30s): FIRST_OK -> same thread resumed -> SECOND_OK, both artifact IDs/content preserved. No tools, user task or user AppData changes.
- Frontend: 5 model tests and runnable TaskChat SSR history/archive/active/browser guards passed. Browser validated rendered real-component test content, version disclosure and minimum desktop width; temporary layout page removed before packaging. Native entire-window automation remains unavailable.
- Architect B diagnosed expected REDs, a sandbox ps restriction (approved rerun), a local text-edit mistake and Unicode separator budget overflow; fixes verified by full suites. Final C reads the incremental diff against the saved pre-feature baseline.

- Final C blocker fixed: completed-message revision now advances immediately for root and child; Store regression proved immediate persistence/reopen without later turn events (RED → GREEN). Final C re-review passed. Updated Orbit.app rebuilt successfully, 40.33 MiB; no code changes after that review.

- Final visual QA: chat Markdown inherited editor fixed height; scoped override now uses automatic height/min-height 0. Browser measured short content 117px and long content 360px with 2224px scroll height. CSS C re-review passed; frontend checks and final refreshed bundle passed (40.33 MiB). Temporary fixture pages and tabs removed.

## Export folder and occupied-session continuation

- Export settings use the native Tauri Rust folder picker; saved custom folder and reset-to-default share atomic Workspace persistence. Regular and archived artifact exports use the same setting. Removed/redirected folders reject export while preserving task history; UUID/create_new preserve existing files.
- Exact original-thread active-writer rejection triggers one bounded read → pinned history fork → durable mapping → supplement turn. The original external writer is preserved; the source ID and branch notice are saved. Source identity/last completed turn and full anchor items must match. New branch must be idle; startup RPCs outside the current phase are ignored, and no turn is auto-retried.
- Chat displays the actual failure and can refill the latest failed pre-start supplement for manual retry. Existing versions and multi-branch lineage survive reopening.
- Final full Rust suite passed: 56 passed / 5 optional ignored. Frontend 5 tests plus settings/chat rendered checks passed. Ordinary real CLI continuation passed. Occupied-source real CLI continuation explicitly passed (18.14 s) with source stdin held open, persisted branch identity, FIRST_OK/SECOND_OK versions retained and owned process cleanup confirmed.
- Browser settings preview checked and saved as docs/export-settings-preview.png. Native folder picker invocation cannot be automated in the currently enabled computer-use surface; Rust API integration compiles, folder persistence/export behavior is tested with temporary directories. No user task was rerun and Orbit user AppData was not edited by verification.
- macOS debug bundle built successfully: src-tauri/target/debug/bundle/macos/Orbit.app (43.70 MiB). Packaged binary --doctor passed initialization with desktop PATH (Codex 0.160.0), no model call. Architect rule C passed; status: complete.


## Unified executor dispatch: Codex and Qoder

Status: complete for unified dispatch and ACP client integration; real Qoder CLI/model integration remains unverified.

- Shared Runtime owns registration, provider routing, global single-root admission, continuation and durable business operations. Executor trait owns its protocol and processes. Codex preserves prior app-server behavior and active-writer recovery. Qoder uses ACP v1 in Rust without a Node sidecar.
- Tasks persist immutable provider, extensible SessionRef and capabilities. Legacy Codex records remain usable; valid unknown-provider history remains readable/exportable, but cannot execute until registered. Store and frontend reject cross-provider/stale-run snapshots.
- Qoder re-handshakes on each run, loads only if freshly advertised, verifies task directory ownership and ignores load-history replay. Cancel notification awaits prompt result; timeout/disconnect is unknown. Independent bounded writer lets cancellation kill owned groups even with blocked stdin. Text reads walk directory descriptors without symlinks and reject FIFO/device/oversized/non-UTF-8 input. Tool permissions deny reject_once or cancel; CLI internal tools are not an OS sandbox.
- UI reads descriptors for the new-task picker, labels and per-executor diagnostics. Existing tasks retain their selected provider. Unsupported steer/history controls are capability-gated; a third descriptor is covered by rendered UI checks. Archive/edit/delete of an unrelated completed task remain possible while another task runs.
- Verification: Rust 69 passed, 5 optional integrations ignored; 7 frontend model tests plus chat/export/settings/catalog SSR checks passed. Production frontend build and macOS debug bundle passed. Separate ACP stdio fixture verifies two rounds and preserved versions; no live model or user AppData changes. Qoder executable was not found in adapter lookup roots, so real Qoder initialization/delivery is unverified.
- Browser checked selection and settings with no console errors; screenshots executor-picker-preview.png and executor-settings-preview.png. Native UI interaction remains unavailable to current computer-use surface.

- Final C blocked two reproduced boundary failures; fixed cancellation cleanup scheduling before publication and bound file reads to an authorized root FD with same-handle task owner validation. Fixed regressions passed, related final Diff re-review passed, and full suite passed afterward. Packaged Codex --doctor confirmed initialization with restricted desktop PATH and no model call before the boundary fixes; final package refreshed successfully (44.54 MiB), and its Codex initialize check also passed without a model call.

## Export all current workbench data

Status: complete after final architect C and macOS bundle verification.

- Settings provides a native-only export-all button. JSON format `orbit-workspace`, version 1, captures full Workspace records and export time: active/archived/unknown-provider tasks, nodes, current output/events/approvals, supplements/directions, delivery versions/content, session references, capabilities and export settings. It does not include unsaved drafts or external CLI credentials/databases/run files; no import is provided.
- Store clones one consistent snapshot under its mutex, then serializes and writes without holding the mutex or executor lifecycle lock. Existing export-folder validation is reused; invalid folders reject, UUID/create_new avoids overwrites, Unix files use 0600, and failed writes attempt to remove the new file. Healthy empty workspaces export; known broken sources reject while preserving original bytes.
- The frontend waits for the actual last submitted document-save promise. A rejected save blocks export instead of being hidden by the existing sequencing queue. Real temporary-file tests prove sequencing and rejection.
- Expected RED checks for missing UI, temporary backend placeholder and temporary save-gate placeholder were reviewed by architect B before implementation resumed. All were resolved by implementation.
- Latest checks: 72 Rust tests passed, 5 optional model integrations ignored; 9 frontend tests and chat/settings/catalog rendered checks passed; production typecheck/build passed. Browser settings layout verified with no warning/error logs, screenshot export-all-preview.png. Browser preview intentionally disables native export; native window automation is unavailable. Verification used temporary stores, did not call models or edit user AppData.
- Final architect C passed with no blockers. Two non-blocking existing write limits remain: path validation cannot prevent directory substitution between validation and open; cleanup is best-effort and may leave a partial file if deletion fails. Documented in export-all-design.md; no code change after final review.
- Final macOS Intel debug Orbit.app rebuilt successfully (44.97 MiB). Fully quit and reopen the updated bundle to use Settings → 导出全部数据.


## Independent local knowledge library

Status: complete after final architect C re-review and refreshed macOS bundle verification.

- User selected Orbit standalone library. Adds PDF/UTF-8 Markdown import, local PDF canvas page/zoom, static web text capture/manual paste, tags, persisted AND smart groups, internal stable document links/backlinks and delivery collection into independent Markdown copies.
- LibraryStore is owned by existing Store; atomic metadata commit, revision/session/sequence checks, immutable private PDF files, bounded source/network reads and metadata/attachment capacity validation. Editor serializes draft/commit/restore/metadata with epoch fences, retains unsaved visible text on errors, flushes before selection/navigation, and restores durable drafts. History retains 20 snapshots with 30-second automatic coalescing; restore first backs up visible text.
- Full export extends version 2 with library bodies/drafts/history/tags/groups and PDF base64 attachments; rejects damaged metadata/missing PDFs. Source files and workspace format 1 stay intact. No full-export import, library deletion, OCR, cloud sync, external DEVONthink connection or OS deep-link registration.
- Verification so far: 86 Rust passed, 5 optional model tests ignored; 15 Node plus chat/export/executor/knowledge SSR passed. Local temporary HTTP fixture tests exact redirect limits, status rejection and gzip decompressed byte limit. Browser guards/min-width layout passed; temporary in-memory desktop bridge exercised autosave, history restore, actual two-page PDF render/page/zoom, internal links and smart filtering with no warning/error logs. It is UI verification, not native IPC/persistence; backend uses real temporary stores separately.
- All failed checks invoked architect B before fixes: local fixture nonblocking sockets and incomplete request parsing, standard reqwest redirect counting, group revision overflow, and simulated desktop flag. Test-only HTML/TS entrypoints removed. No models called, no user AppData changed. Native picker and whole-App crash/quit interactions remain manually unverified.

- Final C first pass blocked stale reverted drafts and input lost while capture was waiting. Both were reproduced before fixes. Session persistence decisions now include durable draft/in-flight work, with all-path pending cleanup; draft responses do not reset the commit timer. Full library actions lock the old editor including metadata and keyboard; failure releases the lock. PDF rendering errors retain canvas and clear on a new page.
- Directed checks passed: actual temporary-file queue completed/in-flight undo, canceled/failed count cleanup; real-component undo/autocommit, controlled fetch locks and failure unlock, successful capture preserving pre-capture text; exactly one injected page-2 canvas failure followed by visible good page-1 render. Fresh-page console clean; temporary fixture removed and canvas override restored. Final C re-review passed; refreshed package and production build passed.

- Final delivery: 16 Node tests and all SSR checks passed after editor fixes, production typecheck/build passed, Rust 86 passed / 5 optional models ignored (backend unchanged since that full check). C re-review passed with no blockers. macOS Intel debug Orbit.app refreshed successfully, 71.67 MiB; no product code changed after review. Reopen the updated app after completely quitting the older instance. Native picker clicks, multiple processes sharing AppData and whole-App unexpected quit remain outside verified coverage.


## PDF WebKit compatibility and icon refresh

Status: complete after architect C, fresh checks, and refreshed macOS bundle.

- Reproduced the screenshot’s native TypeError in isolated WKWebView: missing Promise.withResolvers during loading-task creation. Follow-up font tracing confirmed missing ArrayBuffer.transferToFixedLength during font serialization and a later page waiting for the unsent shared font. Each failed diagnostic check invoked architect B before product changes.
- Main and local worker load two standard core-js compatibility modules. PDF.js is pinned, worker ESM exports preserved for fallback, CSP unchanged. Five fixture cases passed native display rendering, normal/fake workers, system/TrueType fonts, two pages, 150% zoom, real buffer detachment and visible text pixels; same tauri:// source/CSP tested in an independent WK scheme fixture. Original user PDFs and whole-App native interaction were not exercised. Details: pdf-webkit-compatibility.md and pdf-webkit-check.log.
- Unified blue Orbit desktop/interface mark; regenerated platform assets from SVG. Navigation/tool/document glyph sizes and line weights now match the Apple-style layout, with distinct Markdown/PDF/web glyphs. Browser real-component page/zoom and visual checks passed using memory-only fixture data.
- Final C approved no blockers. Temporary entrypoints removed; final dist contains none and worker export verified. Fresh 16 Node tests plus all SSR checks passed, production typecheck/build passed, native macOS Intel debug app refreshed at 71.74 MiB. Bundle ICNS hash matches the regenerated icon; no product code changed after C.
