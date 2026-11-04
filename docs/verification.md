# Verification — 2026-11-04

Environment: Apple Silicon macOS, Node 22.23.1, Electron 45.2.1, PDF.js 6.4.299.

## Automated checks

- `npm run check`: JavaScript syntax checks passed.
- `npm test`: ten tests passed, covering hostile asset paths, IPC payload validation, bounded reading state, backwards capture coordinates, Japanese text extraction, and search highlighting across PDF text runs.
- `npm run test:smoke`: thirty-two checks passed in an actual Electron window, with zero renderer errors. Fixtures contain original English and Japanese text. Japanese/password fixtures are generated using the system AppKit/PDFKit frameworks.
- Native PNG and text clipboard writes, drag capture, search, bookmark persistence, reload/resume, missing-file errors, and sandbox isolation passed.
- Password tests covered wrong password, correct password, cancellation, and replacing the document during a password prompt.
- Ten rapid page changes left one page rendered in single-page mode. Capture shortcuts were rejected until the current page finished rendering.
- Screenshots inspected at 1380×940 and 900×650, including both side panels. Japanese glyphs and text-layer alignment were verified visually.

## Resource limits and observations

- At most two rendered PDF pages; maximum 14 million backing pixels per canvas.
- Up to 32 extracted page texts cached. PDF page resources are cleaned after use.
- Capture resolution capped at six million pixels. Collection previews are at most 440 pixels on their longest side. Retained clips are bounded by count (24), encoded characters (32 million), and total image pixels (24 million).
- The final development smoke run reported an aggregate process working set of 537,040 KiB after rendering, copying, capturing, searching, and switching documents. This sum includes shared pages and is neither unique resident memory nor a clean-launch baseline. It is a limited observation, not a performance benchmark.
- No user ebook was used for testing. Large scanned textbooks and unusual font encodings still need representative real-book validation.

## Distribution

The package is an Apple Silicon `.app` with an original icon and PDF document association. It is ad-hoc signed for local personal use, not Developer ID signed or notarized. `codesign --verify --deep --strict` passed. The packaged application was launched through macOS and its native File menu opened and displayed the Japanese fixture. The app occupies approximately 279 MiB on disk. Electron and PDF.js license files are retained in the bundle.

Reading history and bookmarks live in `~/Library/Application Support/Sumi/library.json`. Passwords and clipping contents are not persisted. Original PDFs are never written to. No remote pages, telemetry, or uploads are enabled.
