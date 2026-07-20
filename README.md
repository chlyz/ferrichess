# Ferrichess

Ferrichess is a public Rust workspace for turning compact chess study text into
legal, position-aware move trees and deterministic PGN. It is source-agnostic:
the library does not access the network or filesystem, and this repository
contains no chess-study content.

Ferrichess is currently developed in public but is not published to crates.io.
Every workspace package must set `publish = false` until the project explicitly
adopts a release process.

The workspace currently provides:

- `ferrichess-study`, which accepts closely written legal SAN moves such as
  `1.e4e52.Nf3Nc6`, preserves prose as comments, and renders stable PGN;
- `ferrichess-games`, a source-neutral library for parsing legal mainlines and
  computing personal opening continuations; and
- `ferrichess-archive`, a local CLI that synchronizes public Chess.com and
  Lichess games into raw snapshots, PGN, and SQLite; and
- `ferrichess-pgn-index`, a local CLI that creates a course-specific,
  variation-aware FEN index from one or more annotated PGNs; and
- `ferrichess`, a pull-only CLI that snapshots authoritative Lichess studies
  locally and rebuilds their FEN indexes without writing to Lichess.

## Generated PGN contract

`course.pgn` is the ordered multi-game container for every game in every
chapter. A repertoire chapter commonly contributes one merged game, while a
tactics chapter can contribute many puzzle games. Repertoire courses may also
produce `black-full.pgn` and `white-full.pgn`: each is one conflict-free tree
built exclusively from chapters classified as `Main`. Quickstarter and
alternative chapters remain in `course.pgn` but cannot influence a side-full
move through output ordering. Tactics courses do not produce side-full PGNs.

See the [study crate README](crates/ferrichess-study/README.md) and
[archive-tool README](tools/ferrichess-archive/README.md) for their contracts.

## Authoritative Lichess repertoires

Configure named studies in the private Ferrichess `config.toml`, then pull all
of them with:

```sh
cargo run -p ferrichess-cli -- study pull
```

Pass one or more configured names to pull only those studies. Pulling downloads
comments, variations, graphical annotations, and orientation tags into
`study.pgn`, then rebuilds `study.fen.sqlite3`. An optional `course_directory`
couples the authoritative study to read-only reference material without ever
copying course moves back into the study. The command never modifies Lichess.

## Getting started

During development, use a path dependency on a local checkout:

```toml
[dependencies]
ferrichess-study = { path = "../ferrichess/crates/ferrichess-study" }
```

Adjust the relative path for the consuming repository. Published crates.io
versions are historical snapshots and are not the active development channel.

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
the repository documentation for data-provenance requirements.

## License

Ferrichess is licensed under the GNU General Public License, version 3 or later
([GPL-3.0-or-later](LICENSE)).
