# Distribution

The native release targets Apple Silicon and macOS 13 or later. Current bundles are ad-hoc signed preview builds, not Developer ID signed or notarized releases. A successful local codesign check verifies the bundle's contents, not a trusted publisher identity.

## Prepare a preview

First follow [private-to-public synchronization](maintainer-sync.md): changes must be committed and pushed privately before the public mirror is updated. Use a clean, audited public checkout with the pinned compiler and cached dependencies described in the README.

```sh
npm run check
npm test
npm run check:public
npm run package:mac
node scripts/verify-native-package.mjs dist/Sumi.app
python3 scripts/check-public.py --history --bundle dist/Sumi.app
mkdir -p release-artifacts
ditto -c -k --sequesterRsrc --keepParent dist/Sumi.app release-artifacts/Sumi-mac-arm64.zip
unzip -tq release-artifacts/Sumi-mac-arm64.zip
```

Create a source archive with `git archive`, so ignored local libraries, books, caches, configuration, and signing material cannot enter it. Inspect the final ZIP members and compute SHA-256 after all packaging and signing steps. Upload only the reviewed archives, checksums, and release notes. Do not upload an entire workspace or output directory: it can also contain user-specific MCP configurations.

The package script fails if dependency licenses do not match the native lockfile, a linked library is outside the system frameworks, a browser engine is linked, or private build paths remain. Project LICENSE/NOTICE, Rust dependency notices, Rust standard-library notices, and icon licenses are included. The public-data checker additionally scans tracked files and reachable history. New images and document fixtures still need a human review.

`scripts/generate-native-notices.mjs` rebuilds dependency notices from the reviewed Cargo cache and records their hashes. Run it after a reviewed dependency change; do not treat a matching hash or an old advisory report as a new security review.

## Local app updates

When delivering an update on the maintainer's Mac, update `/Applications/Sumi.app` as well as the release artifacts. Preserve the bundle identifier `com.kairi7011.sumi`. The installed reader must work independently of the checkout.

The library, per-book reading positions and bookmarks, and shared appearance, layout, direction, and language preferences live in `~/Library/Application Support/com.kairi7011.sumi/library.json`. The existing local search index lives under `~/Library/Application Support/Sumi/library-index/`. Keep both directories and the source PDFs in place. An app update replaces the app bundle, not these files. Never launch the installed app with demo/test library overrides or copy sample data over the normal library. Keep the current schema version until a migration that preserves existing state has been implemented and verified; do not rely on corrupt-data recovery for an unsupported version.

1. Finish the build and package checks in a staging location. Never pass `/Applications/Sumi.app` as the packaging output: the packaging script replaces its output directly. Let imports and other pending operations finish, then quit the running Sumi normally so its latest reading state is saved. Resolve any save error before proceeding; do not overwrite a running bundle.
2. Keep a recoverable copy of the previous app and user data outside Git and release artifacts. Use owner-only access for user-data backups. Do not print or publish their contents.
3. Stage and verify the new bundle, then replace `/Applications/Sumi.app`. Do not re-sign it merely to install it, clear system permissions, or modify the user-data directories.
4. Launch that exact installed path using the normal library. Verify the version, bookshelf, a saved reading position, and shared settings. Restore the books that were open before the update. A renewed macOS file-access prompt must be resolved before claiming startup verification is complete.
5. If verification fails, keep the backups and investigate. Do not reset the library or automatically restore an older data snapshot over newer reading activity. Roll back the app only when its data compatibility has been checked.

## Public Git history

Check every reachable branch and tag before publishing. Deleting a file in a later commit does not remove earlier copies. Commit authors should use the maintainer's GitHub noreply identity. Personal PDFs, titles from a private test library, extracted passages, credentials, and local paths must not be included.

An audited source snapshot and a sanitized Git history can be prepared independently of the private development repository. Preserve a private backup before any history replacement. Do not push private backup refs, old tags, or an unsanitized mirror to a public repository. Historical validation reports may retain original commit identifiers from before a privacy rewrite; those identify the private test record, not a new verification run.

GitHub can retain cached views and references to earlier commits even after a force-push. When an existing remote contains private history, a fresh public repository avoids copying those old objects. See GitHub's [sensitive-data removal guidance](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository).

## Developer ID distribution

For a notarized release, obtain an appropriate Developer ID Application identity, sign the final app with the hardened runtime and timestamp, submit it to Apple's notarization service, and staple and validate the ticket. Regenerate the ZIP and checksums after that process. Keep signing credentials and notarization profiles outside the repository.

This repository does not install certificates, submit notarization requests, or change Gatekeeper settings. Follow Apple's [notarization documentation](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) and check the result on a separate Mac before labeling a build notarized.

## Release description

State the supported architecture and macOS version, signing/notarization status, relevant changes, tests actually performed, and remaining limitations. Include source and binary checksums. Enable private vulnerability reporting when the public repository is created; do not request private books in issue templates.
