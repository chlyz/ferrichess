use std::{error::Error, path::PathBuf};

use clap::{Parser, Subcommand};
use ferrichess_pgn_index::{build_index, query_position};

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Parser)]
#[command(about = "Build and query a course-specific FEN index from annotated PGN")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build one fresh database from one or more PGN files.
    Build {
        #[arg(short, long)]
        database: PathBuf,
        #[arg(required = true)]
        pgn: Vec<PathBuf>,
    },
    /// Find all occurrences and outgoing moves for an exact position.
    Query {
        #[arg(short, long)]
        database: PathBuf,
        /// Full FEN copied from a chess editor.
        #[arg(long)]
        fen: String,
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
        Command::Build { database, pgn } => {
            let paths = pgn.iter().map(PathBuf::as_path).collect::<Vec<_>>();
            let stats = build_index(&database, &paths)?;
            println!(
                "indexed {} games, {} positions, and {} occurrences into {}",
                stats.games,
                stats.positions,
                stats.occurrences,
                database.display()
            );
            Ok(())
        }
        Command::Query { database, fen } => {
            print!("{}", query_position(&database, &fen)?);
            Ok(())
        }
    }
}
