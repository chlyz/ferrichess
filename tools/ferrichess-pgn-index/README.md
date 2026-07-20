# Ferrichess PGN index

`ferrichess-pgn-index` builds one disposable, course-specific SQLite database
from one or more annotated PGN files. It indexes mainlines and recursive
annotation variations, retains comments and numeric annotation glyphs, and
deduplicates positions by the first four FEN fields. Legal en-passant squares
are preserved; move clocks are ignored.

The PGNs remain the source of truth. Re-run `build` after they change:

```sh
cargo run -p ferrichess-pgn-index -- build \
  --database /path/to/course.fen.sqlite3 \
  /path/to/chapter-1.pgn /path/to/chapter-2.pgn
```

Query with a full FEN copied from Lichess or another editor:

```sh
cargo run -p ferrichess-pgn-index -- query \
  --database /path/to/course.fen.sqlite3 \
  --fen 'rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2'
```

Do not combine unrelated courses in one database when provenance must remain
strictly separated. Supplying several PGNs is intended for chapters belonging
to the same course. Generated indexes can contain the PGNs' comments and must
remain outside the public source repository; the recommended
`*.fen.sqlite3` filenames are ignored by Git as an additional safeguard.
