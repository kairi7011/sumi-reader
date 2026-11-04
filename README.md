# Sumi

A quiet, personal PDF reader for macOS. Built for long reading sessions and taking excerpts into your own AI conversations.

Private project owned by **kairi7011**. Documents stay on your Mac. There is no account, telemetry, cloud upload, or AI API integration.

## Development

Requires an Apple Silicon Mac and Node.js 22.13 or newer. Dependency installation is deliberately script-free; consult `docs/dependency-review.md` before installing or updating dependencies. Runtime setup checks the official release age and pinned SHA-256 before extraction. Starting the app never downloads code.

```sh
npm ci --ignore-scripts --omit=optional
npm run setup:runtime
npm start
npm test
npm run package:mac
```

## Reading

- Open a local PDF or drop it onto the window.
- Read one page or a facing spread, with left-to-right or right-to-left navigation.
- Fit a page or its width, zoom, switch paper colors, and hide controls for focused reading.
- Resume reading from recent documents; bookmark pages and navigate the PDF outline.
- Search document text, select text, and collect excerpts with their source page.
- Drag a rectangle over a page to copy a clean PNG or save it to disk.

Scanned PDFs can be captured as images; OCR is not included. Password-protected PDFs request a password for the current session. The original PDF is never modified.

## Local packaging

`npm run package:mac` produces `dist/Sumi.app` using the installed Electron runtime. This personal build is ad-hoc signed, not notarized. Building on Apple Silicon produces an Apple Silicon app.

## Project status

Initial implementation in progress. Verification and known limitations will be recorded before delivery.
