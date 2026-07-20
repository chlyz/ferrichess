use std::path::{Path, PathBuf};

use ferrichess_pgn_index::query_position;

use crate::AppResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CourseIndex {
    pub label: String,
    pub database: PathBuf,
}

pub fn parse_course_index(value: &str) -> Result<CourseIndex, String> {
    let Some((label, database)) = value.split_once('=') else {
        return Err("course index must use LABEL=DATABASE".to_owned());
    };
    let label = label.trim();
    let database = database.trim();
    if label.is_empty() || database.is_empty() {
        return Err("course index label and database path cannot be empty".to_owned());
    }
    Ok(CourseIndex {
        label: label.to_owned(),
        database: PathBuf::from(database),
    })
}

pub fn build_report(
    fen: &str,
    position_report: &str,
    courses: &[CourseIndex],
) -> AppResult<String> {
    let mut output = String::from("# Repertoire position research\n\n");
    output.push_str(&position_report.replacen(
        "# Lichess position report",
        "## Candidate evidence",
        1,
    ));

    output.push_str("\n## Course evidence\n");
    if courses.is_empty() {
        output.push_str("\nNo course indexes were supplied.\n");
    }
    for course in courses {
        output.push_str(&format!("\n### {}\n\n", course.label));
        output.push_str(&query_course(&course.database, fen)?);
    }

    output.push_str(
        "\n## Explanation worksheet\n\n\
         - **Choice:** Which move are we selecting?\n\
         - **Purpose:** What does the move immediately accomplish?\n\
         - **Move order:** Why is it useful or necessary now?\n\
         - **Opponent's idea:** What plan, threat, or setup are we answering?\n\
         - **Piece placement:** Where do Black's pieces normally belong?\n\
         - **Pawn break:** Which central or flank break should Black prepare?\n\
         - **Typical mistake:** Which natural-looking move or plan should we avoid?\n\
         - **Tabiya:** At what stable position can memorisation stop and plans take over?\n\n\
         Database results and engine scores are evidence for the decision, not the explanation itself. Course comments and model games should be paraphrased into a short position-specific account.\n",
    );
    Ok(output)
}

fn query_course(database: &Path, fen: &str) -> AppResult<String> {
    query_position(database, fen).map_err(|error| {
        format!(
            "failed to query course index {}: {error}",
            database.display()
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ferrichess_pgn_index::build_index;

    use super::*;

    #[test]
    fn parses_labelled_course_indexes() {
        assert_eq!(
            parse_course_index("Jones=/courses/jones.sqlite3").unwrap(),
            CourseIndex {
                label: "Jones".to_owned(),
                database: PathBuf::from("/courses/jones.sqlite3"),
            }
        );
        assert!(parse_course_index("missing-label").is_err());
        assert!(parse_course_index("=/tmp/course.sqlite3").is_err());
    }

    #[test]
    fn combines_candidate_and_separate_course_evidence() {
        let root =
            std::env::temp_dir().join(format!("ferrichess-research-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let pgn = root.join("course.pgn");
        let database = root.join("course.sqlite3");
        fs::write(
            &pgn,
            "[Event \"Italian\"]\n\n1. e4 {Claims the centre.} e5 *\n",
        )
        .unwrap();
        build_index(&database, &[pgn.as_path()]).unwrap();

        let report = build_report(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "# Lichess position report\n\nCandidate table\n",
            &[CourseIndex {
                label: "Jones".to_owned(),
                database,
            }],
        )
        .unwrap();
        assert!(report.contains("## Candidate evidence"));
        assert!(report.contains("### Jones"));
        assert!(report.contains("Claims the centre."));
        assert!(report.contains("## Explanation worksheet"));
        fs::remove_dir_all(root).unwrap();
    }
}
