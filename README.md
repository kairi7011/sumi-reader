# Sumi

A quiet, personal PDF reader for macOS. Built for long reading sessions and taking excerpts into your own AI conversations.

Private project owned by **kairi7011**. Documents stay on your Mac. There is no account, telemetry, cloud upload, or AI API integration.

## Development

Sumi 0.2 uses Tauri 2 and the macOS system WebKit. It does not bundle Electron or Chromium. PDF.js uses its compatibility build for WebKit.

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
- Fit a page or its width, zoom, switch paper colors, and hide controls for focused reading.
- Resume reading from recent documents; bookmark pages and navigate the PDF outline.
- Search document text, select text, and collect excerpts with their source page.
- Drag a rectangle over a page to copy a clean PNG or save it to disk.

Scanned PDFs can be captured as images; OCR is not included. Password-protected PDFs request a password for the current session. The original PDF is never modified.

The clipping tray is temporary: save images or copy excerpts before quitting. Text copied through the reader includes the PDF filename and page number. Images use the original page colors, even in night mode.

![Sumi displaying an original Japanese test document](docs/reader-preview.png)

## Local packaging

`npm run package:mac` produces `dist/Sumi.app` with the Rust shell and embedded local frontend. This personal build is ad-hoc signed, not notarized. Building on Apple Silicon produces an Apple Silicon app.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Open PDF | Command O |
| Find text | Command F |
| Previous / next page | Left / right arrow |
| Bookmark current page | Command D |
| Zoom | Command + / - |
| Fit page | Command 0 |
| Focus mode | Command Shift F |
| Capture a rectangle | Command Shift X |
| Copy selected text with source | Command C |
| Cancel capture / leave focus mode | Escape |

## Local books for AI agents

The optional [local book library](docs/local-library.md) indexes personally owned PDFs with macOS PDFKit and SQLite. Its read-only stdio MCP server lets an agent search Japanese keywords, inspect a table of contents, and retrieve a few cited pages. It needs no API key, embedding model, network service, or Python packages. Client configuration is explicit; the app does not alter Codex or Claude settings.

## Verification and limits

See [verification notes](docs/verification.md) for the native WebKit and real-book results. PDFs are limited to 250 MiB. The reader keeps the current page visible while it renders the next page offscreen, then curls the existing pixels over the prepared page. Rapid requests cancel older preparation. Reduced motion uses an immediate complete-page replacement.

Steady-state rendering holds at most two pages, each capped at 14 million pixels. Preparation temporarily adds the next one or two pages; the paper animation uses one temporary canvas capped at 6 million pixels. Old canvases are released after each turn. Clippings and extracted-page caches are also bounded.

OCR, annotation editing, continuous scrolling, and cloud sync are not included. PDF extraction follows the source document's text order; complex multicolumn pages can need cleanup after copying. This personal macOS build is ad-hoc signed and has not been notarized.
