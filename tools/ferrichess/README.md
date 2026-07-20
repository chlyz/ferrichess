# Ferrichess CLI

`ferrichess` provides pull-first workflows for authoritative personal studies.
Pulling never creates, edits, or deletes Lichess studies. Publishing is a
separate, guarded whole-study replacement workflow and is always non-mutating
unless all destructive confirmation options are supplied.

Configure studies in `$XDG_CONFIG_HOME/ferrichess/config.toml` (normally
`~/.config/ferrichess/config.toml`):

```toml
[studies.example-white-course]
study_id = "abcdefgh"
directory = "/path/to/repertoires/example-white-course"
course_directory = "/path/to/courses/example-white-course"
```

Keep the API token in the separate `lichess-token` file with mode `0600`. The
token needs only `study:read` permission.

Pull every configured study:

```sh
cargo run -p ferrichess-cli -- study pull
```

Or pull selected studies:

```sh
cargo run -p ferrichess-cli -- study pull example-white-course
```

Each successful pull atomically replaces `study.pgn` and rebuilds the derived
`study.fen.sqlite3`. If `course_directory` is configured, its existing
`course.pgn` and `course.fen.sqlite3` are validated as separate read-only
reference material. They are never merged into the authoritative study.

## Guarded publishing

First generate a candidate PGN outside the authoritative study directory. A
plain publish command downloads Lichess and prints a plan; it never writes:

```sh
cargo run -p ferrichess-cli -- study publish \
  example-white-course /path/to/candidate/course.pgn
```

The plan compares the live export with the last pulled `study.pgn`. If somebody
renamed, removed, reordered, or edited a chapter on Lichess after that pull,
replacement is refused. Pull and review those changes before planning again.

A whole-study replacement requires all three explicit guards, using values
printed by the immediately preceding plan:

```sh
cargo run -p ferrichess-cli -- study publish \
  example-white-course /path/to/candidate/course.pgn \
  --replace-all \
  --expected-remote-sha256 SHA256_FROM_PLAN \
  --confirm-study-id abcdefgh
```

Before the first remote mutation, Ferrichess saves the complete live PGN under
`.ferrichess-publish/` in the configured study directory and writes a recovery
journal as replacement progresses. Candidate chapters are imported and
verified before superseded chapters are deleted wherever the 64-chapter
Lichess limit permits it. The final live export must match the candidate's
chapter names, orientations, annotators, moves, variations, comments, and NAGs
before `study.pgn` is refreshed.
