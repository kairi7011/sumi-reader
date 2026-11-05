# Sumi 0.6 tabs and page-picker validation

Recorded on Apple Silicon macOS, 2026-11-05. The pinned Home tab, book tabs, compact controls and continuous page picker are integrated and checked in the packaged 0.6 debug app. Release packaging and delivered-application checks have completed. The [0.5 bookshelf report](bookshelf-verification.md) is historical evidence; its measurements are not relabeled as 0.6 results.

## Automated checks

**35 Rust tests** passed: 23 library tests and 12 binary tests, with one opt-in search probe ignored. **14 Python local-library/MCP tests** and `npm run check` passed. The native debug application built and packaged successfully.

The [sanitized nine-book smoke report](tabs-reader-smoke.json) records **198 checks with zero failures**. It checks body-page navigation, forward/back single pages, spread changes, inline 125% zoom and complete-page fit. Phased internal input covers committed, cancelled and reversed horizontal scrolls, ignored momentum/vertical-dominant input, and isolation while zoomed or on Home.

Each book also exercises Escape staying on Home, reopening without duplicate tabs, restoring page/zoom/find text after switching books, realizing and revealing the final page in the virtual grid with at most 36 cached previews, and closing a tab without removing its library entry. No external UI queries ran during animated turns. The committed report contains only generic book ordinals, page numbers/counts, geometry, fit/zoom states and pass results; it contains no titles, filenames, private paths or book text.

## Confirmed GUI checks

- At 1160 × 800, Home showed the cover shelf and compact controls without the old heading, caption or separate filter field. The four native Lucide icons were visually consistent. Escape kept Home selected.
- Command K opened the unified palette. Pasting a Japanese title query and pressing Enter opened a second book tab.
- In a 541-page book, the page picker initially showed the current page 83. Setting the native accessibility scrollbar to its start exposed pages 1–36; an accessibility scroll-down action moved continuously to approximately 0.048 of the range, exposing pages 19–60. Setting it to the end reached page 541, and the final rows were visually checked. Clicking page 541 opened that page.
- After enlarging that book to 125%, Command 2 switched to the other book at page 83 of 384. Command 3 restored page 541 of 541 at 125% zoom.
- Command W closed the active book tab and returned to the other book at page 83. Command 1 selected Home, and Escape stayed there. The closed book remained on the shelf with page 541 saved.

## Code review checks

The reviewed transitions settle the current page turn before saving on Home, tab switch or tab close. A failed save prevents those transitions, keeps the current document state and schedules a retry. Replacing Find with another panel retains its query. Book-title matches are capped before app commands are appended, so a large shelf cannot displace every command. These are code-review observations, not fault-injection GUI results.

## Delivered release

The [0.6 release manifest](tabs-release.json) records the ARM64 build from `43ca21a56832af140efc6ab13fb26ae45b0d23d4`. Application files total **1,110,941 bytes**; the ZIP is **525,067 bytes**. Deep strict signature verification passed for both the built bundle and the delivered copy, and ZIP integrity verification passed. Signing is ad-hoc; no notarization was performed. The executable uses system frameworks without WebKit. The Lucide source assets and ISC/Feather MIT licenses are bundled.

Delivered artifacts are `outputs/Sumi.app` and `outputs/Sumi-0.6.0-mac-arm64.zip`. ZIP SHA-256: `9edd60930a2fec592f468511299ed0792b26f27f9d828e10bc3531c4fefbe6e8`.

Before replacement, the running 0.5 app was at the user's current page **124 of 384**. Quitting preserved that position. The delivered 0.6 app opened on its 10-book shelf with Home selected, and opening that book created a tab at page 124. Command 1 returned to Home; Escape stayed there. Existing positions of **32 of 333** and **2 of 400** were also observed before any change. These were the normal user-library records; QA reading positions were not copied into them.

## Verification limits

| Area | Status |
| --- | --- |
| Physical scrolling | No physical trackpad or coordinate-wheel input was verified. Native accessibility scrolling passed. The coordinate-wheel tool returned `noWindowsAvailable`, including after a reset. |
| Scrollbar dragging | Common-mode UI timer scheduling is implemented, but physically dragging the thumb during thumbnail rendering was not verified. |
| Broader GUI regression | This run does not establish full keyboard/focus traversal, Japanese IME composition, minimum-window layout, import/capture, missing-file relink, password cancellation or save-failure behavior for 0.6. Earlier reports remain version-specific. |
| Session scope | Open-book tabs are in-memory sessions. Restoring the tab list after relaunch is not implemented or claimed. |
| Performance | No new animation timing or memory sample was taken for 0.6. Internal phased inputs are not physical-trackpad validation or a display-frame-rate measurement. |

## Dependency binding

Native and npm application metadata are 0.6.0. The version update changed only local package entries; exact comparisons confirm the same 31 external Rust packages and unchanged npm dependency records, without resolution or downloads for that metadata update. The [native dependency audit](native-dependency-audit.json) retains prior hash bindings and the original artifact/advisory evidence.

The four static Lucide 0.468.0 assets have separate [source, integrity and license provenance](../assets/icons/lucide/provenance.json). Their geometry is drawn natively without an icon runtime or npm dependency; the source SVGs and ISC/Feather MIT licenses are retained with the bundle.
