mod chesscom;
mod database;
mod engine;
mod http;
mod lichess;
mod model;
mod position;
mod research;

use std::{error::Error, fs, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use database::Archive;
use ferrichess_config::Config;
use ferrichess_games::{PlayerColor, continuations};

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Parser)]
#[command(about = "Synchronize and query a local personal chess-game archive")]
struct Cli {
    /// Archive root containing raw downloads, PGNs, reports, and SQLite data.
    #[arg(long, env = "FERRICHESS_GAMES_DIR")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create the archive directories and SQLite schema.
    Init,
    /// Synchronize games from one or both public player APIs.
    Sync {
        #[arg(long)]
        chesscom: Option<String>,
        #[arg(long)]
        lichess: Option<String>,
    },
    /// Show the moves played immediately after an exact UCI move prefix.
    Openings {
        #[arg(long)]
        player: String,
        #[arg(long, value_enum)]
        color: Option<ColorArg>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        time_class: Option<String>,
        /// Comma-separated UCI moves, for example e2e4,e7e5,g1f3,b8c6.
        #[arg(long, value_delimiter = ',')]
        prefix: Vec<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Compare candidate moves using the public Lichess opening explorer.
    PositionReport {
        /// Full FEN of the position to query.
        #[arg(long)]
        fen: String,
        /// SAN or UCI moves to include. By default all returned moves are shown.
        #[arg(long = "candidate")]
        candidates: Vec<String>,
        /// Comma-separated Lichess rating groups.
        #[arg(long, value_delimiter = ',', default_value = "1400,1600,1800")]
        ratings: Vec<u16>,
        /// Comma-separated Lichess speeds.
        #[arg(long, value_delimiter = ',', default_value = "rapid,classical")]
        speeds: Vec<String>,
        /// Maximum number of explorer moves to request.
        #[arg(long, default_value_t = 20)]
        moves: u8,
        /// Skip the Lichess cloud-evaluation lookup.
        #[arg(long)]
        no_cloud: bool,
        /// Run the local Stockfish engine even when cloud analysis exists.
        #[arg(long)]
        local_engine: bool,
        /// Search depth for local Stockfish analysis.
        #[arg(long, default_value_t = 20)]
        engine_depth: u8,
        /// Number of best Stockfish lines when no candidates are specified.
        #[arg(long, default_value_t = 5)]
        engine_lines: u8,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Combine explorer, engine, course-index, and model-game evidence.
    ResearchPosition {
        /// Full FEN of the position to query.
        #[arg(long)]
        fen: String,
        /// SAN or UCI moves to include before database-supplied candidates.
        #[arg(long = "candidate")]
        candidates: Vec<String>,
        /// Labelled course index in the form LABEL=DATABASE. Repeat as needed.
        #[arg(
            long = "course",
            value_name = "LABEL=DATABASE",
            value_parser = research::parse_course_index
        )]
        courses: Vec<research::CourseIndex>,
        /// Comma-separated Lichess rating groups.
        #[arg(long, value_delimiter = ',', default_value = "1400,1600,1800")]
        ratings: Vec<u16>,
        /// Comma-separated Lichess speeds.
        #[arg(long, value_delimiter = ',', default_value = "rapid,classical")]
        speeds: Vec<String>,
        /// Maximum number of explorer moves to request.
        #[arg(long, default_value_t = 20)]
        moves: u8,
        /// Skip the Lichess cloud-evaluation lookup.
        #[arg(long)]
        no_cloud: bool,
        /// Run the local Stockfish engine even when cloud analysis exists.
        #[arg(long)]
        local_engine: bool,
        /// Search depth for local Stockfish analysis.
        #[arg(long, default_value_t = 20)]
        engine_depth: u8,
        /// Number of best Stockfish lines when no candidates are specified.
        #[arg(long, default_value_t = 5)]
        engine_lines: u8,
        /// Write the private research report to this Markdown file.
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ColorArg {
    White,
    Black,
}

impl From<ColorArg> for PlayerColor {
    fn from(value: ColorArg) -> Self {
        match value {
            ColorArg::White => Self::White,
            ColorArg::Black => Self::Black,
        }
    }
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> AppResult<()> {
    let mut archive = Archive::open(&cli.root)?;
    match cli.command {
        Command::Init => {
            write_archive_readme(&cli.root)?;
            println!("initialized {}", cli.root.display());
        }
        Command::Sync {
            mut chesscom,
            mut lichess,
        } => {
            let config = Config::load_default()?;
            chesscom = chesscom.or(config.chesscom.username);
            lichess = lichess.or(config.lichess.username);
            if chesscom.is_none() && lichess.is_none() {
                return Err(
                    "sync requires --chesscom USER, --lichess USER, or usernames in Ferrichess config.toml"
                        .into(),
                );
            }
            if let Some(username) = chesscom {
                let imported = chesscom::sync(&cli.root, &mut archive, &username)?;
                println!("chess.com: synchronized {imported} games for {username}");
            }
            if let Some(username) = lichess {
                let imported = lichess::sync(&cli.root, &mut archive, &username)?;
                println!("lichess: synchronized {imported} games for {username}");
            }
            rebuild_combined_pgn(&cli.root)?;
            write_archive_readme(&cli.root)?;
            println!("archive total: {} games", archive.game_count()?);
        }
        Command::Openings {
            player,
            color,
            source,
            time_class,
            prefix,
            output,
        } => {
            let games = archive.load_games(&player, source.as_deref(), time_class.as_deref())?;
            let prefix_refs: Vec<_> = prefix.iter().map(String::as_str).collect();
            let stats = continuations(&games, &player, color.map(PlayerColor::from), &prefix_refs);
            let report = render_report(
                &player,
                color,
                source.as_deref(),
                time_class.as_deref(),
                &prefix,
                &stats,
            );
            if let Some(path) = output {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, report)?;
                println!("wrote {}", path.display());
            } else {
                print!("{report}");
            }
        }
        Command::PositionReport {
            fen,
            candidates,
            ratings,
            speeds,
            moves,
            no_cloud,
            local_engine,
            engine_depth,
            engine_lines,
            output,
        } => {
            let report = position::build_report(
                &fen,
                &position::ReportOptions {
                    candidates: &candidates,
                    ratings: &ratings,
                    speeds: &speeds,
                    move_limit: moves,
                    include_cloud: !no_cloud,
                    force_local: local_engine,
                    engine_depth,
                    engine_lines,
                },
            )?;
            if let Some(path) = output {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, report)?;
                println!("wrote {}", path.display());
            } else {
                print!("{report}");
            }
        }
        Command::ResearchPosition {
            fen,
            candidates,
            courses,
            ratings,
            speeds,
            moves,
            no_cloud,
            local_engine,
            engine_depth,
            engine_lines,
            output,
        } => {
            let position_report = position::build_report(
                &fen,
                &position::ReportOptions {
                    candidates: &candidates,
                    ratings: &ratings,
                    speeds: &speeds,
                    move_limit: moves,
                    include_cloud: !no_cloud,
                    force_local: local_engine,
                    engine_depth,
                    engine_lines,
                },
            )?;
            let report = research::build_report(&fen, &position_report, &courses)?;
            if let Some(path) = output {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, report)?;
                println!("wrote {}", path.display());
            } else {
                print!("{report}");
            }
        }
    }
    Ok(())
}

