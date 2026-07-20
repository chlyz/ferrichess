use std::{fs, path::Path};

use ferrichess_games::{Game, parse_games};

use crate::{AppResult, database::Archive, http, model::ImportRecord};

pub fn sync(root: &Path, archive: &mut Archive, username: &str) -> AppResult<usize> {
    validate_username(username)?;
    let url = format!(
        "https://lichess.org/api/games/user/{username}?perfType=ultraBullet,bullet,blitz,rapid,classical,correspondence&moves=true&tags=true&clocks=false&evals=false&opening=true&literate=false"
    );
    let pgn = http::get_text(&url, "application/x-chess-pgn")?;
    fs::write(root.join(format!("raw/lichess/{username}.pgn")), &pgn)?;
    fs::write(root.join("pgn/lichess.pgn"), &pgn)?;

    let games = parse_games(pgn.as_bytes())?;
    let chunks = split_pgn_games(&pgn);
    if games.len() != chunks.len() {
        return Err(format!(
            "Lichess PGN boundary mismatch: parsed {} games but found {} raw games",
            games.len(),
            chunks.len()
        )
        .into());
    }

    let imported = games.len();
    let mut records = Vec::with_capacity(imported);
    for (game, raw_pgn) in games.into_iter().zip(chunks) {
        let site = game.header("Site").unwrap_or("").to_owned();
        let record = ImportRecord {
            source: "lichess",
            game_id: game_id(&game)?,
            site,
            played_at: played_at(&game),
            time_class: time_class(&game),
            rated: game
                .header("Event")
                .is_some_and(|event| event.to_ascii_lowercase().contains("rated")),
            white_rating: rating(&game, "WhiteElo"),
            black_rating: rating(&game, "BlackElo"),
            pgn: raw_pgn.to_owned(),
            game,
        };
        records.push(record);
    }
    archive.import_many(&records)?;
    Ok(imported)
}

fn split_pgn_games(pgn: &str) -> Vec<&str> {
    let mut starts = Vec::new();
    if pgn.starts_with("[Event ") {
        starts.push(0);
    }
    for (index, _) in pgn.match_indices("\n[Event ") {
        starts.push(index + 1);
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(pgn.len());
            pgn[*start..end].trim()
        })
        .filter(|game| !game.is_empty())
        .collect()
}

fn game_id(game: &Game) -> AppResult<String> {
    if let Some(id) = game.header("GameId") {
        return Ok(id.to_owned());
    }
    game.header("Site")
        .and_then(|site| site.strip_prefix("https://lichess.org/"))
        .and_then(|path| path.split('/').next())
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Lichess PGN has no GameId or recognizable Site".into())
}

fn played_at(game: &Game) -> String {
    match (game.header("UTCDate"), game.header("UTCTime")) {
        (Some(date), Some(time)) => format!("{date} {time}"),
        (Some(date), None) => date.to_owned(),
        _ => game.header("Date").unwrap_or("").to_owned(),
    }
}

fn rating(game: &Game, header: &str) -> Option<i64> {
    game.header(header)?.parse().ok()
}

fn time_class(game: &Game) -> String {
    if let Some(speed) = game.header("Speed") {
        return speed.to_ascii_lowercase();
    }
    let Some(time_control) = game.header("TimeControl") else {
        return "unknown".to_owned();
    };
    let mut parts = time_control.split('+');
    let Some(base) = parts.next().and_then(|part| part.parse::<u64>().ok()) else {
        return "unknown".to_owned();
    };
    let increment = parts
        .next()
        .and_then(|part| part.parse::<u64>().ok())
        .unwrap_or(0);
    let estimated = base.saturating_add(increment.saturating_mul(40));
    match estimated {
        0..=29 => "ultrabullet",
        30..=179 => "bullet",
        180..=479 => "blitz",
        480..=1499 => "rapid",
        _ => "classical",
    }
    .to_owned()
}

fn validate_username(username: &str) -> AppResult<()> {
    if username.is_empty()
        || !username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("username may contain only ASCII letters, digits, '_' and '-'".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_multi_game_export_without_copying_player_data() {
        let pgn = "[Event \"One\"]\n\n1. e4 *\n\n[Event \"Two\"]\n\n1. d4 *\n";
        let games = split_pgn_games(pgn);
        assert_eq!(games.len(), 2);
        assert!(games[1].starts_with("[Event \"Two\"]"));
    }
}
