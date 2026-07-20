use std::{collections::BTreeMap, path::Path};

use ferrichess_games::{Game, GamePly};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{AppResult, model::ImportRecord};

pub struct Archive {
    connection: Connection,
}

impl Archive {
    pub fn open(root: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(root.join("raw/chesscom"))?;
        std::fs::create_dir_all(root.join("raw/lichess"))?;
        std::fs::create_dir_all(root.join("pgn"))?;
        std::fs::create_dir_all(root.join("reports"))?;
        let connection = Connection::open(root.join("games.sqlite3"))?;
        let archive = Self { connection };
        archive.initialize()?;
        Ok(archive)
    }

    #[cfg(test)]
    fn in_memory() -> AppResult<Self> {
        let archive = Self {
            connection: Connection::open_in_memory()?,
        };
        archive.initialize()?;
        Ok(archive)
    }

    fn initialize(&self) -> AppResult<()> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS games (
                 source TEXT NOT NULL,
                 game_id TEXT NOT NULL,
                 site TEXT NOT NULL,
                 played_at TEXT NOT NULL,
                 time_class TEXT NOT NULL,
                 rated INTEGER NOT NULL,
                 white TEXT NOT NULL,
                 black TEXT NOT NULL,
                 white_rating INTEGER,
                 black_rating INTEGER,
                 result TEXT NOT NULL,
                 pgn TEXT NOT NULL,
                 PRIMARY KEY (source, game_id)
             );
             CREATE TABLE IF NOT EXISTS moves (
                 source TEXT NOT NULL,
                 game_id TEXT NOT NULL,
                 ply INTEGER NOT NULL,
                 san TEXT NOT NULL,
                 uci TEXT NOT NULL,
                 PRIMARY KEY (source, game_id, ply),
                 FOREIGN KEY (source, game_id)
                     REFERENCES games(source, game_id) ON DELETE CASCADE
             );
             CREATE INDEX IF NOT EXISTS games_players
                 ON games(white, black, time_class, source);",
        )?;
        Ok(())
    }

    pub fn import_many(&mut self, records: &[ImportRecord]) -> AppResult<()> {
        let transaction = self.connection.transaction()?;
        for record in records {
            let white = record.game.header("White").unwrap_or("?");
            let black = record.game.header("Black").unwrap_or("?");
            let result = record.game.header("Result").unwrap_or("*");
            transaction.execute(
                "INSERT INTO games (
                 source, game_id, site, played_at, time_class, rated,
                 white, black, white_rating, black_rating, result, pgn
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(source, game_id) DO UPDATE SET
                 site = excluded.site,
                 played_at = excluded.played_at,
                 time_class = excluded.time_class,
                 rated = excluded.rated,
                 white = excluded.white,
                 black = excluded.black,
                 white_rating = excluded.white_rating,
                 black_rating = excluded.black_rating,
                 result = excluded.result,
                 pgn = excluded.pgn",
                params![
                    record.source,
                    record.game_id,
                    record.site,
                    record.played_at,
                    record.time_class,
                    record.rated,
                    white,
                    black,
                    record.white_rating,
                    record.black_rating,
                    result,
                    record.pgn,
                ],
            )?;
            transaction.execute(
                "DELETE FROM moves WHERE source = ?1 AND game_id = ?2",
                params![record.source, record.game_id],
            )?;
            {
                let mut insert = transaction.prepare(
                    "INSERT INTO moves (source, game_id, ply, san, uci)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                )?;
                for (index, chess_move) in record.game.moves.iter().enumerate() {
                    insert.execute(params![
                        record.source,
                        record.game_id,
                        (index + 1) as i64,
                        chess_move.san,
                        chess_move.uci,
                    ])?;
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn game_count(&self) -> AppResult<i64> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM games", [], |row| row.get(0))?)
    }

    pub fn load_games(
        &self,
        player: &str,
        source: Option<&str>,
        time_class: Option<&str>,
    ) -> AppResult<Vec<Game>> {
        let mut statement = self.connection.prepare(
            "SELECT source, game_id, white, black, result
             FROM games
             WHERE (lower(white) = lower(?1) OR lower(black) = lower(?1))
               AND (?2 IS NULL OR source = ?2)
               AND (?3 IS NULL OR time_class = ?3)
             ORDER BY played_at, source, game_id",
        )?;
        let rows = statement.query_map(params![player, source, time_class], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        let mut games = Vec::new();
        for row in rows {
            let (source, game_id, white, black, result) = row?;
            let mut move_statement = self.connection.prepare(
                "SELECT san, uci FROM moves
                 WHERE source = ?1 AND game_id = ?2 ORDER BY ply",
            )?;
            let moves = move_statement
                .query_map(params![source, game_id], |row| {
                    Ok(GamePly {
                        san: row.get(0)?,
                        uci: row.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let headers = BTreeMap::from([
                ("White".to_owned(), white),
                ("Black".to_owned(), black),
                ("Result".to_owned(), result),
            ]);
            games.push(Game { headers, moves });
        }
        Ok(games)
    }

    #[allow(dead_code)]
    pub fn latest_played_at(&self, source: &str) -> AppResult<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT max(played_at) FROM games WHERE source = ?1",
                [source],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }
}

#[cfg(test)]
mod tests {
    use ferrichess_games::parse_games;

    use super::*;

    #[test]
    fn replaces_games_and_reloads_normalized_moves() {
        let mut archive = Archive::in_memory().expect("archive");
        let pgn = "[White \"Other\"]\n[Black \"Example\"]\n[Result \"0-1\"]\n\n1. e4 e5 0-1\n";
        let game = parse_games(pgn.as_bytes()).unwrap().remove(0);
        let record = ImportRecord {
            source: "test",
            game_id: "one".to_owned(),
            site: "local".to_owned(),
            played_at: "2026.07.19".to_owned(),
            time_class: "rapid".to_owned(),
            rated: true,
            white_rating: Some(1200),
            black_rating: Some(1300),
            pgn: pgn.to_owned(),
            game,
        };
        archive.import_many(std::slice::from_ref(&record)).unwrap();
        archive.import_many(std::slice::from_ref(&record)).unwrap();

        assert_eq!(archive.game_count().unwrap(), 1);
        let games = archive
            .load_games("example", Some("test"), Some("rapid"))
            .unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].moves[1].uci, "e7e5");
    }
}
