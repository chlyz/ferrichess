# Ferrichess

Ferrichess is a Rust workspace for turning compact chess study text into legal,
position-aware move trees and deterministic PGN. It is source-agnostic: the
library does not access the network or filesystem, and this repository contains
no chess-study content.

The workspace currently provides the `ferrichess-study` library crate. Its
compact format accepts closely written legal SAN moves such as
`1.e4e52.Nf3Nc6`, preserves prose as comments, and renders stable PGN output.
See the [crate README](crates/ferrichess-study/README.md) for the full format
and API contract.

## Getting started

Add the crate to a Rust project once it is published:

```toml
[dependencies]
ferrichess-study = "0.1"
```

The normal conversion path supplies small, explicit metadata and renders the
resulting PGN:

```rust
use ferrichess_study::{
    convert_single_raw, PgnWriter, RepertoireRole, RepertoireSide,
    SingleRawMetadata,
};

let metadata = SingleRawMetadata {
    course_title: "Example study".to_owned(),
    event: "Open games".to_owned(),
    chapter_slug: "open-games".to_owned(),
    index: "001".to_owned(),
    repertoire_side: RepertoireSide::White,
    repertoire_role: RepertoireRole::Main,
};

let document = convert_single_raw("1. e4e52. Nf3Nc6", &metadata)?;
let pgn = PgnWriter::render(&document)?;
assert!(pgn.contains("1. e4 e5 2. Nf3 Nc6 *"));
# Ok::<(), ferrichess_study::PgnError>(())
```

## Development

Install the stable Rust toolchain, then run the local quality gates:

```sh
cargo fmt --check --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution expectations and
[PLAN.md](PLAN.md) for the pre-release plan.

## License

Ferrichess is licensed under the GNU General Public License, version 3 or later
([GPL-3.0-or-later](LICENSE)).
