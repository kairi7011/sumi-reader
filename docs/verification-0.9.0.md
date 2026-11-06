# Native 0.9.0 verification

Verified on an Apple Silicon Mac on 2026-11-06 using a separately identified QA app and an isolated library containing the project's three original demo PDFs. The automated, isolated UI and recording checks below used no private books or normal user-library records. The separate local-update check is described at the end.

## Automated checks

The completed full suite passed **93 tests**, with one opt-in book-search probe ignored. This count belongs to that full-suite run; the UI checks and recording below provide separate evidence of native behavior.

## Confirmed native UI checks

- Startup restored three book tabs in their saved order, with the middle tab selected at page 4 in spread view.
- Selecting Home, quitting and relaunching retained Home and all three ordered book tabs.
- Quitting with another book selected at page 4 and 125% zoom restored that book, page and zoom. Command F restored its saved `feedback` query.
- Closing all book tabs, quitting and relaunching retained Home with no open book tabs. All three books remained in the library.
- Opening a book and immediately pressing Escape returned to Home. The cancelled book did not replace Home when its delayed load completed. Rapidly opening two books left only the latest requested book active, at its saved page 4.
- Shelf covers and all five page-picker previews rendered correctly for the demo fixture. Night theme and single-page preference were retained.
- A native Right-key page turn advanced page 2 to page 3 after the controlled blocking operation used in the recording.
- When the QA store was made unwritable, Command Q showed a save error and kept the app open. Restoring write access allowed Command Q to save and exit.

These were actual native UI operations. They do not establish fault handling for every filesystem error, physical trackpad behavior, or performance across arbitrary PDFs.

## Recorded loading feedback

A 35-second ScreenCaptureKit recording captured only the QA app window, without audio. Debug-only delays made the asynchronous book load and a main-thread operation measurable. Application event timestamps were aligned with actual video presentation timestamps; the recording was not reconstructed from screenshots.

The original recording SHA-256 was independently checked against its capture metadata:

`56ff03ba9bd8efb96ffeb5752febc43dc718495f6399fe69683e086cf8345792`

AVFoundation decoded 72 original frame crops for review. The retained review artifacts record each requested time, actual source time and crop PNG SHA-256. Contact sheets only arrange and resize those decoded frames; no frames or motion were synthesized.

| Check | Observed result |
| --- | --- |
| Asynchronous loading | The application requested the header spinner **406 ms** after the book-open request. The video shows it rotating and disappearing after completion. The 406 ms value is event-trace timing, not a claim about the exact first displayed pixel. |
| Main-thread delay | The measured block lasted **2,502 ms**. The lower-right Core Animation ring visibly changed orientation during that interval, while the main thread remained blocked. |
| Ring appearance | The first sampled visible source frame was approximately **460 ms** after arming. This is consistent with the configured 400 ms delay and 80 ms fade-in; variable-frame-rate capture and sampling bound the observation rather than identify the exact onset. |
| Ring removal | The ring was absent in a sampled source frame approximately **78 ms** after the block ended. |
| Appearance | The ring remained clearly visible against Night theme without covering the page content. |

An earlier recording did not exercise the intended main-thread delay and was excluded from that conclusion. The aligned recording above is the evidence for rotation during a confirmed block.

## Verification limits

- This run does not separately prove suppression for a controlled 200 ms operation. It also does not measure general PDF-opening speed, idle CPU usage, memory use or sustained display frame rate.
- PDF passwords are excluded from the persisted session model; the UI checks above do not claim a new encrypted-document relaunch test.

## Local installation check

The previous app and user data were backed up before installing the verified 0.9.0 bundle into `/Applications/Sumi.app`. The installed executable matched the packaged executable by SHA-256, kept the existing bundle identifier, passed strict code-signature and native-package checks, and was observed running from that installed path.

The normal library retained its book IDs, reading states, bookmarks, shared preferences and last-opened book. The existing search-index directory was unchanged by a file-hash comparison. A saved book opened at its existing reading position; quitting and reopening the installed app restored that selected tab and position. The app was then returned to its pre-update Home selection with no open tabs. Private book metadata and user-data contents are excluded from this report and the release artifacts.
