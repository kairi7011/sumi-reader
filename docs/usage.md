# Using Sumi

## Your bookshelf

On first launch, Sumi shows its pinned Home tab and bookshelf. On later launches, it restores the saved tab order and selected book or Home. Each book tab retains its page, zoom, and in-book search query. If you close every book tab before quitting, the next launch keeps only Home. When upgrading from a version without saved tabs, Sumi opens the last-read book as one tab.

Click a cover to read, or move the selection with the arrow keys and press Enter. Open books appear in the tab strip alongside Home. Select a book tab to switch reading context; select Home or press Command L to return to the shelf. Escape dismisses an open panel while keeping Home selected.

The compact import icon adds books; the search icon opens the combined book-and-action palette. The shelf uses this palette for search, through the icon, Command K or Command F. The empty shelf also accepts dropped PDFs or folders.

Use Command O to select multiple PDFs or folders, or drop them onto the window. Folder import includes PDFs in nested folders and reports files it cannot read. Import records their locations; it does not move, copy, or modify the originals. Repeated paths and aliases are deduplicated. Separate copies at different paths remain separate library entries. Bulk import leaves the current view open rather than opening a book automatically. Opening a PDF explicitly from Finder opens that book, including one already imported.

The library uses book titles from PDF metadata when available and keeps filenames unchanged. Each book retains its reading position and bookmarks. Appearance, single-page/spread layout, and reading direction apply across the app.

Right-click a book in the library to reveal it in Finder, choose its new location after moving it, or remove it from the library. Relinking preserves its reading position and bookmarks. Removal deletes the library record, position, and bookmarks; the PDF remains on disk. Return to Home with Command L before removing the book currently being read.

Opening a book, searching, importing, and loading covers or page thumbnails each show a loading indicator if the work is still pending after 400 ms. Faster operations finish without showing the indicator. It disappears when the work finishes or is cancelled. Escape cancels a pending book opening.

## Reading and navigation

The top bar holds the pinned Home tab, open-book tabs and compact import/search icons. The bottom control is the page counter; a **Fit page** button appears beside it only while enlarged. Display settings, the outline, bookmarks and copy actions live in the Command K palette.

- Read one page or a facing spread, with left-to-right or right-to-left page turns. The normal view fits the complete page or spread.
- Turn pages with the arrow keys, Space, a click at the page edge or outer margin, or a horizontal two-finger trackpad gesture. The corner can also follow a drag and return to its starting position. Reduced Motion skips the animation.
- Press G or Command G, or click the page counter, to open the page picker. Scroll vertically through thumbnails for every page, enter a page number for a direct jump, or select a thumbnail. The picker renders the visible area and nearby previews with a bounded cache, so long books do not require rendering every thumbnail at once.
- Zoom in place with pinch or Command + / −. Command 0 or the contextual reset button restores the complete page. Hold Space and drag to move an enlarged page; scrolling pans while zoomed.
- Command L, Command T or Command 1 selects the pinned Home tab. Command W closes the current book tab; Control Tab cycles through Home and open books, and Control Shift Tab cycles backward. Command Shift F hides the reader chrome for focused reading; Escape restores it after dismissing any active panel or zoom. macOS standard full-screen behavior is separate.
- Command K offers explicit actions such as **Show Two Pages**, **Show One Page**, and **Read right to left**. Appearance and reading direction are app-wide. Light, sepia and dark change the window and reading background; the PDF's paper, text and illustration colors remain unchanged.

## Language

The Sumi menu and Command K offer System language, English, and 日本語. The choice applies to every book and persists after quitting. System language chooses Japanese for a Japanese macOS preference, and English otherwise. Changing language updates controls, command names, hints, and messages without closing the current book. Book titles and PDF content retain their original language. Command search accepts both Japanese and English action names.

## Search

On Home, Command F opens the combined search palette. While reading, Command F toggles search within the current book. Enter opens the selected result; pressing it again advances to the next result, and Shift Enter goes back. Up and down select a result. Escape or Command F closes search. Clicking outside also closes an empty search field; a nonempty search stays open while you inspect the page. The previous query is retained when reopening search.

Command K, or the search icon, searches book titles, imported-book text and app commands. While reading, it also searches page numbers and the current book's outline. Enter a page number for a direct jump, or an action such as `zoom` or `save image` to find the corresponding command. Text search runs in a cancellable background worker and returns bounded page snippets. Unicode width, case, and common line wrapping are normalized. Results are grouped by matching page, with at most 80 text results per request.

If the optional local book index contains the same file, search can reuse it after checking the file fingerprint. Otherwise it extracts text directly with PDFKit. Importing a book does not create an AI index or configure an AI client.

## Copying and capture

Select text and use Command C for native copying. Command K also offers copying the selection or the current page with the book title and PDF page number attached.

Press Command Shift X, then drag a rectangle to copy the visible region as an image. To save the last captured region as PNG, choose `Save captured image…` in Command K. The last capture is held only for the current app session; save it before quitting. Scanned pages can be captured as images, but OCR is not included.

Password-protected PDFs request a password for the current app session. Passwords are never persisted, including in saved tabs; opening a protected book after relaunch asks for the password again. Text copying follows the PDF's copy permissions, and extraction follows its text order; complex multicolumn layouts can need cleanup after copying.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Import PDFs or folders | Command O |
| Combined search on Home / current-book find while reading | Command F |
| Select the pinned Home tab | Command L / T / 1 |
| Close the current book tab | Command W |
| Next / previous tab, including Home | Control Tab / Control Shift Tab |
| Search books, text, pages, and commands | Command K |
| Open selected search result / next result | Enter |
| Previous current-book search result | Shift Enter |
| Previous / next page | Left / right arrow; Shift Space / Space |
| Visual page navigator | G or Command G |
| Bookmark current page | Command D |
| Zoom in place | Command + / − or pinch |
| Fit the complete page or spread | Command 0 |
| Move an enlarged page | Space + drag |
| Hide reader chrome for focused reading | Command Shift F |
| Capture a rectangle | Command Shift X |
| Copy selected text | Command C |
| Close panel / cancel capture or turn / leave zoom or focus | Escape |

