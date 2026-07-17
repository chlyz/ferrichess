# Ferrichess Study

`ferrichess-study` converts compact chess study text into legal,
position-aware move trees and deterministic PGN. It is a library only: it does
not access the network or filesystem, and it does not include study content.

## Normal conversion path

Use [`convert_single_raw`] to build one `PgnDocument`, then render it with
[`PgnWriter`]. The input is interpreted from the standard initial chess
position.

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

`PgnWriter::render` returns one PGN game without a trailing newline.
`PgnWriter::render_file` returns the standalone-file form, ending in two
newlines. `PgnWriter::render_documents` joins multiple documents using that
same separator.

## Compact study-text format

The format is line-oriented and deliberately less strict than PGN. A line that
starts with a move number is a candidate mainline line; other nonblank lines
are prose comments attached at the current position.

Move numbers may use `N.` for White or `N...` for Black, with optional space
after the dots. They may be adjacent to SAN moves and to each other, so all of
these are equivalent where legal:

```text
1. e4 e5 2. Nf3 Nc6
1.e4e52.Nf3Nc6
1. e4e5 2.Nf3Nc6
```

Moves use legal SAN for the current position. The parser accepts `O-O` and
`0-0` spellings for castling, optional check or mate suffixes, and these move
annotations: `!`, `?`, `!!`, `??`, `!?`, `?!`, `=`, `∞`/`~`, `=∞`/`=~`,
`⩲`/`+=`, `⩱`/`=+`, `±`/`+/-`, `∓`/`-/+`, `+–`/`+-`, `–+`/`-+`, and
`⇆`/`<=>`. The writer retains move-quality annotations in SAN and renders
position evaluations as PGN NAGs.

The parser does not guess. A numbered line whose number backtracks, contains
no legal move, or leaves unparsed text becomes a normalized comment instead of
partially extending the mainline. Result tokens (`1-0`, `0-1`, `1/2-1/2`, and
`*`) end parsing of that line; output always uses the `*` result. `--` after a
move number is an instructional placeholder and is retained as a comment.

The compact format does not accept FEN setup, PGN header tags, PGN brace or
semicolon comment syntax, explicit recursive-annotation variations, or an
arbitrary starting position. For a variation found in prose, use
`RawTreeBuilder::with_comment_variations(true)` and provide a compatible,
explicitly numbered move sequence; this is an advanced tree-building option,
not part of the normal conversion path.

## Conversion model and API boundary

`convert_single_raw` is the stable, convenient boundary for a single study:
it parses the text, validates each SAN move against the current position,
constructs a move tree, and supplies deterministic PGN headers from
`SingleRawMetadata`. It returns `PgnError` if tree construction or rendering
validation fails.

For control over parsing and tree inspection, use `RawTreeBuilder` directly;
its `build` result contains both the `MoveTree` and per-line classifications.
For assembling a repertoire from several raw lines, use the advanced
`generate_course_documents` boundary with `CourseMetadata` and
`RawCourseLine`. That boundary is pure as well: callers supply all metadata
and text, then choose how to persist rendered output.

The first public metadata contract is intentionally small:

- `SingleRawMetadata` is the required metadata for normal single-study
  conversion.
- `RepertoireSide` and `RepertoireRole` describe how the study is oriented
  and labelled in its generated headers.
- `CourseMetadata`, its nested metadata types, and `RawCourseLine` support
  advanced multi-document assembly. They remain public in 0.x, but their JSON
  schema and assembly rules are not the minimal conversion API and may evolve
  in a minor release before 1.0.

## Scope and pre-1.0 stability

The crate currently provides raw study-text parsing, typed annotations and
comments, position-aware move-tree construction, course-document assembly, and
deterministic PGN rendering.

The crate follows Semantic Versioning with pre-1.0 expectations: patch releases
fix defects without intentional API or output-contract changes; minor releases
may change public APIs, the compact syntax, metadata schema, or deterministic
rendering details. Consumers that need reproducible output should pin a
compatible `0.x.y` version and keep regression fixtures for their input.

## License

Licensed under the GNU General Public License, version 3 or later
([GPL-3.0-or-later](LICENSE)).
