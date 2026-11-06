# Native 0.8.0 verification

Verified on an Apple Silicon Mac on 2026-11-06. The packaged app remains Rust + AppKit/PDFKit; no browser engine or new external dependency was added.

## Automated checks

- `npm run check` passed.
- `npm test` passed 76 tests: 45 Rust, 20 local-library, 6 public-audit, and 5 mirror-ordering tests. One opt-in private-book probe was not run.
- After the final language-refresh fixes, the native checks and all 45 Rust tests passed again.
- `npm run package:mac`, strict local codesign verification, native license/linkage/path checks, and ZIP integrity verification passed.
- The app contains 35 files, totals 2,857,393 bytes, and links nine system libraries. This is bundle size, not a measurement of runtime memory.
- Packaged executable SHA-256: `d623069f7797d7d6e5de6084b3a3aeb06f9a7e72ebd69ba8cbdce76f9f835606`.

## Packaged-app checks

Used an isolated library containing the project's three original demo PDFs. Verified English/Japanese switching, English preference persistence after relaunch, localized menus, hints, shelf cards, tabs and page picker, and compact icon buttons after switching languages.

With page 2 being read and page 5 selected in the picker, changing language through the app menu preserved both positions. Automated tests cover the migration of older preferences and rejecting an invalid language without changing saved settings.

Verified a typed `feedback` query returns passages from all three books and selecting Search Design opens PDF page 4 with the matching word selected. Also exercised facing spreads, keyboard page turns, in-place zoom, Escape to fit, thumbnail navigation, a page-number jump, opening a second book tab and returning to the prior book, rectangular image copy, and focus mode.

These interactions are visible in the [actual-operation recording](media/README.md). It uses the packaged executable and real input operations. It is not a physical trackpad test, a benchmark, or a slideshow. The capture hint and copy confirmation are visible; the brief selection rectangle is not clearly discernible in the recording.

## Distribution limits

This is an ARM64 macOS 13+ preview, ad-hoc signed and not notarized. ScreenCaptureKit recording tools require macOS 15+, but they are development tools and are not part of the reader bundle. Public pattern scans are not a complete security or copyright audit; new media was separately inspected for private content.
