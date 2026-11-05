# Sumi 0.4 native validation

Recorded on Apple Silicon macOS, 2026-11-05. This document covers the native Rust/AppKit/PDFKit implementation. The [0.3 verification report](verification.md) describes the retained Tauri version and is not evidence for the native release.

## Confirmed automated checks

The final code at `d90c38a` passed **24 Rust tests**: 22 library/search/thumbnail tests and two reader-navigation tests. `npm run check` also passed. The real-book search probe is a separate opt-in test.

- Importing 65 books alongside an existing book retains all 66 records and preserves the previous last-read book and position.
- Recursive imports deduplicate canonical paths and file symlinks, continue after invalid or missing files, skip directory symlinks, and report traversal-depth limits.
- Relinking preserves the book ID, title, position and bookmarks. Reusing its former path creates a distinct ID. Removing a library entry leaves its PDF untouched.
- Legacy preferences migrate from the last-read book once; later appearance changes remain global. Book state retains page and bookmarks independently.
- Failed writes leave the previous in-memory and on-disk state intact. Malformed history is backed up before recovery.
- An explicit absolute `SUMI_LIBRARY_PATH` isolates QA history without importing the user's normal library. Invalid relative, empty and directory paths are rejected.
- Search tests cover Japanese width/case normalization, line wrapping, original-character snippet offsets, normalized PDF highlight ranges, literal and bounded queries, cancellation, and stale-result rejection. The optional SQLite path is read-only, scoped to matching files, and rejects changed sources.
- Thumbnail tests keep distant targets in a bounded 24-page window and avoid invalid pages. Reader tests cover cover/spread navigation and corner-drag cancellation versus a click.

The **14 Python local-library/MCP tests** were rerun successfully on 2026-11-05. Native debug and release builds succeeded; final artifact checks are recorded below.

## Native reader and manual QA

The [sanitized reader smoke report](native-reader-smoke.json) records **72 checks across nine real books, with zero failures**. An internal debug driver checks body-page navigation, next/previous single pages, switching into and out of spreads while preserving the selected page, advancing a spread, 125% inline zoom, and fitting the complete page again. No external UI automation queries were issued during animations. These checks validate completed reader states; they are not frame-timing or texture-quality measurements.

Manual native QA confirmed:

- Command F opens search; Escape and clicking outside an empty field close it. Enter moved between results on pages 6 and 8, and Shift Enter returned to page 6. A full-width Latin query highlighted the matching PDF text.
- Command K finds book titles and actions. The page navigator displayed its 24-thumbnail grid. A corner drag moved page 80 to 81, and 125% zoom returned to complete-page fit.
- Importing the nine-book set added eight books, reported one duplicate, and reported no errors. Importing the authored recursive fixture added 45 PDFs and reported its one invalid file, with no duplicates; the current reader stayed on page 6 and the additions persisted.
- The password-protected authored PDF imported and unlocked with its fixture password. A captured rectangle was saved as PNG; its private QA artifact is excluded from Git.

The final packaged release was also checked at page **113 of 384** in spread mode. A short corner drag of about 25 Retina pixels cancelled without changing page 113, including after a subsequent zoom. In-place 125% zoom returned to fit with Escape. Reimporting the nine real books added eight, reported one duplicate and no errors, and retained page 113. Command K listed all nine books plus the existing fixture. The existing paper/spread/left-to-right preferences remained unchanged. Shifted-shortcut and decomposed-Unicode title-query fixes are included in the rebuilt release.

## Nine-book search sample

The [sanitized search report](native-search.json) records nine authorized technical books totaling **2,876 pages**. Each individual request completed without warnings in **55–215 ms**. Global search returned its first result in **135 ms** and reached its 80-result cap in **756 ms**; the second request took **12 ms** to first result and **76 ms** in total.

These are single-machine measurements using the existing local index. “Cold” means a fresh search-worker fingerprint cache; the operating-system cache was not cleared. Individual requests cover all nine books separately because the global request stops at its result cap. The report binds these measurements to its recorded source SHA-256; later source revisions are not covered by these timings. No book text, titles, filenames or private paths are included in the report.

## Dependencies and data

The native graph contains 31 previously audited registry packages. Cached archive hashes and eight approved build scripts were rechecked with no downloads; see the [native dependency review](dependency-review-native.md). The release executable links system PDFKit, AppKit, CoreGraphics, Foundation, CoreFoundation, SQLite, Objective-C, System and iconv libraries. It has no WebKit dependency.

The default library file is `~/Library/Application Support/com.kairi7011.sumi/library.json`. `~/Library/Application Support/Sumi/library.json` is a legacy migration source. Source PDFs stay at their original locations.

## Animation measurements

The [animation report](native-animation.json) compares two local debug runs of 27 turns each across nine books. Drawing the active turn overlay in its update tick retained the existing geometry and texture sampling. The median of per-turn median draw-start intervals was **16.656 ms**; the median of per-turn p95 intervals was **22.817 ms**, versus **33.335 ms** in the baseline. The largest observed interval was **45.488 ms**. Median preparation time was **19.376 ms**, and the maximum retained snapshot allocation was 2,525,454 pixels. A repeat reader smoke run with synchronous active-turn drawing passed **72 checks with zero failures**.

These intervals measure starts of overlay drawing, not screen presentation or GPU completion. The reported p95 aggregate is the median of per-turn p95 values, not a pooled percentile. No external UI queries or PNG captures ran during these measurements. This single debug-build sample does not establish 60 fps on every PDF or Mac, nor pixel-perfect curl rendering for every page.

## Packaged release and memory sample

The [release manifest](native-release.json) records the ARM64 application built from `d90c38abbbaae4ffe7bec83c46dfdbdb7e2f1a62`. Application files total **1,031,770 bytes**; the ZIP is **481,441 bytes**. Ad-hoc signature verification passed for both the built bundle and delivered copy, and the ZIP integrity test passed. No notarization was performed.

ZIP SHA-256: `6cb180abbb188c478c14a382850bde9142f664fd467e3bcf2e9ef0a5b6bf863b`.

One release-process sample showed **164.5 MiB RSS** and **128.9 MiB physical footprint**. It was taken with a real book's page-113 spread open after zoom/fit, an eight-book import and opening/closing the command palette. One reader process was present. This is a local observation after those operations, not a peak-memory measurement or a controlled comparison with another reader.

The authored internal-link fixture was prepared but its GUI behavior was **not verified**. Other coverage is limited to the tests and manual actions listed above; prepared fixtures alone are not counted as passed checks.
