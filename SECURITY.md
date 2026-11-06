# Security

## Supported scope

Only the current native macOS release is maintained. The retained Tauri prototype and older binaries are unsupported. Sumi uses the system PDFKit parser; keeping macOS updated is part of its security boundary. It is not a separate sandbox for hostile PDFs and has not received an independent penetration test.

The native reader does not run a network server or a shell to process documents. It restricts document links to web and email destinations, respects PDF text-copy permissions, and does not persist document passwords. The optional MCP service uses stdio, a read-only SQLite connection, bounded requests and responses, and an explicit local index.

## Reporting

Do not put private PDFs, credentials, personal paths, or an exploit against an unpatched release in a public issue. Use GitHub's private vulnerability reporting when the repository offers **Security → Report a vulnerability**. If that option is unavailable, open a minimal issue requesting a private reporting channel without including the vulnerability details.

Useful private reports include the Sumi version, macOS version, affected behavior, reproduction steps, and a minimal synthetic PDF when possible. There is no guaranteed response time or paid support agreement.

## Release verification

Releases must be scanned for private data and include project and dependency license notices. SHA-256 checksums detect a changed download but do not establish the publisher's identity. Current preview bundles are ad-hoc signed and not notarized. A Developer ID certificate and Apple's notarization process are required before claiming a notarized distribution.
