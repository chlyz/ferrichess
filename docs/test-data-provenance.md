# Test-data provenance

The legal move seeds in `ferrichess-study/src/test_support.rs` come from the
CC0 [Lichess Open Database][database] export:

```text
URL:      https://database.lichess.org/standard/lichess_db_standard_rated_2013-01.pgn.zst
SHA-256:  aa40b3671fa3cf1072eb182892cd90b0e1e003a4a5943492f64b77e7f3fd1635
License:  CC0 1.0 Universal
```

The compressed export, game headers, and player names are not stored in this
repository. Only these prefixes are copied:

| Seed | Game URL | Prefix |
| --- | --- | --- |
| `FRENCH_PREFIX`, `FRENCH_MATE` | `https://lichess.org/j1dkb5dw` | `1. e4 e6 2. d4 b6 3. a3 Bb7`; full game prefix through `13. Qe8#` |
| `COLLE_PREFIX` | `https://lichess.org/a9tcp02g` | `1. d4 d5 2. Nf3 Nf6 3. e3 Bf5` |
| `ITALIAN_PREFIX`, `ITALIAN_CHECK` | `https://lichess.org/szom2tog` | prefix through `4... Bc5`; prefix through `11. Nxc7+` |
| `CARO_KANN_PREFIX` | `https://lichess.org/rklpc7mk` | `1. e4 c6 2. Nc3 d5 3. Qf3 dxe4 4. Nxe4 Nd7` |
| `ENGLUND_PREFIX` | `https://lichess.org/9opx3qh7` | `1. d4 e5 2. dxe5 d6 3. exd6 Bxd6 4. Nf3 Nf6` |

## Synthetic test fixtures

Every other chess sequence, FEN, comment, annotation, malformed fragment,
expected SAN/PGN string, UCI string, position key, metadata value, and
documentation example is synthetic and authored for Ferrichess. Tests may
compact or truncate a listed prefix and add test-specific branches, comments,
or annotations; they do not copy source-game commentary or annotations.

[database]: https://database.lichess.org/
