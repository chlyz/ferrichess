# ferrichess-course-merge

Build a small set of Lichess-friendly PGN trees from the chapter PGNs of an
exported repertoire course. Every lesson game in each source chapter is merged
positionally, so its lines become variations within one game rather than
remaining separate games.

```sh
cargo run -p ferrichess-course-merge -- \
  /path/to/exported-course \
  repertoires/grouping.json \
  /path/to/output
```

The source export is never modified. The output contains one PGN per group, a
multi-game `course.pgn`, the copied manifest, and merge diagnostics retaining
the source grouping and any repertoire conflicts. The merge report records its
source root relative to the output directory so it remains portable when the
repertoire repository is cloned.

Each group may set `repertoireSide` to `White` or `Black`; it overrides the
manifest-level default and controls both merge semantics and the Lichess board
orientation. Group titles are written as Lichess `ChapterName` headers, so the
manifest controls the exact imported chapter names and order.

If `[lichess].username` is configured in the shared Ferrichess `config.toml`,
its `https://lichess.org/@/USERNAME` profile URL is written to each derived PGN
as the `Annotator`. The Lichess API token remains in the separate
`lichess-token` file and is never read by this tool.
