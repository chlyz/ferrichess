use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use shakmaty::{Bitboard, Board, Color, EnPassantMode, Position, Square};

/// The side for which repertoire moves are selected.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum RepertoireSide {
    White,
    Black,
}

impl RepertoireSide {
    #[must_use]
    pub const fn color(self) -> Color {
        match self {
            Self::White => Color::White,
            Self::Black => Color::Black,
        }
    }

    #[must_use]
    pub fn is_repertoire_turn(self, turn: Color) -> bool {
        self.color() == turn
    }
}

impl From<RepertoireSide> for Color {
    fn from(side: RepertoireSide) -> Self {
        side.color()
    }
}

/// Whether a chapter is a primary repertoire chapter or a variant chapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RepertoireRole {
    Main,
    Variant,
}

/// A human-readable move annotation or position evaluation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Annotation {
    Good,
    Mistake,
    Brilliant,
    Blunder,
    Interesting,
    Dubious,
    Equal,
    Unclear,
    EqualWithCounterplay,
    WhiteSlightAdvantage,
    BlackSlightAdvantage,
    WhiteAdvantage,
    BlackAdvantage,
    WhiteWinning,
    BlackWinning,
    Counterplay,
}

impl Annotation {
    /// Returns the standard PGN numeric annotation glyph for a position
    /// evaluation. Move-quality annotations retain their SAN suffixes.
    #[must_use]
    pub const fn evaluation_nag(self) -> Option<u16> {
        match self {
            Self::Equal => Some(10),
            Self::EqualWithCounterplay => Some(12),
            Self::Unclear => Some(13),
            Self::WhiteSlightAdvantage => Some(14),
            Self::BlackSlightAdvantage => Some(15),
            Self::WhiteAdvantage => Some(16),
            Self::BlackAdvantage => Some(17),
            Self::WhiteWinning => Some(18),
            Self::BlackWinning => Some(19),
            Self::Counterplay => Some(132),
            Self::Good
            | Self::Mistake
            | Self::Brilliant
            | Self::Blunder
            | Self::Interesting
            | Self::Dubious => None,
        }
    }

    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Good => "!",
            Self::Mistake => "?",
            Self::Brilliant => "!!",
            Self::Blunder => "??",
            Self::Interesting => "!?",
            Self::Dubious => "?!",
            Self::Equal => "=",
            Self::Unclear => "∞",
            Self::EqualWithCounterplay => "=∞",
            Self::WhiteSlightAdvantage => "⩲",
            Self::BlackSlightAdvantage => "⩱",
            Self::WhiteAdvantage => "±",
            Self::BlackAdvantage => "∓",
            Self::WhiteWinning => "+–",
            Self::BlackWinning => "–+",
            Self::Counterplay => "⇆",
        }
    }

    /// Returns an ASCII spelling suitable for normalized raw/comment text.
    #[must_use]
    pub const fn ascii_suffix(self) -> &'static str {
        match self {
            Self::Good => "!",
            Self::Mistake => "?",
            Self::Brilliant => "!!",
            Self::Blunder => "??",
            Self::Interesting => "!?",
            Self::Dubious => "?!",
            Self::Equal => "=",
            Self::Unclear => "~",
            Self::EqualWithCounterplay => "=~",
            Self::WhiteSlightAdvantage => "+=",
            Self::BlackSlightAdvantage => "=+",
            Self::WhiteAdvantage => "+/-",
            Self::BlackAdvantage => "-/+",
            Self::WhiteWinning => "+-",
            Self::BlackWinning => "-+",
            Self::Counterplay => "<=>",
        }
    }

    #[must_use]
    pub fn from_suffix(suffix: &str) -> Option<Self> {
        match suffix {
            "!" => Some(Self::Good),
            "?" => Some(Self::Mistake),
            "!!" => Some(Self::Brilliant),
            "??" => Some(Self::Blunder),
            "!?" => Some(Self::Interesting),
            "?!" => Some(Self::Dubious),
            "=" => Some(Self::Equal),
            "∞" | "~" => Some(Self::Unclear),
            "=∞" | "=~" => Some(Self::EqualWithCounterplay),
            "⩲" | "+=" => Some(Self::WhiteSlightAdvantage),
            "⩱" | "=+" => Some(Self::BlackSlightAdvantage),
            "±" | "+/-" => Some(Self::WhiteAdvantage),
            "∓" | "-/+" => Some(Self::BlackAdvantage),
            "+–" | "+-" => Some(Self::WhiteWinning),
            "–+" | "-+" => Some(Self::BlackWinning),
            "⇆" | "<=>" => Some(Self::Counterplay),
            _ => None,
        }
    }
}

