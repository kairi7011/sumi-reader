# Privacy

The native Sumi reader has no account, analytics, telemetry, upload service, crash-reporting endpoint, or automatic updater. Importing a document records its location and leaves the original file unchanged.

## Local storage

- `~/Library/Application Support/com.kairi7011.sumi/library.json` contains document paths, titles, reading positions, bookmarks, and app preferences. History at `~/Library/Application Support/Sumi/library.json` may be migrated on first launch. Native library saves use an atomic replacement and owner-only file permissions.
- The optional index defaults to `~/Library/Application Support/Sumi/library-index`. Its SQLite database contains extracted book text, filenames, titles, and outlines. It uses an owner-only directory and database.
- Passwords for encrypted PDFs and the last captured image are held for the app session. Passwords are not written to the library.
- Copy actions write to the macOS clipboard. Other apps and clipboard managers may read or retain that content. Saving a captured image writes to the destination you choose.

Removing a book from the shelf removes its library record, position, and bookmarks. It does not remove the PDF or an independently created text index. To remove all Sumi data, quit Sumi and remove the current and legacy library data and any index directories you created. Saved images and clipboard history are managed separately.

## Optional AI clients

Indexing and the stdio MCP server are local and opt-in. Sumi does not configure Codex, Claude, or another client automatically. The server can return excerpts from indexed books to its connected client. A cloud-backed client can send those excerpts to its provider; use a local client and model when the text must remain on your Mac.

The server exposes search and bounded reads from its index; it has no arbitrary-file-read, shell, upload, or indexing tool. Book passages are reference material, not instructions to the client.

## Document links

Clicking a permitted web or email link in a PDF can open another application. That application handles the destination according to its own privacy settings. Sumi does not fetch linked pages itself.
