# Sumi 0.5 bookshelf validation

Draft status, 2026-11-05. Implementation is being integrated. This document records the checks required for the bookshelf and input redesign; it does not claim a completed GUI, hardware-gesture or release-package pass. The [0.4 native report](native-verification.md) remains the earlier implementation's evidence.

## Confirmed metadata check

The native and npm application versions are 0.5.0. Only the local package version entries changed. Exact comparisons confirm the same 31 external Rust packages and unchanged npm dependency records, with no resolution or downloads. The [native dependency audit](native-dependency-audit.json) records the previous and updated manifest/lock hashes and retains the original dependency evidence.

## Checks awaiting execution

| Area | Required observation | Status |
| --- | --- | --- |
| Launch and empty shelf | Launch stays on the full-window cover shelf; an empty shelf has a visible import action and drop target. | Pending |
| Cover shelf | Titles and reading positions remain recognizable; missing, locked and coverless files keep usable cards; filtering and scrolling do not load every PDF at once. | Pending |
| Shelf input | Four-direction selection, Enter, Tab and Command F work; Japanese composition is not consumed as navigation; closing Command K restores shelf focus. | Pending |
| Reading controls | Only the two labeled top actions and page-counter footer remain; zoom reset appears only while enlarged; palette actions show explicit target states. | Pending |
| Reader/shelf transition | Command L saves the current position and restores the shelf; hidden-reader shortcuts cannot turn a book; reopening preserves page and global preferences. | Pending |
| Import and open intent | Bulk import stays on the shelf, duplicates are reported, and explicit Finder-open selects the requested book; invalid/password-cancel cases preserve the current screen. | Pending |
| Book maintenance | Relinking keeps identity/bookmarks; removing the last-read book from the shelf removes only its record and leaves its PDF. | Pending |
| Keyboard and search | Reader Command F, Command K, page entry and native editing shortcuts work in their proper context; empty/nonempty search dismissal remains predictable. | Pending |
| Pointer and trackpad | Edge/margin clicks, corner rollback and horizontal two-finger turns work; vertical scrolling and momentum do not turn unintended pages; zoomed scrolling pans. | Pending — physical trackpad verification required |
| Focus and macOS fullscreen | Command Shift F hides reader chrome without adding a custom fullscreen transition; Escape restores controls. Standard macOS fullscreen remains usable. | Pending |
| Packaging | Updated tests/build, signed ARM64 bundle, ZIP integrity, artifact hash and final delivery path are recorded for 0.5.0. | Pending |

Synthetic gesture tests, if added, must be reported separately from physical trackpad checks. Existing 0.4 animation, search and memory measurements are historical samples and must not be relabeled as 0.5 measurements without rerunning them.
