//! Build and query disposable, course-specific FEN indexes from annotated PGN.

mod database;
mod pgn;

pub use pgn::{IndexedGame, Occurrence, parse_games};

use std::{error::Error, fs, path::Path};

use database::FenIndex;

pub type IndexResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexStats {
    pub games: i64,
    pub positions: i64,
    pub occurrences: i64,
}

/// One PGN input and the stable source name stored in the derived index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexSource<'a> {
    pub input: &'a Path,
    pub label: &'a Path,
}

/// Atomically rebuild one index from one or more PGNs.
///
/// The existing database remains untouched if reading, parsing, or indexing
/// any input fails.
pub fn build_index(database: &Path, pgn_paths: &[&Path]) -> IndexResult<IndexStats> {
    let sources = pgn_paths
        .iter()
        .map(|path| IndexSource {
            input: path,
            label: path,
        })
        .collect::<Vec<_>>();
    build_index_from_sources(database, &sources)
}

/// Atomically rebuild an index while recording stable labels for staged files.
pub fn build_index_from_sources(
    database: &Path,
    sources: &[IndexSource<'_>],
) -> IndexResult<IndexStats> {
    if let Some(parent) = database
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let file_name = database
        .file_name()
        .ok_or("database path must name a file")?
        .to_string_lossy();
    let temporary = database.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }

    let result = (|| -> IndexResult<IndexStats> {
        let mut index = FenIndex::create(&temporary)?;
        for source in sources {
            let bytes = fs::read(source.input)?;
            let games = pgn::parse_games(&bytes)?;
            index.insert_source(source.label, &games)?;
        }
        let (games, positions, occurrences) = index.counts()?;
        Ok(IndexStats {
            games,
            positions,
            occurrences,
        })
    })();

    match result {
        Ok(stats) => {
            fs::rename(&temporary, database)?;
            Ok(stats)
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// Query all occurrences and outgoing moves for a full FEN.
pub fn query_position(database: &Path, fen: &str) -> IndexResult<String> {
    let normalized = pgn::normalize_fen(fen)?;
    FenIndex::open(database)?.report(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_api_builds_and_queries_role_aware_index() {
        let root = std::env::temp_dir().join(format!(
            "ferrichess-index-library-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let pgn = root.join("course.pgn");
        let database = root.join("course.fen.sqlite3");
        fs::write(
            &pgn,
            "[Event \"Quick\"]\n[Chapter \"Italian\"]\n[RepertoireRole \"Quickstarter\"]\n\n1. e4 e5 *\n",
        )
        .unwrap();

        let stats = build_index(&database, &[&pgn]).unwrap();
        assert_eq!(stats.games, 1);
        let report = query_position(
            &database,
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        )
        .unwrap();
        assert!(report.contains("[Quickstarter]"));
        assert!(report.contains("e5 (e7e5)"));
        fs::remove_dir_all(root).unwrap();
    }
}
