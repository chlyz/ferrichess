# Contributing to Ferrichess

Thank you for considering a contribution. Ferrichess is currently pre-1.0 and
developed publicly without crates.io releases, so please discuss a substantial
API, format, or metadata-schema change before investing in an implementation.

## Local setup

Install the stable Rust toolchain and run these checks from the workspace root:

```sh
cargo fmt --check --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

All workspace packages must set `publish = false`. Do not run `cargo publish`
or prepare a crates.io release unless the repository has explicitly adopted a
release process.

## Changes and tests

Keep the normal conversion path pure: the library must not perform filesystem
or network access. Preserve deterministic PGN rendering and position-aware
move validation. Add focused tests for changed behavior, including malformed
or ambiguous input where relevant.

Only add redistributable material. Do not commit captured studies, downloaded
player-game archives, personal reports, private collections, usernames,
absolute home-directory paths, platform-specific source material, credentials,
browser-session data, or test data with uncertain provenance. Keep personal
game databases and reports outside this repository. Document every external
chess seed retained for tests in
[docs/test-data-provenance.md](docs/test-data-provenance.md).

## Commit messages

Use Conventional Commits with an imperative, lowercase summary and no trailing
period. For example: `feat: render stable chapter documents`.

Use `feat:` for a user-visible capability, `fix:` for a bug fix, `refactor:`
for internal restructuring, `test:` for test-only changes, `docs:` for
documentation-only changes, and `chore:` for maintenance or tooling.

## Compatibility

The crate follows pre-1.0 Semantic Versioning. Patch releases should preserve
the documented API and deterministic-output contract; minor releases may make
intentional compatibility changes. Keep the compact format and public API
documentation in sync with behavior.
