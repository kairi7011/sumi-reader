# Privacy

The native Sumi reader has no account, analytics, telemetry, upload service, crash-reporting endpoint, or automatic updater. Importing a document records its location and leaves the original file unchanged.

## Local storage

- `~/Library/Application Support/com.kairi7011.sumi/library.json` contains document paths, titles, reading positions, bookmarks, app preferences, and the saved tab session. The session records tab order, the selected book or Home, and each tab's zoom and in-book search query; page positions remain in the book records. History at `~/Library/Application Support/Sumi/library.json` may be migrated on first launch. Native library saves use an atomic replacement and owner-only file permissions.
- The optional index defaults to `~/Library/Application Support/Sumi/library-index`. Its SQLite database contains extracted book text, filenames, titles, and outlines. It uses an owner-only directory and database.
- Passwords for encrypted PDFs and the last captured image are held only while Sumi is running. Passwords are never written to the library or saved tab session and must be entered again after relaunch.
- Copy actions write to the macOS clipboard. Other apps and clipboard managers may read or retain that content. Saving a captured image writes to the destination you choose.

Removing a book from the shelf removes its library record, position, bookmarks, and any saved tab for that book. Closing a reading tab removes its saved zoom and in-book search query but keeps the book, position, and bookmarks in the library. Neither action removes the PDF or an independently created text index. To remove all Sumi data, quit Sumi and remove the current and legacy library data and any index directories you created. Saved images and clipboard history are managed separately.

## Optional AI clients

Indexing and the stdio MCP server are local and opt-in. Sumi does not configure Codex, Claude, or another client automatically. The server can return excerpts from indexed books to its connected client. A cloud-backed client can send those excerpts to its provider; use a local client and model when the text must remain on your Mac.

The server exposes search and bounded reads from its index; it has no arbitrary-file-read, shell, upload, or indexing tool. Book passages are reference material, not instructions to the client.

## Document links

Clicking a permitted web or email link in a PDF can open another application. That application handles the destination according to its own privacy settings. Sumi does not fetch linked pages itself.
