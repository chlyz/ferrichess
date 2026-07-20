use std::collections::BTreeMap;

use crate::{Game, PlayerColor};

/// A result from the selected player's perspective.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerResult {
    Win,
    Draw,
    Loss,
    Unknown,
}

/// Frequency and results for one move following a requested position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Continuation {
    pub san: String,
    pub uci: String,
    pub games: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
}

impl Continuation {
    fn record(&mut self, result: PlayerResult) {
        self.games += 1;
        match result {
            PlayerResult::Win => self.wins += 1,
            PlayerResult::Draw => self.draws += 1,
            PlayerResult::Loss => self.losses += 1,
            PlayerResult::Unknown => {}
        }
    }
}

/// Counts next moves after an exact UCI prefix in games involving `username`.
pub fn continuations(
    games: &[Game],
    username: &str,
    color: Option<PlayerColor>,
    prefix: &[&str],
) -> Vec<Continuation> {
    let mut counts: BTreeMap<String, Continuation> = BTreeMap::new();

    for game in games {
        let Some(player_color) = game.player_color(username) else {
            continue;
        };
        if color.is_some_and(|wanted| wanted != player_color) {
            continue;
        }
        if game.moves.len() <= prefix.len()
            || !game
                .moves
                .iter()
                .zip(prefix)
                .all(|(game_ply, prefix_uci)| game_ply.uci == *prefix_uci)
        {
            continue;
        }

        let next = &game.moves[prefix.len()];
        let result = player_result(game.header("Result"), player_color);
        counts
            .entry(next.uci.clone())
            .or_insert_with(|| Continuation {
                san: next.san.clone(),
                uci: next.uci.clone(),
                games: 0,
                wins: 0,
                draws: 0,
                losses: 0,
            })
            .record(result);
    }

    let mut continuations: Vec<_> = counts.into_values().collect();
    continuations.sort_by(|left, right| {
        right
            .games
            .cmp(&left.games)
            .then_with(|| left.uci.cmp(&right.uci))
    });
    continuations
}

fn player_result(result: Option<&str>, color: PlayerColor) -> PlayerResult {
    match (result, color) {
        (Some("1-0"), PlayerColor::White) | (Some("0-1"), PlayerColor::Black) => PlayerResult::Win,
        (Some("1-0"), PlayerColor::Black) | (Some("0-1"), PlayerColor::White) => PlayerResult::Loss,
        (Some("1/2-1/2"), _) => PlayerResult::Draw,
        _ => PlayerResult::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_games;

    use super::*;

    #[test]
    fn counts_continuations_and_results_for_the_requested_player() {
        let games = parse_games(
            br#"
[White "Other"]
[Black "Example"]
[Result "0-1"]
1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 0-1

[White "Other"]
[Black "example"]
[Result "1-0"]
1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0

[White "Example"]
[Black "Other"]
[Result "1-0"]
1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 1-0
"#,
        )
        .expect("valid games");

        let stats = continuations(
            &games,
            "Example",
            Some(PlayerColor::Black),
            &["e2e4", "e7e5", "g1f3", "b8c6"],
        );
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].san, "Bb5");
        assert_eq!(stats[0].losses, 1);
        assert_eq!(stats[1].san, "Bc4");
        assert_eq!(stats[1].wins, 1);
    }
}
