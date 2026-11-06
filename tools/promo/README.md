# Record a Sumi demo

[日本語の手順](README.ja.md)

These tools prepare original sample PDFs and record or process the running app. Show real typing, search results, and navigation in one continuous recording. Use the macOS Screenshot toolbar or the authorized ScreenCaptureKit recorder below. The tools do not operate the app UI.

The scripts use Apple's AppKit, PDFKit, and AVFoundation frameworks. They need macOS 15 or later and Apple Command Line Tools, with no external packages or downloads. Run commands from the repository root.

## Prepare English or Japanese books

```sh
swift tools/promo/create-demo-books.swift --language en --output test-artifacts/promo-fixtures-en
swift tools/promo/create-demo-books.swift --language ja --output test-artifacts/promo-fixtures-ja
```

Each language produces three original five-page PDFs with selectable text, a cover, title metadata, and outline entries. The generator reopens each PDF with PDFKit, checks its text and query matches, and writes `verification.json` plus a PNG preview of every page. No text, illustrations, or covers are taken from published books.

| File | English title | Page matching `feedback` | Japanese title |
| --- | --- | --- | --- |
| `interaction-design.pdf` | Interaction Design | 2 | 操作の設計 |
| `search-design.pdf` | Search Design | 4 | 検索の設計 |
| `reading-design.pdf` | Reading Design | 3 | 読書の設計 |

In English, `feedback` occurs exactly once per book, on the page above. In Japanese, search for `操作の結果を伝える` to get one matching page per book. `フィードバック` produces several matching pages per Japanese book. Page numbers include the cover.

The default language is Japanese and the default output is `test-artifacts/promo-fixtures`. A positional output directory remains supported. Generated books, previews, and verification files under `test-artifacts` are ignored by Git. The text and code use the repository's Apache-2.0 license. The notes illustrate app interactions, rather than reporting research findings.

## Record actual interactions

Use a separate demo library containing only these three books. Set the app language to match the books before recording. Keep personal books, file paths, notifications, and unrelated windows outside the recorded area.

1. Open the bookshelf with the sample books visible. Start recording the app region with the macOS Screenshot toolbar. Leave the microphone off unless narration is intended.
2. Open search and visibly type `feedback` for English, or `操作の結果を伝える` for Japanese. Let the results settle so the viewer can read all three book titles.
3. Select a result from another book. For English, **Search Design, page 4** makes the destination easy to verify. Pause on the opened page and matching text.
4. Demonstrate a few reading actions: turn a page, change single/facing-page view, choose a page in the page picker, zoom in and return to fit, or switch open book tabs. Leave enough time to see each result.
5. Stop recording and retain the original MOV locally. Review it before choosing the beginning, end, and crop.

Keep the search-to-page sequence continuous, with the original interaction speed and loading behavior. A shorter recording can omit optional reading actions; avoid cutting out a wait or presenting generated screenshots as recorded interaction. Apple's [screen recording instructions](https://support.apple.com/en-us/102618) describe the recording controls.

## Trim, crop, and export

```sh
swift tools/promo/process-recording.swift test-artifacts/recording.mov \
  --output test-artifacts/demo-en.mp4 \
  --start 2 --end 45 \
  --width 1920 --height 1080 \
  --mute \
  --previews test-artifacts/demo-en-preview \
  --report test-artifacts/demo-en.json
```

Replace the example times with the actual recording's start and end. Omit them to retain the full video. The tool exports H.264 MP4 and preserves source frame timing, including variable-rate recordings. It applies only a continuous trim, fixed crop, and proportional resize. It adds no UI, captions, transitions, freeze frames, or speed changes.

`--width` and `--height` are maximum bounds, not a forced aspect ratio. The image is never stretched or enlarged; output dimensions are rounded down to even pixels. Audio is retained unless `--mute` is supplied. An existing MP4 is never overwritten.

Use `--crop X,Y,WIDTH,HEIGHT` to remove the desktop around the app. Coordinates are pixels from the top-left of the displayed recording, after orientation is applied. Retina pixels may differ from window coordinates in points. Keep the whole search panel, destination page, and page number visible. Cropping is optional; check the source dimensions before choosing values.

The exporter decodes the entire output and checks its codec, dimensions, duration, and increasing frame timestamps. `--report` saves JSON with source and output SHA-256 hashes; `--previews` saves the first, middle, and near-final frames. Review those frames and play the full MP4 to confirm that input, result choice, and the destination remain legible. Decoding checks do not verify the recording's content or privacy.

The implementation uses Apple's [crop rectangle API](https://developer.apple.com/documentation/avfoundation/avmutablevideocompositionlayerinstruction/setcroprectangle%28_%3Aat%3A%29) and [asset export API](https://developer.apple.com/documentation/AVFoundation/AVAssetExportSession/export%28to%3Aas%3Aisolation%3A%29). It clears exported asset metadata and keeps reports to filenames, but filenames and recorded pixels still need review before publication.

## Record one authorized window with ScreenCaptureKit

`record-window.swift` records one explicitly selected app window and its child windows. Run it only after the user authorizes screen capture. Even `--list` may invoke macOS capture-permission controls. The `--allow-screen-capture` flag records the operator's intent; it does not bypass system permissions or establish user approval by itself.

Compile without starting capture:

```sh
swiftc -O tools/promo/record-window.swift -o test-artifacts/record-window
```

After authorization, record the intended bundle ID:

```sh
test-artifacts/record-window --allow-screen-capture \
  --bundle-id com.kairi7011.sumi.livepromo \
  --output test-artifacts/demo-window.mov --duration 65 --fps 30 \
  --report test-artifacts/demo-window.json
```

The recorder starts immediately, prints `recording started`, and stops after the requested duration. Its start and finish callbacks log ISO8601 wall-clock timestamps; the JSON report includes those times and the recording's SHA-256 hash. It includes the real cursor by default; `--hide-cursor` omits it. System audio, microphone audio, and decorative click effects are off. The window is captured at its Retina pixel scale, with no desktop or unrelated windows. Do not resize the window during the take. If multiple windows match, use `--list` to obtain their IDs and repeat with `--window-id ID`. Listing returns IDs and dimensions without window titles.

The FPS argument sets the requested capture interval, not a guarantee of delivered frames. Review the finalized recording and use `process-recording.swift` for decoding checks and any needed trim. This recorder uses Apple's [ScreenCaptureKit recording output](https://developer.apple.com/documentation/screencapturekit/screcordingoutput).

## Legacy static cards

`render-promo.swift` and `storyboard.json` remain available for screenshot-based promotional cards. Their screenshot sequence is not a recording of app interaction. For a live product demo, use a recorded MOV and `process-recording.swift`.
