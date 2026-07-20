use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    io::{self, Cursor},
    ops::ControlFlow,
};

use pgn_reader::{RawTag, Reader, SanPlus, Visitor};
use shakmaty::{CastlingMode, Chess, Position};

use crate::{Game, GamePly};

/// A failure to read PGN syntax or validate a mainline move.
#[derive(Debug)]
pub enum PgnParseError {
    Io(io::Error),
    IllegalMove { ply: usize, san: String },
}

impl fmt::Display for PgnParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::IllegalMove { ply, san } => {
                write!(formatter, "illegal PGN mainline move at ply {ply}: {san}")
            }
        }
    }
}

impl Error for PgnParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::IllegalMove { .. } => None,
        }
    }
}

impl From<io::Error> for PgnParseError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
struct Movetext {
    headers: BTreeMap<String, String>,
    position: Chess,
    moves: Vec<GamePly>,
}

#[derive(Debug)]
struct GameVisitor;

impl Visitor for GameVisitor {
    type Tags = BTreeMap<String, String>;
    type Movetext = Movetext;
    type Output = Result<Game, PgnParseError>;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(BTreeMap::new())
    }

    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        tags.insert(
            String::from_utf8_lossy(name).into_owned(),
            value.decode_utf8_lossy().into_owned(),
        );
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, headers: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        ControlFlow::Continue(Movetext {
            headers,
            position: Chess::default(),
            moves: Vec::new(),
        })
    }

    fn san(
        &mut self,
        movetext: &mut Self::Movetext,
        san_plus: SanPlus,
    ) -> ControlFlow<Self::Output> {
        let san = san_plus.to_string();
        let chess_move = match san_plus.san.to_move(&movetext.position) {
            Ok(chess_move) => chess_move,
            Err(_) => {
                return ControlFlow::Break(Err(PgnParseError::IllegalMove {
                    ply: movetext.moves.len() + 1,
                    san,
                }));
            }
        };
        let uci = chess_move.to_uci(CastlingMode::Standard).to_string();
        movetext.position.play_unchecked(chess_move);
        movetext.moves.push(GamePly { san, uci });
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, movetext: Self::Movetext) -> Self::Output {
        Ok(Game {
            headers: movetext.headers,
            moves: movetext.moves,
        })
    }
}

/// Parses all PGN games, retaining tags and legal mainline moves.
///
/// Recursive annotation variations, comments, and glyphs are intentionally
/// skipped: personal opening statistics describe moves actually played.
pub fn parse_games(bytes: &[u8]) -> Result<Vec<Game>, PgnParseError> {
    let mut reader = Reader::new(Cursor::new(bytes));
    let mut visitor = GameVisitor;
    let mut games = Vec::new();
    while let Some(game) = reader.read_game(&mut visitor)? {
        games.push(game?);
    }
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_mainlines_and_skips_variations() {
        let pgn = br#"
[Event "First"]
[White "Example"]
[Black "Opponent"]
[Result "1-0"]

1. e4 e5 2. Nf3 (2. f4) Nc6 1-0

[Event "Second"]
[White "Opponent"]
[Black "Example"]
[Result "0-1"]

1. d4 Nf6 2. c4 e6 0-1
"#;

        let games = parse_games(pgn).expect("valid games");
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].moves[2].san, "Nf3");
        assert_eq!(games[0].moves[2].uci, "g1f3");
        assert_eq!(games[0].moves.len(), 4);
        assert_eq!(games[1].header("Event"), Some("Second"));
    }

    #[test]
    fn rejects_an_illegal_mainline() {
        let error = parse_games(b"1. e4 e5 2. Bh6 *").expect_err("illegal bishop move");
        assert!(matches!(error, PgnParseError::IllegalMove { ply: 3, .. }));
    }
}
