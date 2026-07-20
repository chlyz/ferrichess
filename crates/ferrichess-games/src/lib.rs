//! Source-neutral parsing and queries for personal chess-game archives.
//!
//! This crate operates on caller-provided PGN bytes. It performs no network or
//! filesystem access and contains no player data.

mod pgn;
mod stats;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use pgn::{PgnParseError, parse_games};
pub use stats::{Continuation, PlayerResult, continuations};

/// The color a player had in a game.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerColor {
    White,
    Black,
}

/// One legal mainline move represented for display and stable comparison.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GamePly {
    pub san: String,
    pub uci: String,
}

/// A parsed standard-chess PGN game.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub headers: BTreeMap<String, String>,
    pub moves: Vec<GamePly>,
}

impl Game {
    /// Returns a PGN header using its case-sensitive standard name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    /// Returns the player's color using a case-insensitive username match.
    pub fn player_color(&self, username: &str) -> Option<PlayerColor> {
        if self
            .header("White")
            .is_some_and(|white| white.eq_ignore_ascii_case(username))
        {
            Some(PlayerColor::White)
        } else if self
            .header("Black")
            .is_some_and(|black| black.eq_ignore_ascii_case(username))
        {
            Some(PlayerColor::Black)
        } else {
            None
        }
    }
}
