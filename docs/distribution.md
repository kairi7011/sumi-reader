# Distribution

The native release targets Apple Silicon and macOS 13 or later. Current bundles are ad-hoc signed preview builds, not Developer ID signed or notarized releases. A successful local codesign check verifies the bundle's contents, not a trusted publisher identity.

## Prepare a preview

Use a clean, audited checkout with the pinned compiler and cached dependencies described in the README.

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

## Public Git history

Check every reachable branch and tag before publishing. Deleting a file in a later commit does not remove earlier copies. Commit authors should use the maintainer's GitHub noreply identity. Personal PDFs, titles from a private test library, extracted passages, credentials, and local paths must not be included.

An audited source snapshot and a sanitized Git history can be prepared independently of the private development repository. Preserve a private backup before any history replacement. Do not push private backup refs, old tags, or an unsanitized mirror to a public repository. Historical validation reports may retain original commit identifiers from before a privacy rewrite; those identify the private test record, not a new verification run.

GitHub can retain cached views and references to earlier commits even after a force-push. When an existing remote contains private history, a fresh public repository avoids copying those old objects. See GitHub's [sensitive-data removal guidance](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository).

## Developer ID distribution

For a notarized release, obtain an appropriate Developer ID Application identity, sign the final app with the hardened runtime and timestamp, submit it to Apple's notarization service, and staple and validate the ticket. Regenerate the ZIP and checksums after that process. Keep signing credentials and notarization profiles outside the repository.

This repository does not install certificates, submit notarization requests, or change Gatekeeper settings. Follow Apple's [notarization documentation](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) and check the result on a separate Mac before labeling a build notarized.

## Release description

State the supported architecture and macOS version, signing/notarization status, relevant changes, tests actually performed, and remaining limitations. Include source and binary checksums. Enable private vulnerability reporting when the public repository is created; do not request private books in issue templates.
