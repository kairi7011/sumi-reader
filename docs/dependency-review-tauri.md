# Tauri dependency review

Reviewed 2026-11-04 for **macOS Apple Silicon (`aarch64-apple-darwin`)**. The minimum publication age is 72 hours. This review permits the exact locked macOS build; it does not approve other platforms or future dependency updates.

## Toolchain

Rust 1.99.1, released 2026-10-15, was obtained from the Rust project's [official distribution metadata](https://static.rust-lang.org/dist/2026-10-15/channel-rust-stable.toml) and [release announcement](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/). Only `rustc`, `cargo`, and the macOS ARM64 standard library were downloaded. Rust is MIT/Apache-2.0 with bundled component notices. Release notes/security fixes were inspected; OSV returned no advisory for the corresponding Cargo crate version 0.100.0.

All URLs have prefix `https://static.rust-lang.org/dist/2026-10-15/`:

| Archive | Authoritative SHA-256 |
| --- | --- |
| `rustc-1.99.1-aarch64-apple-darwin.tar.xz` | `23948a0a9c162fd2952defb3fb0462c44e8fcfd1a4ab8a274a1e1bdb2cf810a3` |
| `cargo-1.99.1-aarch64-apple-darwin.tar.xz` | `685e563d94db1f003ffa5f5831439eec02569b9950b69867953c8213269c1eaf` |
| `rust-std-1.99.1-aarch64-apple-darwin.tar.xz` | `fe79d6ab5a0321de26463999d8c112b8ad3eafbcbcf822d66a2300b45dde1031` |

Each archive passed SHA-256 verification before extraction. The official installer was read before execution; the three copies differ only in their success labels. Component manifests contain relative paths with no parent traversal. Installation copies these components under the explicit workspace prefix, disables `ldconfig`, and does not access the network. No rustup installer was run and no home-directory toolchain was changed.

Workspace installation: `../toolchains/rust-1.99.1`; Cargo cache: `../cargo`, both relative to this repository. Verified executable versions are `rustc 1.99.1 (58cf8f9c3 2026-10-13)` and `cargo 1.99.1 (5f94df478 2026-08-27)`.

## Direct dependencies and purpose

All versions are exact `=` requirements in `src-tauri/Cargo.toml`. Authoritative identities, publication timestamps, checksums, licenses, and trusted-publishing data are retained for **all 457 locked registry releases** in [tauri-dependency-audit.json](tauri-dependency-audit.json). Direct crates use MIT/Apache-2.0 dual licenses.

| Crate | Version | Published UTC | Publisher / canonical project | Purpose |
| --- | --- | --- | --- | --- |
| tauri | 2.12.5 | 2026-10-21 11:54 | tauri-bot / tauri-apps/tauri | Native window and local commands with system WebKit |
| tauri-build | 2.7.4 | 2026-10-21 11:53 | tauri-bot / tauri-apps/tauri | Build-time assets, configuration and permissions |
| tauri-plugin-dialog | 2.8.4 | 2026-10-28 00:03 | tauri-bot / tauri-apps/plugins-workspace | Native file picker |
| tauri-plugin-clipboard-manager | 2.5.0 | 2026-10-14 19:15 | tauri-bot / tauri-apps/plugins-workspace | Native text and image clipboard |
| serde | 1.0.229 | 2026-07-18 23:05 | dtolnay / serde-rs/serde | Typed persisted state and commands |
| serde_json | 1.0.154 | 2026-10-11 15:31 | dtolnay / serde-rs/json | Existing JSON state compatibility |
| sha2 | 0.11.1 | 2026-10-12 18:58 | GitHub trusted publishing / RustCrypto/hashes | Stable document identifiers |
| base64 | 0.23.1 | 2026-08-04 12:34 | marshallpierce / marshallpierce/rust-base64 | Image data URL decoding |

The sha2 release has no individual `published_by` entry because it used trusted publishing. Registry evidence identifies `RustCrypto/hashes`, workflow run `38294476233`, commit `138dc5a05b93f8e49cda177fc6cfd7cc43673e5d`. No unexplained identity/repository change was found in the direct metadata. `pdfjs-dist@6.4.299` is retained unchanged from [the previous npm review](dependency-review.md); the Tauri migration adds no npm package and needs no Tauri CLI.

## Resolution and integrity

