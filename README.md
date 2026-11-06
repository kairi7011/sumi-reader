# Sumi

Find a passage across your PDFs, then jump straight to its page.

[日本語](README.ja.md) · [Download the macOS preview](https://github.com/kairi7011/sumi-reader/releases/tag/v0.7.0)

Command K searches the text of imported PDFs in one place. Results include the book title and page number, so you can open the passage without first remembering which book contains it. Search requires selectable text and respects PDF copy permissions; it is keyword search, not regular-expression grep.

![Search across three original sample PDFs](docs/media/cross-book-search.png)

[Watch the short walkthrough](https://github.com/kairi7011/sumi-reader/releases/download/v0.7.0/Sumi-0.7.0-demo.mp4). Screenshots and video use original sample documents, not purchased books.

Sumi is written in Rust with native AppKit controls and Apple's PDFKit renderer. It has no browser engine, account, telemetry, cloud upload, or built-in AI service. The interface is currently Japanese.

- A cover bookshelf and tabs for the books you are reading.
- Single pages or facing spreads, page turns, and a scrollable thumbnail picker.
- In-place zoom, text selection, and rectangular image capture.
- One search palette for books, pages, document text, and app actions.
- An optional local index that exposes selected book excerpts to your own AI client through MCP.

## Requirements

Apple Silicon Mac, macOS 13 or later. Intel Macs, Windows, and Linux are not supported. The packaged reader needs no Node.js, Rust, or Python installation. The optional book-index tool has its own [requirements](docs/local-library.md).

## Getting started

Open Sumi, then drop a PDF or folder onto the window, or press **Command O**. Files stay in their original locations. Use **Command K** for search and actions, **Command L** for the bookshelf, and the page counter to jump through thumbnails.

See the [reading guide](docs/usage.md) for navigation, tabs, search, capture, and keyboard shortcuts.

## Build from source

Install Apple's Command Line Tools, Node.js 22.13 or later, and Rust 1.99.1 for `aarch64-apple-darwin`. Python 3.9 or later is required for tests. Native builds do not need npm dependencies.

```sh
git clone https://github.com/kairi7011/sumi-reader.git
cd sumi-reader
# Populate the pinned Cargo cache once; this step downloads dependencies.
cargo +1.99.1 fetch --locked --manifest-path src-native/Cargo.toml --target aarch64-apple-darwin
npm run check
npm test
npm run package:mac
```

Builds use `--locked --offline`. The build scripts support an installed pinned rustup toolchain, matching tools on PATH, or explicit `SUMI_RUST_BIN` / `CARGO_HOME` paths. They do not install compilers or search private workspace folders. See the [dependency review](docs/dependency-review-native.md) before changing dependencies.

The result is `dist/Sumi.app`. This project currently produces **ad-hoc signed, unnotarized preview builds**. They are not Developer ID releases; a downloaded build may be blocked by Gatekeeper. Do not disable macOS security checks globally. Release preparation and signing requirements are documented in [distribution](docs/distribution.md).

## Data and privacy

Sumi stores book paths, reading positions, bookmarks, and preferences on your Mac. It does not copy PDFs into the app or upload them. The optional MCP index is opt-in; excerpts sent to an external AI client are subject to that client's data handling. See [privacy](PRIVACY.md) for storage locations and deletion.

OCR, annotation editing, continuous-scroll reading, cloud sync, automatic updates, and tab restoration after relaunch are not included. See [security](SECURITY.md) for the current scope and reporting guidance.

## Contributing and license

Small fixes and reproducible bug reports are welcome. See [contributing](CONTRIBUTING.md). Please use synthetic PDFs in reports and tests; do not upload purchased books or private documents.

Copyright 2026 kairi7011. Sumi's source, original assets, and original fixtures are licensed under [Apache License 2.0](LICENSE). Third-party code and icons retain their own licenses; see [third-party notices](THIRD_PARTY_NOTICES.md).

The earlier Tauri implementation remains under `src/`, `src-tauri/`, and the `legacy:*` scripts as an unsupported prototype. It is not included in the native app. Historical verification reports describe their stated versions, not the current release.
