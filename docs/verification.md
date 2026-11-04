# Sumi 0.2.0 verification

Verified on macOS 15.8.1, Apple Silicon, 2026-11-04. The release uses Tauri 2 with the system WKWebView; Electron and its runtime are removed.

## Checks

- 12 JavaScript tests: reading-state bounds, Japanese text/search geometry, capture bounds, native-open races, startup ordering, and text extraction on WebKit streams without async iteration.
- 14 Python tests: local indexing, Japanese search, bounded reads, read-only MCP, modern and legacy protocol messages.
- 5 Rust tests: state/PNG limits, Electron history migration, authorized file reads, corrupt-history recovery, and stale-open suppression.
- 40 checks in a real Tauri WKWebView using authored Japanese, English, and encrypted fixtures. Native text/PNG clipboard, search, wrong/correct password, bookmarks, single/spread forward/backward turns, rapid navigation, reduced motion, cleanup, and uncaught errors were checked.
- 114 native checks across an authored fixture and nine personally owned technical books. All nine reached a dense body page in addition to cover/early pages. Book text, PDFs, and private screenshots are excluded from Git and the application bundle.

Machine-readable results: [authored fixtures](tauri-smoke-fixtures.json), [technical books](tauri-smoke-books.json). The debug-only harness is excluded from the release executable.

The nine-book run plus authored fixture sampled 378 animation/navigation frames and found zero missing or zero-sized visible canvases. On actual books, observed maximum animation frame gaps were 18–20 ms, and next-page completion including the 360 ms curl was 418–505 ms. These are observations on this Mac, not a general performance guarantee. Pixel-by-pixel correctness across every page has not been established.

## Packaged app

The signed release was launched through macOS. Native file selection opened a real 384-page book. Page entry and next-page controls worked on body pages 128–129; dragging a capture produced a clip and native clipboard success feedback. Quit/relaunch restored page 129 and fit-width. Existing Electron reading settings migrated successfully. The final sidebar-order correction was checked in the packaged app: both the page control and book card advanced to page 130. Drag-and-drop is implemented through native Tauri events; the final manual check used the file picker.

The final package uses a stripped ARM64 binary and macOS frameworks. `codesign --verify --deep --strict` and `plutil -lint` pass. Signing is ad-hoc for personal use; no notarization was performed. Application files total approximately 12.4 MB, compared with the previous 279 MiB Electron bundle. This is a disk-size comparison; whole-system WebKit memory was not benchmarked.

## Local book index

Nine authorized O'Reilly PDFs, 2,876 pages, were indexed locally. The database is 26,976,256 bytes; SQLite integrity checking passed. Reindexing skipped all nine unchanged files. Actual stdio discovery, Japanese keyword search, and page retrieval passed. The index lives outside the repository, with owner-only permissions. MCP client settings were prepared but not installed globally.

## Limits

Only Apple Silicon macOS was tested. PDF text extraction follows the source PDF; complex fonts, spacing, and columns can need cleanup after copying. Scanned pages have image capture but no OCR. The reader does not provide annotation editing or continuous scrolling. Clippings last for the current process only. No uploaded book content, telemetry, cloud sync, or AI API calls are part of the viewer or indexer.

Release ZIP: 4,675,508 bytes. SHA-256: `09176eba2f19fc8f3fcbcf72780e0cdc7956ab015c5f352e2e14cdc6761dab5f`.
