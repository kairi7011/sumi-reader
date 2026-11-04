# Sumi 0.3.0 verification

Verified on macOS 15.8.1, Apple Silicon, 2026-11-04. The release uses Tauri 2 with the system WKWebView; Electron and its runtime are removed.

## Checks

- 34 JavaScript tests: reading-state bounds, Japanese text/search geometry, capture bounds, native-open races, startup ordering, WebKit-compatible text extraction, bounded page navigation, paper-turn lifecycle and texture work, and metadata/cover title derivation.
- 14 Python tests: local indexing, Japanese search, bounded reads, read-only MCP, modern and legacy protocol messages.
- 8 Rust tests: reading-state/PNG limits, history migration, authorized file reads, corrupt-history recovery, stale-open suppression, app-wide preference migration/persistence, and authorized title updates.
- 72 checks in a real Tauri WKWebView using authored Japanese, English, and encrypted fixtures. Native text/PNG clipboard, search, password feedback, bookmarks, single/spread turns, corner rollback/commit, rapid navigation, reduced motion, inline zoom and scaled capture, direct page navigation, compact layout, and global settings across books were checked.
- 173 native checks across an authored fixture and nine personally owned technical books. All nine reached a dense body page in addition to cover/early pages. Book text, PDFs, and private screenshots are excluded from Git and the application bundle.

Machine-readable results: [authored fixtures](tauri-smoke-fixtures.json), [technical books](tauri-smoke-books.json), [immediate page-navigation regression](tauri-page-map-race.json), [native visibility behavior](tauri-visibility.json). The debug-only harness is excluded from the release executable.

The page-map handoff is covered by focused native checks: direct and digit-by-digit page entry across eight preview delays from 0 to 2,500 ms, plus sixteen rapid map reopenings retaining at most two thumbnail jobs. Eighteen checks passed with the minified release PDF.js frontend in the native shell. Reader pages are protected during preparation, and bounded thumbnail work finishes without cancelling shared PDF.js streams. Packaged-app observation also identified a separate native visibility effect: WKWebView could suspend page-render animation callbacks while its window was obscured; raising the same window immediately resumed the pending render. Native diagnostics confirmed that the operator lists were already loaded while the animation-frame heartbeat stopped in a hidden document. Bringing the window forward resumed the same pending render. Background-throttling settings did not override this WebKit behavior and were not retained. The app does not run a forced background drawing loop.

The nine-book run plus authored fixture sampled 414 animation/navigation frames and found zero missing or zero-sized visible canvases. Actual-book curl frame intervals measured 17–22 ms at the median, 26–29 ms at p95, and 26–30 ms maximum. The separate authored-fixture run had one initial 95 ms outlier; its p95 was 26–27 ms. These are observations on this Mac, not a 60 fps or general performance guarantee. Static visual checks covered forward/backward and single/spread poses, including the printed backside and the former white triangle seams. Pixel-by-pixel correctness across every page has not been established.

## Reading changes

The normal view fits the complete page or spread without reader scrolling. A 36 px titlebar and 34 px floating navigation capsule keep the page area clear; library, search, and tools are overlays. The page navigator offers direct numbering, a preview slider, a virtualized thumbnail grid, and return to the prior position.

Zoom transforms the existing page in place and sharpens its canvas after the gesture settles. Pinch and Command + / − change scale; Command 0 fits the full page. Space-drag or a trackpad moves the enlarged page. Text selection remains aligned, and rectangle capture converts pointer coordinates back into page coordinates. No magnifier window or auxiliary reading pane is created.

Appearance, page layout, and reading direction are shared app preferences. Existing settings migrate from the last opened book once; individual books retain reading position and bookmarks. All nine authorized technical PDFs supplied usable titles in their metadata. The app uses these titles without renaming source files, and resolves titles in the existing bookshelf through sequential local metadata reads.

## Packaged app

The signed release was launched through macOS with the existing library. It restored the user’s 114–115 spread, replaced the opaque filename with the book’s metadata title, and kept both pages inside the window. Real keyboard input through G moved directly from page 114 to 200; the next-page key completed a turn to 202–203. Direct return to 114, the single/spread toggle, in-place 125% zoom, Command 0, and dismissal of tools all worked. Final visual inspection confirmed the thin titlebar, small floating controls, whole-spread fit, and no auxiliary magnifier. The app was left open at the original 114–115 spread.

The final package uses a stripped ARM64 binary and macOS frameworks. `codesign --verify --deep --strict` and `plutil -lint` pass. Signing is ad-hoc for personal use; no notarization was performed. Application files total 11,611,005 bytes (approximately 11.6 MB), compared with the previous 279 MiB Electron bundle. This is a disk-size comparison; whole-system WebKit memory was not benchmarked.

## Local book index

Nine authorized O'Reilly PDFs, 2,876 pages, were indexed locally. The database is 26,976,256 bytes; SQLite integrity checking passed. Reindexing skipped all nine unchanged files. Actual stdio discovery, Japanese keyword search, and page retrieval passed. The index lives outside the repository, with owner-only permissions. MCP client settings were prepared but not installed globally.

## Limits

Only Apple Silicon macOS was tested. PDF text extraction follows the source PDF; complex fonts, spacing, and columns can need cleanup after copying. Scanned pages have image capture but no OCR. The reader does not provide annotation editing or continuous scrolling. Clippings last for the current process only. No uploaded book content, telemetry, cloud sync, or AI API calls are part of the viewer or indexer.

Release ZIP: 4,565,474 bytes. SHA-256: `52fe1f6cc6e9eedeb028387c350a5158f85da94723f529d42f89283440ee2c90`.
