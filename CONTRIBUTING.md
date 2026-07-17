# Contributing to Ferrichess

Thank you for considering a contribution. Ferrichess is currently pre-1.0, so
please discuss a substantial API, format, or metadata-schema change before
investing in an implementation.

## Local setup

Install the stable Rust toolchain and run these checks from the workspace root:

```sh
cargo fmt --check --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Run `cargo package --manifest-path crates/ferrichess-study/Cargo.toml` when a
change affects the crate manifest, included files, README, license, or other
packaging-relevant material.

## Changes and tests

Keep the normal conversion path pure: the library must not perform filesystem
or network access. Preserve deterministic PGN rendering and position-aware
move validation. Add focused tests for changed behavior, including malformed
or ambiguous input where relevant.

Only add redistributable material. Do not commit captured studies, private
collections, private paths, platform-specific source material, credentials, or
test data with uncertain provenance. Document external chess seeds in
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
