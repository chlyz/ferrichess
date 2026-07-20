use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::{IndexResult, pgn::IndexedGame};

pub struct FenIndex {
    connection: Connection,
}

impl FenIndex {
    pub fn open(path: &Path) -> IndexResult<Self> {
        Ok(Self {
            connection: Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?,
        })
    }

    pub fn create(path: &Path) -> IndexResult<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE sources (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE
             );
             CREATE TABLE games (
                 id INTEGER PRIMARY KEY,
                 source_id INTEGER NOT NULL REFERENCES sources(id),
                 source_game INTEGER NOT NULL,
                 event TEXT,
                 chapter TEXT,
                 repertoire_side TEXT,
                 repertoire_role TEXT,
                 repertoire_label TEXT,
                 tags_json TEXT NOT NULL,
                 UNIQUE(source_id, source_game)
             );
             CREATE TABLE positions (
                 id INTEGER PRIMARY KEY,
                 fen TEXT NOT NULL UNIQUE
             );
             CREATE TABLE occurrences (
                 id INTEGER PRIMARY KEY,
                 game_id INTEGER NOT NULL REFERENCES games(id),
                 position_id INTEGER NOT NULL REFERENCES positions(id),
                 parent_position_id INTEGER REFERENCES positions(id),
                 sequence INTEGER NOT NULL,
                 ply INTEGER NOT NULL,
                 san_path TEXT NOT NULL,
                 uci_path TEXT NOT NULL,
                 incoming_san TEXT,
                 incoming_uci TEXT,
                 comments TEXT NOT NULL,
                 nags TEXT NOT NULL
             );
             CREATE INDEX occurrences_position ON occurrences(position_id);
             CREATE INDEX occurrences_parent ON occurrences(parent_position_id);",
        )?;
        Ok(Self { connection })
    }

    pub fn insert_source(&mut self, label: &Path, games: &[IndexedGame]) -> IndexResult<()> {
        let transaction = self.connection.transaction()?;
        let source_path = label.to_string_lossy();
        transaction.execute("INSERT INTO sources(path) VALUES (?1)", [&source_path])?;
        let source_id = transaction.last_insert_rowid();

        for (game_index, game) in games.iter().enumerate() {
            let event = game.headers.get("Event");
            let chapter = game
                .headers
                .get("ChapterName")
                .or_else(|| game.headers.get("Chapter"));
            let repertoire_side = game.headers.get("RepertoireSide");
            let repertoire_role = game.headers.get("RepertoireRole");
            let repertoire_label = game.headers.get("RepertoireLabel");
            let tags_json = serde_json::to_string(&game.headers)?;
            transaction.execute(
                "INSERT INTO games(
                     source_id, source_game, event, chapter, repertoire_side,
                     repertoire_role, repertoire_label, tags_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    source_id,
                    (game_index + 1) as i64,
                    event,
                    chapter,
                    repertoire_side,
                    repertoire_role,
                    repertoire_label,
                    tags_json
                ],
            )?;
            let game_id = transaction.last_insert_rowid();

            for (sequence, occurrence) in game.occurrences.iter().enumerate() {
                let current_position_id = position_id(&transaction, &occurrence.fen)?;
                let parent_position_id = occurrence
                    .parent_fen
                    .as_deref()
                    .map(|fen| position_id(&transaction, fen))
                    .transpose()?;
                transaction.execute(
                    "INSERT INTO occurrences(
                         game_id, position_id, parent_position_id, sequence, ply,
                         san_path, uci_path, incoming_san, incoming_uci, comments, nags
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        game_id,
                        current_position_id,
                        parent_position_id,
                        sequence as i64,
                        occurrence.ply as i64,
                        occurrence.san_path,
                        occurrence.uci_path,
                        occurrence.incoming_san,
                        occurrence.incoming_uci,
                        occurrence.comments.join("\n\n"),
                        occurrence
                            .nags
                            .iter()
                            .map(|nag| format!("${nag}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                    ],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn counts(&self) -> IndexResult<(i64, i64, i64)> {
        Ok((
            self.connection
                .query_row("SELECT count(*) FROM games", [], |row| row.get(0))?,
            self.connection
                .query_row("SELECT count(*) FROM positions", [], |row| row.get(0))?,
            self.connection
                .query_row("SELECT count(*) FROM occurrences", [], |row| row.get(0))?,
        ))
    }

    pub fn report(&self, fen: &str) -> IndexResult<String> {
        let position_id = self
            .connection
            .query_row("SELECT id FROM positions WHERE fen = ?1", [fen], |row| {
                row.get::<_, i64>(0)
            })
            .optional()?;
        let Some(position_id) = position_id else {
            return Ok(format!("No occurrences found for `{fen}`.\n"));
        };

        let mut output = format!("Position: `{fen}`\n\nOccurrences:\n");
        let mut statement = self.connection.prepare(
            "SELECT s.path, g.source_game, coalesce(g.chapter, g.event, ''),
                    g.repertoire_role, g.repertoire_label,
                    o.san_path, o.comments, o.nags
             FROM occurrences o
             JOIN games g ON g.id = o.game_id
             JOIN sources s ON s.id = g.source_id
             WHERE o.position_id = ?1
             ORDER BY CASE g.repertoire_role
                        WHEN 'Main' THEN 0
                        WHEN 'Quickstarter' THEN 1
                        WHEN 'Alternative' THEN 2
                        WHEN 'Variant' THEN 3
                        ELSE 4
                      END,
                      s.path, g.source_game, o.sequence",
        )?;
        let rows = statement.query_map([position_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;
        for row in rows {
            let (source, game, chapter, role, label, path, comments, nags) = row?;
            output.push_str("- ");
            push_role(&mut output, role.as_deref(), label.as_deref());
            output.push_str(&format!("{source}, game {game}"));
            if !chapter.is_empty() {
                output.push_str(&format!(" ({chapter})"));
            }
            output.push_str(&format!("\n  Path: {}\n", display_path(&path)));
            if !nags.is_empty() {
                output.push_str(&format!("  Glyphs: {nags}\n"));
            }
            if !comments.is_empty() {
                output.push_str(&format!("  Comment: {}\n", comments.replace('\n', " ")));
            }
        }

        output.push_str("\nOutgoing moves:\n");
        let mut statement = self.connection.prepare(
            "SELECT s.path, g.source_game, coalesce(g.chapter, g.event, ''),
                    g.repertoire_role, g.repertoire_label,
                    o.incoming_san, o.incoming_uci, o.comments, o.nags, o.san_path
             FROM occurrences o
             JOIN games g ON g.id = o.game_id
             JOIN sources s ON s.id = g.source_id
             WHERE o.parent_position_id = ?1
             ORDER BY CASE g.repertoire_role
                        WHEN 'Main' THEN 0
                        WHEN 'Quickstarter' THEN 1
                        WHEN 'Alternative' THEN 2
                        WHEN 'Variant' THEN 3
                        ELSE 4
                      END,
                      s.path, g.source_game, o.sequence",
        )?;
        let rows = statement.query_map([position_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?;
        let mut found = false;
        for row in rows {
            found = true;
            let (source, game, chapter, role, label, san, uci, comments, nags, path) = row?;
            output.push_str(&format!("- {san} ({uci}) — "));
            push_role(&mut output, role.as_deref(), label.as_deref());
            output.push_str(&format!("{source}, game {game}"));
            if !chapter.is_empty() {
                output.push_str(&format!(" ({chapter})"));
            }
            output.push_str(&format!("\n  Path: {}\n", display_path(&path)));
            if !nags.is_empty() {
                output.push_str(&format!("  Glyphs: {nags}\n"));
            }
            if !comments.is_empty() {
                output.push_str(&format!("  Comment: {}\n", comments.replace('\n', " ")));
            }
        }
        if !found {
            output.push_str("- none\n");
        }
        Ok(output)
    }
}

fn push_role(output: &mut String, role: Option<&str>, label: Option<&str>) {
    if let Some(role) = role {
        output.push('[');
        output.push_str(role);
        if let Some(label) = label {
            output.push_str(": ");
            output.push_str(label);
        }
        output.push_str("] ");
    }
}

fn position_id(connection: &Connection, fen: &str) -> rusqlite::Result<i64> {
    connection.execute("INSERT OR IGNORE INTO positions(fen) VALUES (?1)", [fen])?;
    connection.query_row("SELECT id FROM positions WHERE fen = ?1", [fen], |row| {
        row.get(0)
    })
}

fn display_path(path: &str) -> &str {
    if path.is_empty() {
        "(starting position)"
    } else {
        path
    }
}
