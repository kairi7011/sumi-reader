# Sumi

A quiet, personal PDF reader for macOS. Built for long reading sessions and taking excerpts into your own AI conversations.

Private project owned by **kairi7011**. Documents stay on your Mac. There is no account, telemetry, cloud upload, or AI API integration.

## Development

Sumi 0.3 uses Tauri 2 and the macOS system WebKit. It does not bundle Electron or Chromium. PDF.js uses its compatibility build for WebKit.

Requires an Apple Silicon Mac, Apple's command line tools, Node.js 22.13+, and the verified Rust toolchain. Dependency installation is script-free. Read [dependency verification](docs/dependency-review-tauri.md) before fetching Rust crates or updating either lockfile. The build itself is locked and offline; it never downloads code.

```sh
npm ci --ignore-scripts --omit=optional
npm start
npm test
npm run test:native
npm run package:mac
```

`SUMI_RUST_BIN` can point at the directory containing the verified `rustc` and `cargo`. The build script also recognizes the workspace toolchain in `../toolchains/rust-1.99.1/bin` and its `../cargo` cache.

## Reading

- Open a local PDF or drop it onto the window.
- Read one page or a facing spread, with left-to-right or right-to-left navigation.
- Keep the whole page in view, with a thin titlebar and a small page-navigation capsule. Library, search, and settings open over the page without resizing it.
- Turn pages with the arrow keys, Space, or a corner drag that follows your hand and returns when cancelled.
- Press G to jump by page number, preview a slider position, or choose a thumbnail. The grid renders only a small window of pages.
- Zoom the page in place with pinch or Command + / −. Command 0 restores the complete page; Space-drag moves an enlarged page. Text selection and rectangle capture keep working at the displayed scale.
- Share light, sepia, or dark appearance and page-layout settings across the app. Only reading positions and bookmarks belong to individual books.
- Show the book title from PDF metadata, with a conservative cover-text fallback; keep original filenames unchanged.
- Resume reading from recent documents; bookmark pages and navigate the PDF outline.
- Search document text, select text, and collect excerpts with their source page.
- Drag a rectangle over a page to copy a clean PNG or save it to disk.

Scanned PDFs can be captured as images; OCR is not included. Password-protected PDFs request a password for the current session. The original PDF is never modified.

The clipping tray is temporary: save images or copy excerpts before quitting. Text copied through the reader includes the PDF filename and page number. Images use the original page colors, even in night mode.

## Local packaging

`npm run package:mac` produces `dist/Sumi.app` with the Rust shell and embedded local frontend. This personal build is ad-hoc signed, not notarized. Building on Apple Silicon produces an Apple Silicon app.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Open PDF | Command O |
| Find text | Command F |
| Previous / next page | Left / right arrow; Shift Space / Space |
| Visual page navigator | G or Command G |
| Bookmark current page | Command D |
| Zoom in place | Command + / - or pinch |
| Fit the complete page | Command 0 |
| Move an enlarged page | Space + drag |
| Focus mode | Command Shift F |
| Capture a rectangle | Command Shift X |
| Copy selected text with source | Command C |
| Cancel corner drag / close overlay / cancel capture | Escape |

## Local books for AI agents

The optional [local book library](docs/local-library.md) indexes personally owned PDFs with macOS PDFKit and SQLite. Its read-only stdio MCP server lets an agent search Japanese keywords, inspect a table of contents, and retrieve a few cited pages. It needs no API key, embedding model, network service, or Python packages. Client configuration is explicit; the app does not alter Codex or Claude settings.

## Verification and limits

See [verification notes](docs/verification.md) for the native WebKit and real-book results. PDFs are limited to 250 MiB. The reader keeps the current page visible while it renders the next page offscreen, then curls a diagonal paper corner over the prepared page. Dragging controls the bend and release; returning the corner cancels the turn. Rapid requests cancel older preparation. Reduced motion uses an immediate complete-page replacement.

Steady-state rendering holds at most two pages, each capped at 14 million pixels. Preparation temporarily adds the next one or two pages; the paper animation uses one temporary canvas capped at 3.5 million pixels. Old canvases are released after each turn. The navigator caches at most 36 small thumbnails and runs two render jobs at a time. Inline zoom refines the existing page canvas after the gesture settles, with the same 14-million-pixel cap per page. Clippings and extracted-page caches are also bounded.

OCR, annotation editing, continuous scrolling, and cloud sync are not included. PDF extraction follows the source document's text order; complex multicolumn pages can need cleanup after copying. This personal macOS build is ad-hoc signed and has not been notarized.
