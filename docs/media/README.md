# Sumi promotional media

The current images show native Sumi 0.8.0 in English with three original sample PDFs. The source prose and generator are in `tools/promo/create-demo-books.swift`. Purchased books, the personal library, and private reading history are not used.

## Actual app operation

[Watch the 37-second workflow](https://github.com/kairi7011/sumi-reader/releases/download/v0.8.0/Sumi-0.8.0-workflow-en.mp4).

This is a continuous ScreenCaptureKit recording of the demo app window. It shows typing `feedback`, matches from three PDFs, selecting Search Design and opening PDF page 4, facing pages, page turns, in-place zoom and Escape to fit, the thumbnail picker and page-number jump, reading tabs, rectangular image copying, and focus mode.

Only the idle beginning and ending were trimmed. The clip retains real timing: no screenshot montage, reconstructed UI, inserted freeze frames, transitions, or speed changes. The action inputs were automated through the running app's keyboard and accessibility controls; the video is not a physical trackpad demonstration or a performance benchmark. The video contains no audio.

The app executable matches the packaged 0.8.0 executable. The local demo bundle has a separate identifier and an isolated sample library so recording does not change the reader's personal library. Language, layout, search, and all reading operations use the release app's normal implementation.

The recording source, trim bounds, hashes, and decoding results are recorded in `workflow-verification.json`. Reproducible capture and processing instructions are in [tools/promo](../../tools/promo/README.md).

## Images

- `bookshelf.png`: the original books and open reading tabs.
- `cross-book-search.png`: `feedback` found in three different PDFs, with book titles and page numbers.
- `reading.png`: two complete PDF pages fitted to the window with two reading tabs.
- `screenshots/en-03-result-page.png`: the selected result opens Search Design page 4 with the matching word selected.
- `screenshots/en-04-page-picker.png`: the native thumbnail picker.

These are unmodified app-window screenshots at 1720 × 960. PDF contents and labels are rendered by the app, not redrawn for promotion.

## Earlier media

The Japanese screenshots named `screenshots/01-*` through `05-*` were captured from 0.7.0. Its historical release includes a screenshot slideshow, not an actual-operation recording. The old `render-promo.swift` and `storyboard.json` reproduce that older material; current promotion uses the real recording above.

Original demo prose, screenshots, capture tools, and video are offered under the repository's Apache License 2.0. Font files are not distributed. The app's third-party notices remain applicable to the software shown.
