# Ferrichess Archive

`ferrichess-archive` creates a local, queryable copy of a player's public chess
games. The executable belongs in the public source repository; downloaded
games, usernames, reports, and SQLite databases do not.

## Archive layout

Given `--root /path/to/games`, the tool creates:

```text
games/
├── raw/
│   ├── chesscom/
│   └── lichess/
├── pgn/
│   ├── chesscom.pgn
│   ├── lichess.pgn
│   └── all-games.pgn
├── reports/
├── games.sqlite3
└── README.md
```

Chess.com synchronization retains immutable monthly responses and refreshes the
current month. Lichess synchronization currently refreshes the complete public
PGN export. Both sources are normalized into `games.sqlite3` using the source
and game identifier as the stable deduplication key.

## Commands

Initialize an archive:

```sh
cargo run -p ferrichess-archive -- \
  --root /path/to/games init
```

Synchronize either or both accounts:

```sh
cargo run -p ferrichess-archive -- \
  --root /path/to/games sync --chesscom USER --lichess USER
```

Usernames may instead be stored in
`$XDG_CONFIG_HOME/ferrichess/config.toml` (normally
`~/.config/ferrichess/config.toml`):

```toml
[lichess]
username = "your-lichess-name"

[chesscom]
username = "your-chesscom-name"
```

With configured usernames, `sync` needs no service arguments. This TOML file
contains non-secret preferences. Keep the Lichess API token in the separate
protected `lichess-token` file described below.

Count White's fifth-ply choices after `1.e4 e5 2.Nf3 Nc6` in the selected
player's rapid games as Black:

```sh
cargo run -p ferrichess-archive -- \
  --root /path/to/games openings \
  --player USER --color black --time-class rapid \
  --prefix e2e4,e7e5,g1f3,b8c6
```

Use `--source chesscom` or `--source lichess` to isolate one source, and
`--output /path/to/report.md` to save Markdown instead of printing it.

Compare candidate moves in a position using similarly rated Lichess rapid and
classical games, with a cached cloud evaluation when one exists:

```sh
cargo run -p ferrichess-archive -- \
  --root /path/to/games position-report \
  --fen "r1bqkb1r/p4ppp/2p2n2/n3p1N1/8/3B4/PPPP1PPP/RNBQK2R b KQkq - 1 8" \
  --candidate Nd5 --candidate Ng4 \
  --ratings 1400,1600,1800 --speeds rapid,classical
```

Candidates may be written as SAN (`Nd5`) or UCI (`f6d5`). If the explorer
receives explicit candidates, it preserves their order, reports them first in
bold, and supplements them to a five-move shortlist. Moves found in both the
masters and 2200/2500 Lichess databases are added first, followed by
master-only and then high-Elo-only moves. The `Basis` column makes that origin
visible.

If the explorer
requires authenticated access, the tool first checks `LICHESS_TOKEN` and then
`$XDG_CONFIG_HOME/ferrichess/lichess-token` (or
`~/.config/ferrichess/lichess-token`). Token files must not be accessible by
group or other users; mode `600` is recommended. Tokens are never accepted as
command-line arguments or written to the archive. Cloud evaluations are cached
data and may be unavailable or contain fewer lines than requested. When
`stockfish` is installed, missing cloud evaluations and incomplete cloud
coverage automatically fall back to it; only uncovered candidates are checked
locally. A complete cloud answer does not run the local engine. Use
`--local-engine` to run Stockfish for every candidate even when cloud data is
complete, and `--engine-depth` to control the search.
The default local search depth is `20`. Without `--candidate`, Stockfish
searches all legal moves and reports its five best lines as a safety net around
human candidate sources. Use `--engine-lines` to request up to ten lines. With
one or more `--candidate` options, Stockfish restricts its search to those moves
instead. When available, the report also links one representative master game
for each shortlisted move.

Combine that evidence with separately labelled, course-specific FEN indexes:

```sh
cargo run -p ferrichess-archive -- \
  --root /path/to/games research-position \
  --fen "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4" \
  --course Jones=/path/to/jones/course.fen.sqlite3 \
  --course Valkova=/path/to/valkova/course.fen.sqlite3 \
  --output /path/to/private-research/italian.md
```

`research-position` preserves each course's provenance, includes its position
and outgoing-move comments, and adds a worksheet for turning statistics,
course prose, and model games into a concise explanation. Repeat `--candidate`
for moves that must appear first. The generated report can contain licensed
course comments; keep it with the private course exports and do not commit it
to the public Ferrichess repository.

## Privacy and repository boundary

The APIs expose public games, but a compiled personal archive is still personal
data. Keep the archive outside a source checkout and do not commit it. The
Ferrichess repository contains only source-neutral code and synthetic tests.
