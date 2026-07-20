use std::{fs, path::Path};

use ferrichess_games::parse_games;
use serde::Deserialize;

use crate::{AppResult, database::Archive, http, model::ImportRecord};

#[derive(Debug, Deserialize)]
struct ArchiveIndex {
    archives: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MonthlyArchive {
    games: Vec<ApiGame>,
}

#[derive(Debug, Deserialize)]
struct ApiGame {
    url: String,
    pgn: String,
    end_time: u64,
    rated: bool,
    time_class: String,
    rules: String,
    white: ApiPlayer,
    black: ApiPlayer,
}

#[derive(Debug, Deserialize)]
struct ApiPlayer {
    username: String,
    rating: Option<i64>,
}

pub fn sync(root: &Path, archive: &mut Archive, username: &str) -> AppResult<usize> {
    validate_username(username)?;
    let raw_dir = root.join("raw/chesscom");
    let index_url = format!("https://api.chess.com/pub/player/{username}/games/archives");
    let index_text = http::get_text(&index_url, "application/json")?;
    fs::write(raw_dir.join("archives.json"), &index_text)?;
    let index: ArchiveIndex = serde_json::from_str(&index_text)?;
    let last_archive = index.archives.last().cloned();
    let mut combined_pgn = String::new();
    let mut records = Vec::new();

    for url in &index.archives {
        let filename = archive_filename(url)?;
        let path = raw_dir.join(filename);
        let text = if path.exists() && last_archive.as_deref() != Some(url) {
            fs::read_to_string(&path)?
        } else {
            let text = http::get_text(url, "application/json")?;
            fs::write(&path, &text)?;
            text
        };
        let month: MonthlyArchive = serde_json::from_str(&text)?;
        for api_game in month.games {
            if api_game.rules != "chess" {
                continue;
            }
            let mut parsed = parse_games(api_game.pgn.as_bytes())?;
            if parsed.len() != 1 {
                return Err(format!("expected one PGN game from {}", api_game.url).into());
            }
            let game = parsed.remove(0);
            let record = ImportRecord {
                source: "chesscom",
                game_id: game_id(&api_game.url)?,
                site: api_game.url,
                played_at: api_game.end_time.to_string(),
                time_class: api_game.time_class,
                rated: api_game.rated,
                white_rating: api_game.white.rating,
                black_rating: api_game.black.rating,
                pgn: api_game.pgn.clone(),
                game,
            };
            if !record
                .game
                .header("White")
                .is_some_and(|name| name.eq_ignore_ascii_case(&api_game.white.username))
                || !record
                    .game
                    .header("Black")
                    .is_some_and(|name| name.eq_ignore_ascii_case(&api_game.black.username))
            {
                return Err(format!("player metadata mismatch for {}", record.site).into());
            }
            combined_pgn.push_str(api_game.pgn.trim());
            combined_pgn.push_str("\n\n");
            records.push(record);
        }
    }
    archive.import_many(&records)?;
    fs::write(root.join("pgn/chesscom.pgn"), combined_pgn)?;
    Ok(records.len())
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

fn archive_filename(url: &str) -> AppResult<String> {
    let mut parts = url.rsplit('/');
    let month = parts.next().ok_or("archive URL has no month")?;
    let year = parts.next().ok_or("archive URL has no year")?;
    Ok(format!("{year}-{month}.json"))
}

fn game_id(url: &str) -> AppResult<String> {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "game URL has no identifier".into())
}
