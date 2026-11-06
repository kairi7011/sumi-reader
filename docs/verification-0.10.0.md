# Native 0.10.0 verification

Checked on an Apple Silicon Mac running macOS 15.8.1 on 2026-11-06. Native UI testing used an isolated library pointing to five authorized, real technical books, approximately 4.7–24.5 MiB each. The normal library was not replaced with test data. Private titles, paths, book IDs, page images and recordings are excluded from this repository and release assets.

## Automated checks

`npm run check`, `npm test` and `npm run package:mac` passed. The suite passed 105 tests, with one opt-in private-book probe ignored: 41 library tests, 33 native UI/model tests and 31 Python tests. Focused cases cover stable tab identity, rapid close/reopen, overflow during close-all, distinct Command W/Command Shift W handling, and the loading deadline across asynchronous preparation and native attachment.

## Actual reading sessions

- Opened five technical books using the native bookshelf, switched between them and checked their existing reading pages.
- Quit normally with the fifth book selected. Relaunch restored the same five tabs in order and selected the same book at its saved page. Only that book was prepared during restoration; background tabs did not open their PDF documents.
- Closed middle tabs, reopened a book, and used Command Shift W from both the reader and Home. All book tabs closed while all nine library entries, reading states and shared preferences remained.
- Made only the isolated store directory unwritable. Command Shift W showed a save error and retained all five tabs and their selection. Restoring directory access allowed the operation to finish.
- After closing all tabs, quit and relaunched again. Home remained selected with no book tabs; the library was intact.

## Timing and resource observations

Debug-only event traces distinguish document preparation, native attachment and ready state from the time taken by the automation tool to inspect PDF accessibility content. A process sample showed substantial PDF accessibility hierarchy parsing during inspection; tool-call duration is therefore not used as a reading-performance measurement. Accessibility remains enabled.

The five-tab relaunch after the manual reading round prepared its selected document in 67.3 ms and reached the application's ready event in 165.8 ms. These are warm, instrumented observations on one Mac, not cold-start or frame-presentation guarantees. Several repeated switches reached ready in 157–324 ms. An initial restore took 1.87 seconds, and one first switch took 16.01 seconds, principally inside PDFKit attachment/navigation; its repeat took 176 ms. The cause of that outlier was not isolated, so this report does not claim every first open is fast.

During one 90-second observed reading interval, resident memory ranged from approximately 78 MiB to a peak of 236 MiB. That interval includes accessibility inspection and does not cover every interaction or establish a memory ceiling. No additional cache of full PDF documents was introduced. Startup no longer loads shelf covers when restoring directly into a book, and restoring an already fitted tab skips a redundant fit calculation.

## Loading and animation behavior

The 400 ms deadline now starts with the book-open request and continues through native display work. The blocking-operation ring takes over at the header indicator's position and inherits the elapsed reveal/rotation state. Visible completion fades over 120 ms; completion before the deadline cancels the unseen indicator. Tests also cover a new operation during dismissal and prevent a fading indicator from jumping to a different location.

Tab controls retain their identities. Closing controls become inert immediately while their visuals finish; reopening during that interval reuses the visual with current action targets. Initial session restoration does not animate all tabs into place, and macOS Reduce Motion disables the transitions. Native Core Animation drives the 200 ms position, width, opacity and selection-background transitions independently of the main run loop.

ScreenCaptureKit captured the actual QA window during reading and tab operations. These local recordings contain private book material and are not distributed. Their source hashes are retained for the local verification record:

- Reading interval: `9f3b4dfcaecc57d1354455e9c17406df565f628b65ae0047e6f913dc7acb76a3`
- Tab operations: `e5568ad35115e38fb4e3045e27fc851ec9137c2ccc78f78e23d867294eaac01f`

This run does not establish sustained frame rate, behavior for every PDF, physical trackpad performance or a cold-system performance bound.
