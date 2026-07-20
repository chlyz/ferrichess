# ferrichess-course-merge

Build a small set of Lichess-friendly PGN trees from the chapter PGNs of an
exported repertoire course. Source chapters are merged positionally, so their
lines become variations within one game rather than separate games.

```sh
cargo run -p ferrichess-course-merge -- \
  /path/to/exported-course \
  repertoires/grouping.json \
  /path/to/output
```

The source export is never modified. The output contains one PGN per group, a
multi-game `course.pgn`, the copied manifest, and merge diagnostics retaining
the source grouping and any repertoire conflicts.

If `[lichess].username` is configured in the shared Ferrichess `config.toml`,
it is written to each derived PGN as the `Annotator`. The Lichess API token
remains in the separate `lichess-token` file and is never read by this tool.
