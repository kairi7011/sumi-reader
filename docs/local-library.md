# Local technical-book library

Sumi includes a local search tool for personally owned PDF books. macOS PDFKit extracts text and the embedded table of contents. Python's standard library stores a page-level SQLite FTS5 index. The tool requires no Python packages, API keys, embeddings, or network services.

The viewer and this index are independent. Indexing does not alter the PDFs. The default directory is `~/Library/Application Support/Sumi/library-index`; it contains the private SQLite database and the compiled PDFKit helper. Neither belongs in Git. The directory is owner-only, and the database is created with mode `0600`.

## Requirements

- macOS with Apple's Command Line Tools (`swiftc` and PDFKit).
- Python 3.9 or later, built with SQLite 3.34 or later and FTS5 trigram support.

No package installation occurs. The first index operation compiles the checked-in `extract.swift` with system frameworks. Subsequent runs reuse the helper identified by its source hash.

## Index and query

Run these commands from the repository root:

```sh
python3 tools/library/sumi_library.py index --source "$HOME/Downloads"
python3 tools/library/sumi_library.py list
python3 tools/library/sumi_library.py search 'フィードバック'
python3 tools/library/sumi_library.py search '認知' --limit 5
python3 tools/library/sumi_library.py read BOOK_ID --page 25 --count 2
python3 tools/library/sumi_library.py toc BOOK_ID
```

A directory source matches only `oreilly-*.pdf`, without recursion. Unrelated Downloads files are not read. To include a different book, pass its explicit path with `--source`; this option is repeatable. A quoted glob is also accepted. `--pattern '*.pdf'` deliberately widens directory matching. Files above 250 MB, locked PDFs, and PDFs that prohibit copying are rejected.

The index operation compares a SHA-256 fingerprint, skips unchanged books, and replaces changed books within a transaction. An incomplete extraction retains the previously indexed book. Missing source files remain in the index until the index directory is removed and rebuilt. `--force` re-extracts unchanged PDFs. PDF pages without selectable text remain empty; this version does not perform OCR.

All commands accept `--index-dir DIRECTORY` **before** the subcommand, for example:

```sh
python3 tools/library/sumi_library.py --index-dir /private/local/book-index index --source "$HOME/Downloads"
```

Search treats the query as an exact phrase, not SQL or FTS syntax. It normalizes Unicode width and case and repairs common line wrapping. Japanese queries of one or two characters use substring matching because the trigram index needs three characters. Search returns at most 30 page snippets. `read` returns at most five pages and 5,000 characters per page, with an explicit truncation flag. PDF page numbers are one-based; printed page labels can differ.

## Read-only MCP integration

The stdio server exposes four tools:

| Tool | Result |
| --- | --- |
| `list_books` | Book IDs, actual PDF titles, filenames, page counts |
| `search_books` | Exact-phrase matches with snippets and page citations |
| `read_book_pages` | Bounded text from a known book ID and page range |
| `book_table_of_contents` | Embedded outline headings and PDF page references |

The MCP process opens SQLite in read-only mode. It has no indexing, arbitrary-path read, shell, upload, or configuration tools. Book content is treated as source material rather than instructions to an agent. The server does not open a TCP port or make network requests.

Generate a client configuration with the exact absolute script path:

```sh
python3 - <<'PY'
import json
import sys
from pathlib import Path
script = str((Path.cwd() / 'tools/library/sumi_library.py').resolve())
print(json.dumps({'mcpServers': {'sumi-books': {
    'command': sys.executable,
    'args': [script, 'serve']
}}}, indent=2))
PY
```

Add the resulting server entry through your MCP client's configuration mechanism. The tool does not edit global client settings. For a custom index directory, use arguments `[script, "--index-dir", "/absolute/index/path", "serve"]`.

The server supports modern MCP `2026-07-28` discovery and per-request metadata, as well as the `initialize` handshake for `2025-11-25`, `2025-06-18`, `2025-03-26`, and `2024-11-05`. It uses UTF-8 newline-delimited JSON-RPC on stdin/stdout; diagnostics go to stderr. Closing stdin ends the process. Protocol details follow the official [stdio specification](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio), [discovery specification](https://modelcontextprotocol.io/specification/2026-07-28/server/discover), and [legacy lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle).

The index and server stay local. If the connected AI client uses a cloud model, the excerpts returned to that client may enter its model context under that client's own data policy. A fully local model/client is required if extracted text must remain entirely on the Mac.

## Verification

```sh
python3 -m unittest discover -s tests -p 'library_test.py' -v
```

Tests cover Japanese short and long queries, exact phrases, width normalization, page citations, reindex replacement, incomplete-extraction recovery, bounded reads, path rejection, read-only SQLite connections, and modern/legacy MCP messages. Real PDF extraction can be verified by indexing explicitly selected local books and comparing a returned passage with the cited PDF page.
