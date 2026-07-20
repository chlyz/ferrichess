use std::{
    env,
    error::Error,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use ferrichess_config::{Config, StudyConfig};
use ferrichess_pgn_index::{IndexSource, build_index_from_sources, parse_games};

type AppResult<T> = Result<T, Box<dyn Error>>;

const MAX_STUDY_BYTES: u64 = 100_000_000;

#[derive(Debug, Parser)]
#[command(about = "Pull authoritative chess studies into local snapshots")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Work with authoritative Lichess studies.
    Study {
        #[command(subcommand)]
        command: StudyCommand,
    },
}

#[derive(Debug, Subcommand)]
enum StudyCommand {
    /// Download configured studies and rebuild their local FEN indexes.
    Pull {
        /// Configured study names. Omit to pull every configured study.
        names: Vec<String>,
    },
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> AppResult<()> {
    match cli.command {
        Command::Study {
            command: StudyCommand::Pull { names },
        } => pull_studies(&Config::load_default()?, &names),
    }
}

fn pull_studies(config: &Config, names: &[String]) -> AppResult<()> {
    let selected = select_studies(config, names)?;
    let token = load_lichess_token()?;
    for (name, study) in selected {
        validate_study_id(&study.study_id)?;
        validate_course_reference(study)?;
        let pgn = download_study(&study.study_id, &token)?;
        let stats = save_study(&study.directory, &pgn)?;
        println!(
            "{name}: pulled {} chapters and indexed {} positions into {}",
            stats.chapters,
            stats.positions,
            study.directory.display()
        );
    }
    Ok(())
}

fn validate_course_reference(study: &StudyConfig) -> AppResult<()> {
    let Some(directory) = &study.course_directory else {
        return Ok(());
    };
    if !directory.is_dir() {
        return Err(format!(
            "reference course directory does not exist: {}",
            directory.display()
        )
        .into());
    }
    for file in ["course.pgn", "course.fen.sqlite3"] {
        let path = directory.join(file);
        if !path.is_file() {
            return Err(format!("reference course is missing {}", path.display()).into());
        }
    }
    Ok(())
}

fn select_studies<'a>(
    config: &'a Config,
    names: &[String],
) -> AppResult<Vec<(&'a str, &'a StudyConfig)>> {
    if config.studies.is_empty() {
        return Err("no studies configured in Ferrichess config.toml".into());
    }
    if names.is_empty() {
        return Ok(config
            .studies
            .iter()
            .map(|(name, study)| (name.as_str(), study))
            .collect());
    }
    names
        .iter()
        .map(|name| {
            config
                .studies
                .get_key_value(name)
                .map(|(configured_name, study)| (configured_name.as_str(), study))
                .ok_or_else(|| format!("unknown configured study {name:?}").into())
        })
        .collect()
}

fn validate_study_id(study_id: &str) -> AppResult<()> {
    if study_id.len() != 8 || !study_id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(format!("invalid Lichess study ID {study_id:?}").into());
    }
    Ok(())
}

fn download_study(study_id: &str, token: &str) -> AppResult<String> {
    let url = format!("https://lichess.org/api/study/{study_id}.pgn");
    let mut response = ureq::get(&url)
        .query("clocks", "false")
        .query("comments", "true")
        .query("variations", "true")
        .query("orientation", "true")
        .header("Accept", "application/x-chess-pgn")
        .header("Authorization", format!("Bearer {token}"))
        .header(
            "User-Agent",
            "ferrichess/0.1 (pull-only personal repertoire backup)",
        )
        .call()?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(MAX_STUDY_BYTES)
        .read_to_string()?)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PullStats {
    chapters: usize,
    positions: i64,
}

