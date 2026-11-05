# Sumi

A personal PDF reader for macOS, built for long reading sessions and taking excerpts into your own AI conversations.

Sumi 0.4 is a native Rust application using AppKit controls and the system PDFKit renderer. The packaged reader contains no WebView, browser engine, JavaScript runtime, or PDF.js. This is a private project owned by **kairi7011**. The reader has no account, telemetry, cloud upload, or AI API integration.

## Development

The native build targets Apple Silicon and macOS 13 or later. It requires Apple's Command Line Tools, Node.js 22.13 or later, the reviewed Rust 1.99.1 toolchain, and the reviewed Cargo cache. Python 3.9 or later is needed for the local-library tests and optional MCP tools.

From your checkout, for example `~/programs/sumi-reader`:

```sh
cd ~/programs/sumi-reader
npm start
npm run check
npm test
npm run package:mac
```

`npm start` builds and launches the native debug app. `npm run check` checks the build scripts and Rust sources; `npm test` runs the Rust and local-library tests. `npm run test:native` runs only the Rust tests. The native scripts use Node's built-in modules, so they do not require `npm ci`.

The build uses `--locked --offline` and downloads no code. Set `SUMI_RUST_BIN` to the directory containing the verified `rustc` and `cargo`, and `CARGO_HOME` to its reviewed cache if they are not already discoverable locally. Read the [native dependency review](docs/dependency-review-native.md) before adding dependencies or changing the lockfile.

## Your books

Use Command O to select multiple PDFs or folders, or drop them onto the window. Folder import includes PDFs in nested folders and reports files it cannot read. Import records their locations; it does not move, copy, or modify the originals. Repeated paths and aliases are deduplicated. Separate copies at different paths remain separate library entries.

The library uses book titles from PDF metadata when available and keeps filenames unchanged. Each book retains its reading position and bookmarks. Appearance, single-page/spread layout, and reading direction apply across the app.

Right-click a book in the library to reveal it in Finder, choose its new location after moving it, or remove it from the library. Relinking preserves its reading position and bookmarks. Removal deletes the library record, position, and bookmarks; the PDF remains on disk. Open another book before removing the one currently being read.

## Reading and navigation

- Read one page or a facing spread, with left-to-right or right-to-left page turns. The normal view fits the complete page or spread.
- Use the arrow keys, Space, or a corner drag to turn pages. The corner follows the drag and can return to its starting position. Reduced Motion skips the animation.
- Press G or Command G to open the native thumbnail grid. Enter a page number, choose a thumbnail, or browse the next or previous group. The grid caches up to 24 thumbnails and prepares one per UI tick.
- Zoom the page in place with pinch or Command + / −. Command 0 restores the complete page. Hold Space and drag to move an enlarged page.
- Open the embedded outline or bookmarks from the tools menu or Command K.
- Choose light, sepia, or dark app appearance. These settings change the window and reading background; the PDF's paper, text, and illustration colors remain unchanged.

## Search

Command F toggles search within the current book. Enter opens the selected result; pressing it again advances to the next result, and Shift Enter goes back. Up and down select a result. Escape or Command F closes search. Clicking outside also closes an empty search field; a nonempty search stays open while you inspect the page. The previous query is retained when reopening search.

Command K searches book titles, imported-book text, page numbers, the current book's outline, and app commands. Enter a page number for a direct jump, or an action such as `拡大` or `画像を保存` to find the corresponding command. Text search runs in a cancellable background worker and returns bounded page snippets. Unicode width, case, and common line wrapping are normalized. Results are grouped by matching page, with at most 80 text results per request.

If the optional local book index contains the same file, search can reuse it after checking the file fingerprint. Otherwise it extracts text directly with PDFKit. Importing a book does not create an AI index or configure an AI client.

## Copying and capture

Select text and use Command C for native copying. Command K also offers copying the selection or the current page with the book title and PDF page number attached.

Press Command Shift X, then drag a rectangle to copy the visible region as an image. To save the last captured region as PNG, choose `切り抜いた画像を保存…` in Command K or the tools menu. The last capture is held only for the current app session; save it before quitting. Scanned pages can be captured as images, but OCR is not included.

Password-protected PDFs request a password for the current session. Passwords are not persisted. Text copying follows the PDF's copy permissions, and extraction follows its text order; complex multicolumn layouts can need cleanup after copying.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Import PDFs or folders | Command O |
| Toggle current-book search | Command F |
| Search books, text, pages, and commands | Command K |
| Open selected search result / next result | Enter |
| Previous current-book search result | Shift Enter |
| Previous / next page | Left / right arrow; Shift Space / Space |
| Visual page navigator | G or Command G |
| Bookmark current page | Command D |
| Zoom in place | Command + / − or pinch |
| Fit the complete page or spread | Command 0 |
| Move an enlarged page | Space + drag |
| Focus mode | Command Shift F |
| Capture a rectangle | Command Shift X |
| Copy selected text | Command C |
| Close panel / cancel capture or turn / leave zoom or focus | Escape |

## Packaging and local data

`npm run package:mac` produces `dist/Sumi.app` with the native executable, app icon, and optional local-library tools. It is an Apple Silicon build, ad-hoc signed and not notarized. Node.js is needed for development scripts, not for running the packaged reader.

Library records and app preferences are stored in `~/Library/Application Support/com.kairi7011.sumi/library.json`. Existing history in `~/Library/Application Support/Sumi/library.json` is migrated when the current library does not yet exist. PDFs remain at their original locations and are not added to the app bundle or Git.

## Local books for AI agents

The optional [local book library](docs/local-library.md) indexes personally owned PDFs with macOS PDFKit and SQLite. Its read-only stdio MCP server lets an agent search Japanese keywords, inspect a table of contents, and retrieve a few cited pages. It needs no API key, embedding model, network service, or Python packages. Client configuration is explicit; Sumi does not alter Codex or Claude settings.

The index stays on the Mac. Excerpts returned to a cloud-based AI client can enter that client's model context. Use a fully local client and model if the extracted text must also remain entirely local.

## Verification and retained versions

See [native validation notes](docs/native-verification.md), the [native dependency review](docs/dependency-review-native.md), and the [sanitized native search measurements](docs/native-search.json). Personal PDFs, extracted book contents, and private screenshots are excluded from the repository.

The earlier Tauri/WebKit implementation remains for comparison under `src/`, `src-tauri/`, and the `legacy:*` npm scripts. Its PDF.js dependency and frontend preparation are used only by those legacy scripts; the default build and package commands use the native app.

OCR, annotation editing, continuous reading scroll, and cloud sync are not included.
