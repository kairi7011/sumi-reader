# Sumi 0.6 tabs and page-picker validation

Draft status, 2026-11-05. The pinned Home tab, open-book tab strip, compact controls and continuous page picker are being integrated. Product tests, GUI checks and release packaging have not yet been reported for this version. The [0.5 bookshelf report](bookshelf-verification.md) is historical evidence, not a completed 0.6 validation run.

## Confirmed metadata check

Native and npm application metadata are 0.6.0. Only local package-version entries changed. Exact manifest/lock comparisons confirm the same 31 external Rust packages and unchanged npm dependency records. No resolution, downloads, Cargo commands or builds were used for this update. The [native dependency audit](native-dependency-audit.json) retains prior hash bindings and the original artifact/advisory evidence.

## Checks awaiting execution

| Area | Required observation | Status |
| --- | --- | --- |
| Pinned Home | Launch opens Home; Escape dismisses overlays without switching into the last-read book; Home remains present while books are open. | Pending |
| Compact controls | Import and unified search use compact icon buttons with accessible names; obsolete shelf heading/caption/filter controls are absent; reader zoom reset remains contextual. | Pending |
| Book tabs | Opening and selecting books keeps the visible tab and PDF identity consistent; switching preserves each book's page/bookmarks; opening the same book has a deliberate, consistent result. | Pending |
| Tab lifecycle | Closing or removing a book cannot leave a stale active document; failed/missing/password-cancel opens retain the previous screen and selection; Home cannot disappear. | Pending |
| Unified shelf search | Search icon, Command K and Home Command F reach the same palette; reader Command F still searches only the visible book; closing search restores the correct screen focus. | Pending |
| Continuous page picker | A long book exposes all pages through one vertical scroll range; entering a distant page scrolls it into view; clicking/keyboard selection opens the intended page. | Pending |
| Virtualization | Rendered controls and cached previews stay bounded while scrolling from first to last page; book/tab changes discard stale thumbnails and pending work. | Pending |
| Input isolation | Shelf/picker scrolling does not turn the hidden reader; native text editing and IME composition keep their normal behavior; layout fits the minimum window size. | Pending |
| Regression checks | Current Rust/Python tests and build checks, reading gestures, import, capture, global preferences and per-book persistence are rerun as appropriate. | Pending |
| Release artifacts | The 0.6 source revision, signed ARM64 bundle, ZIP integrity, sizes/hash and delivered-build checks are recorded. | Pending |

Synthetic input checks must remain distinct from physical-trackpad testing. No 0.6 animation timing, memory measurement or hardware-gesture validation is claimed here. Earlier-version measurements must retain their original version and source attribution.
