# Ferrichess CLI

`ferrichess` provides pull-only workflows for authoritative personal studies.
It never creates, edits, or deletes Lichess studies.

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
