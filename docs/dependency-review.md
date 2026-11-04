> Historical Electron 0.1.0 verification. The retained PDF.js package is unchanged. For the current native graph, see [Tauri dependency review](dependency-review-tauri.md).

# Dependency review — 2026-11-04

Sumi uses Electron 45.2.1 for the macOS shell and Mozilla PDF.js 6.4.299 for rendering, text selection, and extraction. These are the only direct packages. Both are pinned exactly in package.json; package-lock.json pins and records registry integrity for the complete graph. No rendering framework or packaging dependency is required.

## Verification before installation

- Registry: https://registry.npmjs.org/ only. Direct names, Electron/Mozilla repositories, maintainer identities, licenses, deprecation status, signatures, and PDF.js provenance were inspected. No deprecations or unexpected artifact identities were found.
- Node 22.23.1 / npm 10.9.8. The 72-hour policy was enforced with a dynamically calculated cutoff during metadata-only resolution using npm install --before <current-time-minus-72-hours> --package-lock-only --ignore-scripts.
- The personal dependency-safety check-npm-release.mjs checker passed both direct versions and all 26 external lockfile entries at 2026-11-04T02:55:50.669Z, before npm ci. Every registry tarball URL and SHA-512 integrity matched authoritative metadata.
- npm audit --package-lock-only --audit-level=low and the installation audit both reported zero known vulnerabilities.
- npm audit signatures verified registry signatures for all 13 installed packages and provenance attestations for five.
- npm ci --ignore-scripts --omit=optional installed 13 packages. Optional @napi-rs/canvas binaries and undici were not installed. Their complete lock entries were nevertheless included in age/integrity verification.
- No lifecycle scripts were approved or executed. @electron/get 5.1.0 declares a development prepare hook (husky); it remains disabled. Undici's lifecycle metadata is also disabled and the optional package is omitted.

## Advisory review

[Electron advisories](https://github.com/electron/electron/security/advisories) and [Mozilla PDF.js advisories](https://github.com/mozilla/pdf.js/security/advisories) were checked. Electron's October 2026 eMudhra mitigation GHSA-jr5w-w6x2-84p9 was fixed in 45.0.0-beta.6; 45.2.1 includes the fix. PDF.js GHSA-hq66-cqwq-w95j is patched in 6.2.108 and GHSA-wgrm-67xf-hhpq in 4.2.67; 6.4.299 exceeds both patch versions. PDF scripting is not enabled by this application, eval is disabled, and renderer content is constrained by CSP. npm audit reported no known vulnerability affecting the resolved graph. These checks are not a guarantee against undisclosed defects.

## Electron runtime

The npm package no longer installs Electron in a postinstall hook; its index.js can lazily invoke install.js. install.js, index.js, cli.js, and @electron/get 5.1.0 downloader/URL/checksum/proxy sources were inspected before invoking Electron. The downloader supports mirror environment variables and a native extractor. To keep this installation explicit, the pinned official archive was fetched directly over HTTPS and extracted with the installed macOS ditto utility after SHA-256 verification. No npm installer, mirror, native extraction dependency, or runtime download hook was executed.

- Release: https://github.com/electron/electron/releases/tag/v45.2.1
- Published: 2026-10-31T00:44:03Z (older than 72 hours).
- Artifact: https://github.com/electron/electron/releases/download/v45.2.1/electron-v45.2.1-darwin-arm64.zip
- Expected SHA-256: a9a1a0a5b64df714957f83467c84823cb34792205922c6f136550b2403605922
- The downloaded archive matched this SHA-256 before extraction. The digest is present in both the authoritative GitHub release asset metadata and the integrity-verified npm package's checksums.json.
- The extracted executable reported v45.2.1 with --version.

Fresh Apple Silicon installations use `npm run setup:runtime`. This first-party script confirms the installed package pin and embedded checksum, checks authoritative npm and GitHub publication times against the 72-hour minimum, and requires the official asset URL and SHA-256 to match the reviewed constants. It streams the archive with a size limit, verifies the digest before extraction, and uses `/usr/bin/ditto` without invoking npm package code. A complete existing runtime skips the download. An incomplete existing distribution causes a failure instead of being overwritten.

`npm start` invokes the fixed local executable through `scripts/start.mjs`; it never imports the npm Electron loader and cannot trigger its automatic download. Missing runtime files produce setup instructions. Syntax checks and the existing-runtime setup path were verified on this Mac. Isolated fixtures also confirmed that a corrupt archive, an under-age release, and a missing runtime each fail before extraction or execution.

## Resolved release ages

| Exact package | Authoritative publication time (UTC) | Gate |
| --- | --- | --- |
| @electron-internal/extract-zip@1.0.8 | 2026-10-29T08:23:14.541Z | PASS |
| @electron/get@5.1.0 | 2026-07-27T05:10:56.194Z | PASS |
| @napi-rs/canvas@1.1.0 | 2026-10-30T06:20:17.120Z | PASS |
| @napi-rs/canvas-android-arm64@1.1.0 | 2026-10-30T06:22:49.233Z | PASS |
| @napi-rs/canvas-darwin-arm64@1.1.0 | 2026-10-30T06:22:38.840Z | PASS |
| @napi-rs/canvas-darwin-x64@1.1.0 | 2026-10-30T06:18:28.172Z | PASS |
| @napi-rs/canvas-linux-arm-gnueabihf@1.1.0 | 2026-10-30T06:20:11.198Z | PASS |
| @napi-rs/canvas-linux-arm64-gnu@1.1.0 | 2026-10-30T06:19:45.968Z | PASS |
| @napi-rs/canvas-linux-arm64-musl@1.1.0 | 2026-10-30T06:19:55.746Z | PASS |
| @napi-rs/canvas-linux-riscv64-gnu@1.1.0 | 2026-10-30T06:20:31.119Z | PASS |
| @napi-rs/canvas-linux-x64-gnu@1.1.0 | 2026-10-30T06:17:51.347Z | PASS |
| @napi-rs/canvas-linux-x64-musl@1.1.0 | 2026-10-30T06:22:14.766Z | PASS |
| @napi-rs/canvas-win32-arm64-msvc@1.1.0 | 2026-10-30T06:18:58.198Z | PASS |
| @napi-rs/canvas-win32-x64-msvc@1.1.0 | 2026-10-30T06:18:22.584Z | PASS |
| @types/node@24.19.2 | 2026-10-09T18:44:25.189Z | PASS |
| debug@4.4.3 | 2025-09-13T17:25:19.732Z | PASS |
| electron@45.2.1 | 2026-10-31T01:03:15.875Z | PASS |
| env-paths@3.0.0 | 2021-08-27T20:16:10.424Z | PASS |
| graceful-fs@4.2.11 | 2023-03-16T19:30:19.323Z | PASS |
| ms@2.1.3 | 2020-12-08T13:54:35.223Z | PASS |
| pdfjs-dist@6.4.299 | 2026-10-03T16:47:40.337Z | PASS |
| progress@2.0.3 | 2018-12-05T16:57:58.058Z | PASS |
| semver@7.8.5 | 2026-06-19T18:32:48.972Z | PASS |
| sumchecker@3.0.1 | 2019-11-25T22:00:49.327Z | PASS |
| undici@7.30.1 | 2026-10-15T14:18:31.283Z | PASS |
| undici-types@7.24.6 | 2026-03-25T16:01:36.357Z | PASS |

For future dependency updates, repeat the direct metadata check, cutoff resolution, full-lock check, audit, and source review before installing or executing newly resolved code. Do not replace the pins with latest or wildcard ranges.
