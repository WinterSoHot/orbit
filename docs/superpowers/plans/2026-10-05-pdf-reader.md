# PDF reader implementation plan

**Goal:** Extend the existing PDF.js preview with selection, continuous reading, search, navigation, local highlights/comments and Markdown excerpts.

**Architecture:** Use the installed upstream legacy PDFViewer. Store PDF-space annotations and page/zoom in Document.pdfReader with an independent CAS revision. Disk schema V3 protects annotations from older V2 writers; V1/V2 migrate only on successful writes. No original PDF rewrites, OCR, embedded-annotation PDF export, or Zotero AGPL code.

**Review:** Architect A accepted direction with conditions adopted below. Implement inline, following writing-plans, executing-plans and TDD; no new worktree or automatic commits in the untracked workspace.

- [x] Verify helpers and WK13 viewer compatibility first. Sequentially import API then viewer; add polyfills only for observed failures. Disable scripts, forms, auto links, internal editors and detail canvases. Keep worker fallback exports.
- [x] Add bounded PDF reader state, CAS command and schema V3 migration. Page 1..100000; scale .25..4; max 500 annotations, 256 rectangles/annotation; text/comment 16KiB each; finite ordered PDF coordinates bounded to +/-1e7. Preserve 16MiB total. Trash/restore invalidate reader revision, including empty readers. Verify originals, corrupt records, body independence and export/reopen.
- [x] Implement serial reader persistence with failed-input retention, unified flush before navigation/export/lifecycle, and synchronous mutation freeze. Merge body and reader revisions independently. Browser fixture exercises repeated viewer mounts, text selection, search, zoom, navigation and cleanup.
- [x] Add continuous viewer (upstream bounded page buffer; per-canvas 2M/4096, not a total-memory guarantee). For >200 pages use upstream single-page scroll mode to limit visible work. Lazy thumbnails only for current +/-2 pages, at most five small canvases concurrently, cancellations on unmount. Outline uses upstream destination navigation. No duplicated search index.
- [x] Add single-page text selection highlight overlays transformed with viewport, comment editing/deletion, Markdown excerpts from latest body, shared validated document query links; deleted annotations fall back to page. Reject cross-page selection explicitly. Actual page count and bounds validated in loaded PDF.
- [x] Run Node/SSR, Rust, production build, isolated native WK compatibility and UI fixture; address test failures via architect B. Final diff/boundaries architect C, then build desktop bundle. Document actual verification limits.

Validation: core WK passed; complete native React UI automation environment-blocked (hidden/no RAF). IAB full interaction fixture passed. Final architect C passed after recovery and link-insertion fixes; desktop debug Orbit.app bundle built successfully (76.26 MiB). See ../../pdf-reader.md for limits.