/// A limit measured in moves made by the repertoire side.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum DepthLimit {
    #[default]
    Unlimited,
    RepertoireMoves(u32),
}

impl DepthLimit {
    #[must_use]
    pub const fn includes(self, repertoire_moves: u32) -> bool {
        match self {
            Self::Unlimited => true,
            Self::RepertoireMoves(limit) => repertoire_moves <= limit,
        }
    }
}

/// Stable identity of an input source.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(PathBuf);

impl SourceId {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl<T: Into<PathBuf>> From<T> for SourceId {
    fn from(path: T) -> Self {
        Self::new(path)
    }
}

/// The first four FEN fields, with en passant normalized to legal captures.
///
/// Move clocks are deliberately excluded because they do not identify a
/// repertoire position.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PositionKey {
    board: Board,
    turn: Color,
    castling_rights: Bitboard,
    ep_square: Option<Square>,
}

impl PositionKey {
    #[must_use]
    pub fn from_position(position: &impl Position) -> Self {
        Self {
            board: position.board().clone(),
            turn: position.turn(),
            castling_rights: position.castles().castling_rights(),
            ep_square: position.ep_square(EnPassantMode::Legal),
        }
    }

    #[must_use]
    pub const fn board(&self) -> &Board {
        &self.board
    }

    #[must_use]
    pub const fn turn(&self) -> Color {
        self.turn
    }

    #[must_use]
    pub const fn castling_rights(&self) -> Bitboard {
        self.castling_rights
    }

    #[must_use]
    pub const fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }
}

#[cfg(test)]
mod tests {
    use shakmaty::{Chess, Move, Position, Role, Square};

    use super::{Annotation, DepthLimit, PositionKey};

    #[test]
    fn annotations_have_readable_suffixes() {
        assert_eq!(Annotation::Good.suffix(), "!");
        assert_eq!(Annotation::Dubious.suffix(), "?!");
        assert_eq!(Annotation::Blunder.suffix(), "??");
        assert_eq!(Annotation::Counterplay.suffix(), "⇆");
        assert_eq!(Annotation::WhiteWinning.suffix(), "+–");
        assert_eq!(Annotation::WhiteWinning.evaluation_nag(), Some(18));
        assert_eq!(Annotation::BlackWinning.evaluation_nag(), Some(19));
        assert_eq!(Annotation::EqualWithCounterplay.evaluation_nag(), Some(12));
        assert_eq!(Annotation::Good.evaluation_nag(), None);
        assert_eq!(Annotation::WhiteWinning.ascii_suffix(), "+-");
        assert_eq!(
            Annotation::from_suffix("+-"),
            Some(Annotation::WhiteWinning)
        );
    }

    #[test]
    fn depth_limit_counts_repertoire_moves_inclusively() {
        assert!(DepthLimit::Unlimited.includes(u32::MAX));
        assert!(DepthLimit::RepertoireMoves(2).includes(2));
        assert!(!DepthLimit::RepertoireMoves(2).includes(3));
    }

    #[test]
    fn position_key_uses_only_legal_en_passant_squares() {
        let initial = Chess::default();
        let after_e4 = initial
            .play(Move::Normal {
                role: Role::Pawn,
                from: Square::E2,
                capture: None,
                to: Square::E4,
                promotion: None,
            })
            .expect("e4 is legal in the initial position");

        let key = PositionKey::from_position(&after_e4);
        assert_eq!(key.turn(), shakmaty::Color::Black);
        assert_eq!(key.ep_square(), None);
    }
}
