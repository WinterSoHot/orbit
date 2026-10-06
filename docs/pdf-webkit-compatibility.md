# PDF WebKit compatibility and icon refresh

PDF.js 6.4.299 legacy lacks two APIs on this machine’s WKWebView: `Promise.withResolvers` stops document initialization, and `ArrayBuffer.transferToFixedLength` stops font serialization, leaving later pages waiting for a shared font. The original error and the font serialization access stack were reproduced using generated PDFs.

`src/pdfCompatibility.ts` loads the standard core-js implementations in the reader and the separate local worker. PDF.js is pinned to 6.4.299. Both API and worker use that package; the worker is emitted as ESM and preserves `WorkerMessageHandler` for the fake-worker fallback. The CSP and canvas-only document rendering boundary remain unchanged.

Checks: production TypeScript/build, 16 Node tests plus SSR checks, and isolated native WKWebView tests for normal worker, forced fallback, TrueType fonts, 150% scaling and `tauri://` under the configured CSP. Transfer checks confirmed source detachment, retained bytes, short buffers and zero-filled extension. Both pages had nonzero text pixels. See `pdf-webkit-check.log`. The original user PDFs and whole-app picker interaction were not exercised.

The icon source is `src-tauri/app-icon.svg` (1024 canvas, 824 tile, transparent outer padding); Tauri generates desktop and platform sizes. `public/orbit.svg` uses the same mark with a tight viewBox for the interface. UI sizes: brand 36, profile 28, navigation 18, toolbar 16, document glyph 20 within a 36×40 tile. Lucide line weight is 1.75; Markdown, PDF and web use distinct glyphs. Metadata and editing now use sliders and a pencil.
