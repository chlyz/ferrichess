use std::{collections::BTreeMap, fmt, io::Cursor, ops::ControlFlow};

use pgn_reader::{Nag, RawComment, RawTag, Reader, SanPlus, Skip, Visitor};
use shakmaty::{
    CastlingMode, Chess, EnPassantMode, Position,
    fen::{Epd, Fen},
};

#[derive(Clone, Debug)]
struct State {
    position: Chess,
    san_path: Vec<String>,
    uci_path: Vec<String>,
    occurrence: usize,
}

#[derive(Debug)]
struct Movetext {
    headers: BTreeMap<String, String>,
    states: Vec<State>,
    resume: Vec<Vec<State>>,
    occurrences: Vec<Occurrence>,
    partial_comment: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Occurrence {
    pub fen: String,
    pub parent_fen: Option<String>,
    pub ply: usize,
    pub san_path: String,
    pub uci_path: String,
    pub incoming_san: Option<String>,
    pub incoming_uci: Option<String>,
    pub comments: Vec<String>,
    pub nags: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct IndexedGame {
    pub headers: BTreeMap<String, String>,
    pub occurrences: Vec<Occurrence>,
}

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    InvalidStartingPosition(String),
    IllegalMove { path: String, san: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::InvalidStartingPosition(error) => {
                write!(formatter, "invalid PGN starting position: {error}")
            }
            Self::IllegalMove { path, san } => write!(formatter, "illegal move {san} after {path}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
struct IndexVisitor;

impl Visitor for IndexVisitor {
    type Tags = BTreeMap<String, String>;
    type Movetext = Movetext;
    type Output = Result<IndexedGame, ParseError>;

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
        let position = match starting_position(&headers) {
            Ok(position) => position,
            Err(error) => return ControlFlow::Break(Err(error)),
        };
        let root = Occurrence {
            fen: normalized_fen(&position),
            parent_fen: None,
            ply: 0,
            san_path: String::new(),
            uci_path: String::new(),
            incoming_san: None,
            incoming_uci: None,
            comments: Vec::new(),
            nags: Vec::new(),
        };
        ControlFlow::Continue(Movetext {
            headers,
            states: vec![State {
                position,
                san_path: Vec::new(),
                uci_path: Vec::new(),
                occurrence: 0,
            }],
            resume: Vec::new(),
            occurrences: vec![root],
            partial_comment: String::new(),
        })
    }

    fn san(
        &mut self,
        movetext: &mut Self::Movetext,
        san_plus: SanPlus,
    ) -> ControlFlow<Self::Output> {
        let current = movetext.states.last().expect("root state").clone();
        let san = san_plus.to_string();
        let chess_move = match san_plus.san.to_move(&current.position) {
            Ok(chess_move) => chess_move,
            Err(_) => {
                return ControlFlow::Break(Err(ParseError::IllegalMove {
                    path: current.san_path.join(" "),
                    san,
                }));
            }
        };
        let uci = chess_move.to_uci(CastlingMode::Standard).to_string();
        let parent_fen = normalized_fen(&current.position);
        let mut position = current.position.clone();
        position.play_unchecked(chess_move);
        let mut san_path = current.san_path;
        san_path.push(san.clone());
        let mut uci_path = current.uci_path;
        uci_path.push(uci.clone());
        let occurrence = movetext.occurrences.len();
        movetext.occurrences.push(Occurrence {
            fen: normalized_fen(&position),
            parent_fen: Some(parent_fen),
            ply: san_path.len(),
            san_path: san_path.join(" "),
            uci_path: uci_path.join(" "),
            incoming_san: Some(san),
            incoming_uci: Some(uci),
            comments: Vec::new(),
            nags: Vec::new(),
        });
        movetext.states.push(State {
            position,
            san_path,
            uci_path,
            occurrence,
        });
        ControlFlow::Continue(())
    }

    fn nag(&mut self, movetext: &mut Self::Movetext, nag: Nag) -> ControlFlow<Self::Output> {
        let occurrence = movetext.states.last().expect("root state").occurrence;
        movetext.occurrences[occurrence].nags.push(nag.0);
        ControlFlow::Continue(())
    }

    fn comment(
        &mut self,
        movetext: &mut Self::Movetext,
        comment: RawComment<'_>,
    ) -> ControlFlow<Self::Output> {
        movetext
            .partial_comment
            .push_str(&String::from_utf8_lossy(comment.0));
        let text = movetext.partial_comment.trim().to_owned();
        movetext.partial_comment.clear();
        if !text.is_empty() {
            let occurrence = movetext.states.last().expect("root state").occurrence;
            movetext.occurrences[occurrence].comments.push(text);
        }
        ControlFlow::Continue(())
    }

    fn partial_comment(
        &mut self,
        movetext: &mut Self::Movetext,
        comment: RawComment<'_>,
    ) -> ControlFlow<Self::Output> {
        movetext
            .partial_comment
            .push_str(&String::from_utf8_lossy(comment.0));
        ControlFlow::Continue(())
    }

    fn begin_variation(
        &mut self,
        movetext: &mut Self::Movetext,
    ) -> ControlFlow<Self::Output, Skip> {
        movetext.resume.push(movetext.states.clone());
        if movetext.states.len() > 1 {
            movetext.states.pop();
        }
        ControlFlow::Continue(Skip(false))
    }

    fn end_variation(&mut self, movetext: &mut Self::Movetext) -> ControlFlow<Self::Output> {
        if let Some(states) = movetext.resume.pop() {
            movetext.states = states;
        }
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, movetext: Self::Movetext) -> Self::Output {
        Ok(IndexedGame {
            headers: movetext.headers,
            occurrences: movetext.occurrences,
        })
    }
}

fn starting_position(headers: &BTreeMap<String, String>) -> Result<Chess, ParseError> {
    match headers.get("FEN") {
        Some(fen) => parse_position(fen),
        None => Ok(Chess::default()),
    }
}

pub fn normalize_fen(fen: &str) -> Result<String, ParseError> {
    let position = parse_position(fen)?;
    Ok(normalized_fen(&position))
}

fn parse_position(fen: &str) -> Result<Chess, ParseError> {
    let parsed = fen
        .parse::<Fen>()
        .map_err(|error| ParseError::InvalidStartingPosition(error.to_string()))?
        .into_position(CastlingMode::Standard);
    match parsed {
        Ok(position) => Ok(position),
        Err(error) => error
            .ignore_too_much_material()
            .map_err(|error| ParseError::InvalidStartingPosition(error.to_string())),
    }
}

fn normalized_fen(position: &Chess) -> String {
    Epd::from_position(position, EnPassantMode::Legal).to_string()
}

pub fn parse_games(bytes: &[u8]) -> Result<Vec<IndexedGame>, ParseError> {
    let mut reader = Reader::new(Cursor::new(bytes));
    let mut visitor = IndexVisitor;
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
    fn indexes_mainline_variations_comments_and_glyphs() {
        let games = parse_games(
            br#"[Event "Italian"]

1. e4 {centre} e5 2. Nf3 Nc6 (2... Nf6 $5 {Petrov}) 3. Bc4 *
"#,
        )
        .expect("valid PGN");

        let game = &games[0];
        assert_eq!(game.occurrences.len(), 7);
        assert_eq!(game.occurrences[1].comments, ["centre"]);
        let petrov = game
            .occurrences
            .iter()
            .find(|occurrence| occurrence.incoming_san.as_deref() == Some("Nf6"))
            .expect("variation");
        assert_eq!(petrov.san_path, "e4 e5 Nf3 Nf6");
        assert_eq!(petrov.comments, ["Petrov"]);
        assert_eq!(petrov.nags, [5]);
    }

    #[test]
    fn normalizes_away_move_clocks() {
        let first = normalize_fen("8/8/8/8/8/4k3/8/4K3 w - - 0 1").unwrap();
        let second = normalize_fen("8/8/8/8/8/4k3/8/4K3 w - - 37 82").unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn indexes_composed_positions_with_excess_material() {
        let games = parse_games(
            br#"[SetUp "1"]
[FEN "r4rk1/1ppqn1p1/4pn1p/pP1pp3/P3p2P/1QPP2PB/3N1P2/R3R1K1 w - - 0 18"]

18. Rxe4 Nxe4 19. Nxe4 b6 *
"#,
        )
        .expect("playable composed position");

        assert_eq!(games[0].occurrences.len(), 5);
    }

    #[test]
    fn preserves_comments_larger_than_the_reader_buffer() {
        let comment = "explanation ".repeat(1_000);
        let pgn = format!("1. e4 {{{comment}}} *");
        let games = parse_games(pgn.as_bytes()).expect("valid PGN");
        assert_eq!(games[0].occurrences[1].comments, [comment.trim()]);
    }
}