`cargo generate-lockfile` used the sparse crates.io index, with no archive downloads or script execution. Every entry was checked against the crates.io API before `cargo fetch`. Eight initially selected releases were younger than 72 hours; metadata-only `cargo update --precise` selected these eligible versions:

| Crate | Rejected version | Locked version |
| --- | --- | --- |
| cc | 1.11.0 | 1.10.0 |
| crc32fast | 1.5.4 | 1.5.3 |
| jiff | 0.2.39 | 0.2.38 |
| jiff-static | 0.2.39 | 0.2.38 |
| jiff-core | 0.1.2 | 0.1.1 |
| toml | 1.2.0+spec-1.1.0 | 1.1.8+spec-1.1.0 |
| toml_edit | 0.25.18+spec-1.1.0 | 0.25.17+spec-1.1.0 |
| toml_writer | 1.2.0+spec-1.1.0 | 1.1.3+spec-1.1.0 |

After the complete graph passed age, yanked-state, registry, and checksum checks, `cargo fetch --locked --target aarch64-apple-darwin` downloaded 267 archives. Their SHA-256 hashes were independently recomputed and all matched Cargo.lock. No Git dependency or unofficial registry is present. Cargo metadata for this target contains those 267 external packages plus Sumi.

Reviewed Cargo.lock SHA-256: `d4b1b30dd89a345d314ba6c1bebf85d237a53a7831db5f16099d95f76ee64d17`.

## Advisories and target scope

The complete lockfile was queried through OSV, including its RustSec records. Two Linux/BSD-only dependencies have advisories:

- `glib@0.18.5`: [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), also [GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g), concerns unsound `VariantStrIter` behavior.
- `proc-macro-error@1.0.4`: [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html), an unmaintained-crate notice.

Every route into GTK/WebKitGTK is gated by Linux/BSD target conditions in the authoritative sparse index manifests. `proc-macro-error` is reachable only through `glib-macros` or `gtk3-macros`. Neither affected package appears in the filtered macOS Cargo metadata or inverse `cargo tree`; neither was fetched. These are explicit target exclusions, not a claim that the complete cross-platform lockfile is advisory-free. The same review does not authorize a Linux build.

`scripts/cargo-advisory-scope.json` binds these exact advisory IDs and versions to the reviewed target, Cargo.lock hash, and Cargo.toml hash. Changing any bound input invalidates the exclusions. Unknown advisories remain failures. Metadata for off-target `libappindicator`/`libappindicator-sys` lacks repository fields; both identify `tauri-bot` as publisher, and neither was downloaded for this build.

## Build-script approval

The 44 external custom-build entry points (plus Sumi's own build.rs) were inspected before the first build. Exact entry-point hashes are recorded in [tauri-build-script-review.json](tauri-build-script-review.json). Relevant invoked helper paths were also inspected, including `cc`, `autocfg`, `version_check`, Tauri ACL/config generation, and Tauri mobile setup.

- Compiler probes run the pinned rustc, emit configuration flags, or write/remove temporary probe output under `OUT_DIR`.
- Code generators read bundled data and emit Rust, permission, schema, or JavaScript files. Tauri plugin generation also updates generated permission files inside its extracted crate directory.
- `objc2-exception-helper` compiles bundled Objective-C source with installed Apple compiler tools through `cc`. Wry links the macOS system WebKit framework; no browser engine is downloaded or bundled.
- Tauri's iOS/Android setup branches are inactive on this target. `swift-rs` has a test-only Swift compilation hook; builds explicitly set `TEST_SWIFT_RS=false`. No network downloader or package-manager invocation is required by the reviewed macOS build path.

Build with `--locked --offline --target aarch64-apple-darwin` using the verified toolchain. The offline flag prevents Cargo from changing or fetching dependencies; it is not an operating-system network sandbox for build scripts. Compiler wrapper overrides require separate review.

## Rechecking

Run `node scripts/verify-cargo.mjs` before fetching a changed graph. It performs metadata-only exact-release checks and OSV queries, writes the report outside the repository by default, and fails on changed/unreviewed advisory scope. It never invokes Cargo or runs package code. Arguments are `[lockfile] [report] [metadata-cache] [target]`. The default target is this project's Apple Silicon macOS target; another target will not inherit these exclusions.

Any dependency update requires a fresh graph age/integrity/advisory review and review of changed build scripts before execution. A passed gate describes the available evidence, not a guarantee of harmlessness.
