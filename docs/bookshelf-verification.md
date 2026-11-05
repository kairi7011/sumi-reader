# Sumi 0.5 bookshelf validation

Recorded on Apple Silicon macOS, 2026-11-05, for code at `05c8ab0`. The bookshelf and simplified reader are integrated. Empty-library import QA, release packaging and delivered-application checks have completed. The [0.4 native report](native-verification.md) is historical evidence for the earlier implementation; its measurements are not relabeled as 0.5 results.

## Automated checks

**31 Rust tests**, **14 Python local-library/MCP tests**, and `npm run check` passed. The [sanitized nine-book smoke report](bookshelf-reader-smoke.json) records **126 checks with zero failures**.

Each real book was checked through body-page navigation, forward/back single pages, spread changes preserving the selected page, advancing a spread, inline 125% zoom and complete-page fit. Additional internal checks exercise a committed horizontal scroll, ignored momentum/vertical-dominant input, cancelled and reversed scroll rollback, no page turn while zoomed, and an inactive reader while the shelf is visible.

The scroll inputs are phased events supplied by the internal debug driver. **No physical trackpad was operated.** No external UI queries ran during the animated turns. The report contains only generic book ordinals, page counts/numbers, fit/zoom states and pass results; it includes no titles, filenames, private paths or book text.

## Confirmed GUI checks

- At the default 1160 × 800 window and minimum 680 × 480 window, the shelf's first row of covers, titles and reading progress fit without overlap.
- Command L returned from a real book to the shelf; Right changed the selection and Enter opened the next selected book. Sequential interaction restored its saved page 84 of 333.
- The reader showed two labeled header buttons, a page-counter footer and a zoom-reset control only while enlarged. The palette displayed explicit target-state labels.
- Reader Command F opened search; a second Command F and Escape closed it. Clicking the page body also closed an empty search field.
- An isolated empty shelf showed its import action and drop instructions. Selecting a folder in the native file picker imported 45 authored PDFs and reported its one invalid file; the shelf stayed open with 45 cards and an import-result panel. Escape dismissed the result, and Enter opened a selected book.
- Shelf Command F followed by pasting an authored book title narrowed the shelf to 1 of 45 books. Enter opened it; Command L retained the filter, and Escape cleared it to restore all 45. A previous Japanese filter query also returned matches, but accessibility value assignment does not establish IME-composition correctness.
- Right-margin clicks advanced pages 83 → 84 → 85; a left-margin click returned to 84. After zooming to 125%, clicking the margin did not change the page; Escape restored fit.

## Delivered release

The [0.5 release manifest](bookshelf-release.json) records the ARM64 build from `05c8ab03b0966e566145887cf33a61efa2f74d8b`. Application files total **1,083,135 bytes**; the ZIP is **502,477 bytes**. Deep strict signature verification passed for both the built bundle and the delivered copy, and the ZIP integrity test passed. Signing is ad-hoc; no notarization was performed. The executable uses system frameworks and has no WebKit dependency.

Delivered artifacts are `outputs/Sumi.app` and `outputs/Sumi-0.5.0-mac-arm64.zip`. ZIP SHA-256: `d7a9c852ea5ba3b76c11ddc390c853cc4ca43347edfc0a846360533fa3d41f8a`.

The previous application was quit after saving page 126. The delivered 0.5 app opened on its 10-book home shelf, showed the real book at **126 of 384**, and reopened that cover at page 126 in spread mode. Zooming to 125% and pressing Escape fitted both pages again. Command L returned to the shelf with page 126 preserved. The existing paper/spread/left-to-right preferences remained unchanged.

No memory benchmark or RSS sample was taken for 0.5.

## Verification limits

| Area | Status |
| --- | --- |
| Import-flow breadth | Empty shelf, native folder import, invalid-file feedback and staying on the shelf confirmed. Duplicate feedback and explicit Finder-open behavior still lack direct GUI evidence for 0.5. |
| Shelf keyboard and text-input breadth | Right/Enter/Command L confirmed. Full Tab/focus traversal, all four directions and Japanese marked-text composition are not yet established by GUI evidence. |
| Book-maintenance GUI | Current backend tests cover reference/state safety; new-shelf missing-file relink, password cancellation and removing the retained last-read card still need direct GUI evidence. |
| Physical trackpad | Not verified. Synthetic phase, cancellation, direction and shelf-gating checks passed. |
| Focus/fullscreen GUI | No new claim: focus hides chrome; standard macOS fullscreen remains separate. |

## Dependency binding

The native and npm application versions are 0.5.0. Only local package-version entries changed. Exact comparisons confirm the same 31 external Rust packages and unchanged npm dependency records, without resolution or downloads. The [native dependency audit](native-dependency-audit.json) records previous and current manifest/lock hashes and retains the original artifact/advisory evidence.

The 0.4 animation, search and memory measurements remain historical samples. Synthetic input checks must not be presented as physical-trackpad validation or a guarantee of display frame rate.
