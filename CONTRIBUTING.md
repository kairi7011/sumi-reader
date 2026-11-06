# Contributing

Sumi focuses on reading and predictable native interactions. Prefer a small fix to adding another mode or control. Describe the user's action, the observed result, and the expected result in issues or pull requests.

## Development

Follow the build instructions in the README. `npm start` runs a debug build. `npm run check` checks the native code and build scripts; `npm test` runs Rust and Python tests. No npm install is needed for the native app. Dependencies are pinned and build offline after the Cargo cache is populated.

Run checks relevant to your change. For input or layout changes, also exercise the packaged app with an original test PDF and record which interactions you actually tried. Do not describe internal input simulations as physical trackpad testing.

## Fixtures and privacy

Use `tests/create-fixture.cjs` or your own original document. Do not commit books, extracted text, a personal bookshelf, screenshots of private documents, local configuration, signing identities, or credentials. Use GitHub's noreply address for commit authorship. The `.gitignore` excludes common local artifacts; it does not remove already committed data.

`npm run check:public` scans tracked files and reachable Git history. Release maintainers also scan the final `.app`. These checks are a guard against common mistakes, not a complete security or licensing review.

## Dependencies and contributions

Explain why a new dependency is needed and update the lockfile, provenance, license notices, and applicable dependency audit. Do not silently enable dependency download or installation in the offline build path.

Contributions are made under Apache License 2.0, as described by the repository LICENSE. Retain third-party attribution. There is no separate contributor agreement.
