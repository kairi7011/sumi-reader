# Native macOS dependency review

Reviewed 2026-11-05 for Sumi 0.4.0 on `aarch64-apple-darwin`. This build uses Rust, AppKit and PDFKit. It has no WebView, JavaScript runtime, browser assets, Tauri, Electron or PDF.js dependency.

All **31 registry packages** in `src-native/Cargo.lock` are unchanged versions from the [previous audited graph](tauri-dependency-audit.json). Resolution was offline, seeded from the previous lockfile; it downloaded no external code. Each cached archive was independently rehashed and matched its registry checksum. The audit preserves the original publisher, canonical repository, publication time, license and checksum evidence. Every release satisfies the 72-hour minimum age. The previous complete OSV/RustSec query is retained as dated evidence; neither previously reported Linux-only package exists in this smaller graph.

The 0.5.0 update changes only the local application version in Cargo.toml/Cargo.lock and npm root metadata. Exact parsed comparisons confirm that all 31 external Rust package records and all npm dependency records are unchanged. No dependency resolution, download, or build was performed for this metadata update. The current native audit records the previous and updated manifest/lock hashes in `bindingUpdates`; the original artifact and advisory evidence retains its original dates.

Exact direct versions:

| Dependency | Version | Purpose |
| --- | --- | --- |
| objc2 | 0.6.5 | Objective-C runtime interoperability |
| objc2-foundation | 0.3.2 | Native strings, collections, URLs and system objects |
| objc2-app-kit | 0.3.2 | macOS application, controls, window and drawing |
| objc2-core-graphics | 0.3.2 | Bounded page snapshots and graphics |
| block2 | 0.6.2 | Native event callback interoperability |
| serde | 1.0.229 | Persisted library records |
| serde_json | 1.0.154 | Compatibility with existing library JSON |
| sha2 | 0.11.1 | Stable identifiers for canonical file paths |

AppKit features are explicitly selected. PDFKit bindings are first-party Rust declarations against the macOS system framework; there is no downloaded PDFKit wrapper or renderer. AppKit, PDFKit, Foundation and CoreGraphics are supplied by macOS and are not copied into the application.

The eight external build-script entry points are identical to the already inspected and approved scripts: `libc`, `objc2`, `proc-macro2`, `quote`, `serde`, `serde_core`, `serde_json`, and `zmij`. Their source hashes are rechecked against [the prior script review](tauri-build-script-review.json) before being recorded in [native-dependency-audit.json](native-dependency-audit.json). Their approved behavior is compiler probing and generated configuration/data inside Cargo build output. No package installer or downloader is introduced.

The reviewed toolchain remains Rust 1.99.1 from the official 2026-10-15 distribution; component hashes and installer review are in [the toolchain review](dependency-review-tauri.md#toolchain). The repository move did not reinstall or change the toolchain. `scripts/build-native.mjs` accepts `SUMI_RUST_BIN` and `CARGO_HOME`, discovers the existing local verified workspace installation, or uses an explicitly installed matching compiler. It rejects compiler wrapper overrides and invokes Cargo with `--locked --offline --target aarch64-apple-darwin`. A build is not an operating-system network sandbox for build scripts.

Recheck existing artifact evidence without network access:

```sh
CARGO_HOME=/path/to/reviewed/cargo node scripts/verify-native-dependencies.mjs
node scripts/build-native.mjs --test --lib
node scripts/build-native.mjs
node scripts/package-native.mjs --out /path/to/output/Sumi.app
```

The verification command fails for a new package, version, registry, checksum or changed build script. It refreshes the current manifest/lockfile hash binding while retaining the date of the reused advisory evidence. A dependency change still requires a new age, identity, integrity, advisory and build-script review before compilation.