fn render_report(
    player: &str,
    color: Option<ColorArg>,
    source: Option<&str>,
    time_class: Option<&str>,
    prefix: &[String],
    stats: &[ferrichess_games::Continuation],
) -> String {
    let total: usize = stats.iter().map(|item| item.games).sum();
    let mut report = format!(
        "# Opening continuations\n\nPlayer: `{player}`  \nColor: `{}`  \nSource: `{}`  \nTime class: `{}`  \nUCI prefix: `{}`  \nMatching games: **{total}**\n\n",
        color.map_or("all", |value| match value {
            ColorArg::White => "white",
            ColorArg::Black => "black",
        }),
        source.unwrap_or("all"),
        time_class.unwrap_or("all"),
        prefix.join(" "),
    );
    report.push_str("| Move | Games | Share | W-D-L |\n|---|---:|---:|---:|\n");
    for item in stats {
        let share = if total == 0 {
            0.0
        } else {
            100.0 * item.games as f64 / total as f64
        };
        report.push_str(&format!(
            "| `{}` ({}) | {} | {:.1}% | {}-{}-{} |\n",
            item.san, item.uci, item.games, share, item.wins, item.draws, item.losses
        ));
    }
    report
}

fn rebuild_combined_pgn(root: &std::path::Path) -> AppResult<()> {
    let pgn_dir = root.join("pgn");
    fs::create_dir_all(&pgn_dir)?;
    let mut combined = String::new();
    for source in ["chesscom.pgn", "lichess.pgn"] {
        let path = pgn_dir.join(source);
        if path.exists() {
            let text = fs::read_to_string(path)?;
            combined.push_str(text.trim());
            combined.push_str("\n\n");
        }
    }
    fs::write(pgn_dir.join("all-games.pgn"), combined)?;
    Ok(())
}

fn write_archive_readme(root: &std::path::Path) -> AppResult<()> {
    let readme = "# Personal chess-game archive\n\n\
This directory contains local player data synchronized by `ferrichess-archive`.\n\
It is deliberately stored outside the public Ferrichess source repository.\n\n\
- `raw/`: original API responses\n\
- `pgn/`: source-specific and combined PGN exports\n\
- `reports/`: generated local reports\n\
- `games.sqlite3`: normalized metadata and legal mainline moves\n";
    fs::write(root.join("README.md"), readme)?;
    Ok(())
}
