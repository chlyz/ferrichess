# Ferrichess Study

`ferrichess-study` is a Rust library for parsing compact chess study text into
legal, position-aware move trees and rendering deterministic PGN.

The input syntax is intentionally more forgiving than PGN. It accepts compact
move numbers and SAN moves that may be joined together, while retaining text
that cannot be read as a legal move as a comment. Move recognition is validated
against the current chess position.

The crate is source-agnostic: it does not access the network or filesystem and
does not include chess-course content.

## Example

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

## Scope and stability

The crate currently provides raw study-text parsing, typed annotations and
comments, position-aware move-tree construction, course-document assembly, and
deterministic PGN rendering.

The public API and input syntax are pre-1.0. Consumers that require reproducible
output should pin a compatible version.

## License

The license for the first public release has not been selected yet. Do not
redistribute the crate until a license is added.
