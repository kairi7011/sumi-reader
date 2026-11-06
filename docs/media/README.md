# Sumi promotional media

These images show the native Sumi 0.7.0 app with three original Japanese sample PDFs. No purchased book, personal bookshelf, or private reading history appears in them. The sample generator and original prose are in `tools/promo/create-demo-books.swift`.

## Files

- `screenshots/01-bookshelf.png`: the three sample books on the shelf.
- `screenshots/02-palette.png`: Command K opens search and actions.
- `screenshots/03-search-results.png`: the same phrase found in three different PDFs, with book titles and page numbers.
- `screenshots/04-selected-result.png`: the second book's result selected.
- `screenshots/05-reading-page.png`: that result opens page 4, with the matched phrase selected in the real PDF renderer.
- `bookshelf.png`, `cross-book-search.png`, and `reading.png`: 1920 × 1080 promotional cards composed from those screenshots.

The search phrase is `操作の結果を伝える`. It occurs once in each sample PDF. This is literal full-text keyword search, not regular-expression search.

The screenshots were captured from the running app in a separate demo library. The app executable is the released 0.7.0 executable; only the local demo bundle's identifier/display name was changed so it could run alongside the normal reader. No UI pixels were reconstructed. Cards contain scaling/cropping, captions, and a background around the original screenshots.

The 24-second video presents these real screenshots with short captions and crossfades. Scene durations are chosen for explanation, not a speed measurement. The downloadable H.264 MP4 and a media ZIP are attached to the [0.7.0 release](https://github.com/kairi7011/sumi-reader/releases/tag/v0.7.0).

## Rebuild the cards and video

On macOS with Apple's Command Line Tools:

```sh
mkdir -p test-artifacts/promo-render
swift tools/promo/render-promo.swift tools/promo/storyboard.json \
  --output test-artifacts/promo-render/demo.mp4 \
  --cards test-artifacts/promo-render/cards
```

The renderer uses system AppKit and AVFoundation, without extra packages or network requests. `--cards-only` generates just the PNG cards. The source screenshots and `tools/promo/storyboard.json` are checked in; generated demo PDFs and video intermediates remain ignored.

Original demo prose, screenshots, cards, renderer, and video are offered under the repository's Apache License 2.0. macOS-rendered fonts are used in the images; font files are not distributed. The app's third-party notices remain applicable to the software shown.