fn save_study(directory: &Path, pgn: &str) -> AppResult<PullStats> {
    let chapters = parse_games(pgn.as_bytes())?.len();
    if chapters == 0 {
        return Err("downloaded study contains no PGN chapters".into());
    }
    fs::create_dir_all(directory)?;
    let process = std::process::id();
    let staged_pgn = directory.join(format!(".study.pgn.pull-{process}"));
    let staged_database = directory.join(format!(".study.fen.sqlite3.pull-{process}"));
    let final_pgn = directory.join("study.pgn");
    let final_database = directory.join("study.fen.sqlite3");

    let result = (|| -> AppResult<PullStats> {
        fs::write(&staged_pgn, pgn)?;
        let index = build_index_from_sources(
            &staged_database,
            &[IndexSource {
                input: &staged_pgn,
                label: &final_pgn,
            }],
        )?;
        fs::rename(&staged_pgn, &final_pgn)?;
        fs::rename(&staged_database, &final_database)?;
        Ok(PullStats {
            chapters,
            positions: index.positions,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(staged_pgn);
        let _ = fs::remove_file(staged_database);
    }
    result
}

fn load_lichess_token() -> AppResult<String> {
    if let Ok(token) = env::var("LICHESS_TOKEN") {
        return nonempty_token(token, "LICHESS_TOKEN");
    }
    let path = default_token_path().ok_or("cannot resolve the Lichess token path")?;
    read_token_file(&path)
}

fn default_token_path() -> Option<PathBuf> {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("ferrichess/lichess-token"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/ferrichess/lichess-token"))
}

fn read_token_file(path: &Path) -> AppResult<String> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(format!(
            "Lichess token path is not a regular file: {}",
            path.display()
        )
        .into());
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(format!(
            "Lichess token file {} has mode {mode:o}; use chmod 600 {}",
            path.display(),
            path.display()
        )
        .into());
    }
    nonempty_token(fs::read_to_string(path)?, &path.display().to_string())
}

fn nonempty_token(token: String, source: &str) -> AppResult<String> {
    let token = token.trim().to_owned();
    if token.is_empty() {
        Err(format!("Lichess token from {source} is empty").into())
    } else {
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn study(id: &str, directory: &str) -> StudyConfig {
        StudyConfig {
            study_id: id.to_owned(),
            directory: PathBuf::from(directory),
            course_directory: None,
        }
    }

    #[test]
    fn selects_all_studies_in_stable_name_order() {
        let config = Config {
            studies: BTreeMap::from([
                ("white".to_owned(), study("abcdefgh", "/white")),
                ("black".to_owned(), study("hgfedcba", "/black")),
            ]),
            ..Config::default()
        };
        let selected = select_studies(&config, &[]).unwrap();
        assert_eq!(
            selected.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["black", "white"]
        );
    }

    #[test]
    fn rejects_unknown_names_and_malformed_ids() {
        let config = Config {
            studies: BTreeMap::from([("white".to_owned(), study("abcdefgh", "/white"))]),
            ..Config::default()
        };
        assert!(select_studies(&config, &["missing".to_owned()]).is_err());
        assert!(validate_study_id("short").is_err());
        assert!(validate_study_id("abc-defg").is_err());
        validate_study_id("aB12cd34").unwrap();
    }

    #[test]
    fn validates_an_optional_reference_course() {
        let directory = env::temp_dir().join(format!(
            "ferrichess-course-reference-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let mut configured = study("abcdefgh", "/study");
        configured.course_directory = Some(directory.clone());
        assert!(validate_course_reference(&configured).is_err());
        fs::write(directory.join("course.pgn"), "*").unwrap();
        fs::write(directory.join("course.fen.sqlite3"), "index").unwrap();
        validate_course_reference(&configured).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn saves_a_study_snapshot_and_derived_index() {
        let directory =
            env::temp_dir().join(format!("ferrichess-study-pull-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let pgn = concat!(
            "[Event \"Chapter one\"]\n",
            "[ChapterName \"One\"]\n\n",
            "1. e4 e5 *\n\n",
            "[Event \"Chapter two\"]\n",
            "[ChapterName \"Two\"]\n\n",
            "1. d4 d5 *\n",
        );
        let stats = save_study(&directory, pgn).unwrap();
        assert_eq!(stats.chapters, 2);
        assert!(stats.positions > 0);
        assert_eq!(
            fs::read_to_string(directory.join("study.pgn")).unwrap(),
            pgn
        );
        assert!(directory.join("study.fen.sqlite3").is_file());
        let report = ferrichess_pgn_index::query_position(
            &directory.join("study.fen.sqlite3"),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        )
        .unwrap();
        assert!(report.contains(&directory.join("study.pgn").display().to_string()));
        assert!(!report.contains(".study.pgn.pull-"));
        fs::remove_dir_all(directory).unwrap();
    }
}
