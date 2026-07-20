use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{AppResult, engine, http};

const EXPLORER_URL: &str = "https://explorer.lichess.org/lichess";
const MASTERS_URL: &str = "https://explorer.lichess.org/masters";
const CLOUD_EVAL_URL: &str = "https://lichess.org/api/cloud-eval";
const HIGH_RATINGS: &str = "2200,2500";
const TARGET_CANDIDATES: usize = 5;

#[derive(Debug, Deserialize)]
struct ExplorerResponse {
    white: u64,
    draws: u64,
    black: u64,
    moves: Vec<ExplorerMove>,
    #[serde(default, rename = "topGames")]
    top_games: Vec<MasterGame>,
}

#[derive(Clone, Debug, Deserialize)]
struct MasterGame {
    id: String,
    #[serde(default)]
    uci: Option<String>,
    winner: Option<String>,
    white: MasterPlayer,
    black: MasterPlayer,
    year: u16,
}

#[derive(Clone, Debug, Deserialize)]
struct MasterPlayer {
    name: String,
    rating: u16,
}

#[derive(Clone, Debug, Deserialize)]
struct ExplorerMove {
    uci: String,
    san: String,
    white: u64,
    draws: u64,
    black: u64,
    #[serde(default)]
    game: Option<MasterGame>,
}

#[derive(Debug)]
struct CandidateMove {
    move_data: ExplorerMove,
    requested: bool,
    masters: bool,
    high_elo: bool,
    master_game: Option<MasterGame>,
}

impl CandidateMove {
    fn basis(&self) -> String {
        let mut sources = Vec::new();
        if self.requested {
            sources.push("requested");
        }
        if self.masters {
            sources.push("masters");
        }
        if self.high_elo {
            sources.push("high Elo");
        }
        sources.join(" + ")
    }
}

impl ExplorerMove {
    fn games(&self) -> u64 {
        self.white + self.draws + self.black
    }
}

#[derive(Debug, Deserialize)]
struct CloudEvaluation {
    depth: u32,
    knodes: u64,
    pvs: Vec<CloudPv>,
}

#[derive(Debug, Deserialize)]
struct CloudPv {
    moves: String,
    cp: Option<i32>,
    mate: Option<i32>,
}

pub fn build_report(fen: &str, options: &ReportOptions<'_>) -> AppResult<String> {
    let ReportOptions {
        candidates,
        ratings,
        speeds,
        move_limit,
        include_cloud,
        force_local,
        engine_depth,
        engine_lines,
    } = options;
    validate_fen(fen)?;
    validate_filters(ratings, speeds, *move_limit)?;
    if *engine_lines == 0 || *engine_lines > 10 {
        return Err("--engine-lines must be between 1 and 10".into());
    }

    let ratings_text = join_display(ratings);
    let speeds_text = speeds.join(",");
    let moves_text = move_limit.to_string();
    let explorer_query = [
        ("variant", "standard"),
        ("fen", fen),
        ("speeds", speeds_text.as_str()),
        ("ratings", ratings_text.as_str()),
        ("moves", moves_text.as_str()),
    ];
    let token = load_lichess_token()?;
    let explorer_result = http::get_optional_text_with_query(
        EXPLORER_URL,
        "application/json",
        &explorer_query,
        token.as_deref(),
    );
    let explorer_text = match explorer_result {
        Ok(Some(text)) => text,
        Ok(None) => return Err("Lichess opening explorer has no data for this position".into()),
        Err(error)
            if error
                .downcast_ref::<ureq::Error>()
                .is_some_and(|error| matches!(error, ureq::Error::StatusCode(401))) =>
        {
            return Err(
                "Lichess opening explorer rejected authorization; provide a valid token through LICHESS_TOKEN or the protected Ferrichess token file"
                    .into(),
            );
        }
        Err(error) => return Err(error),
    };
    let explorer: ExplorerResponse = serde_json::from_str(&explorer_text)?;

    let high_ratings = HIGH_RATINGS;
    let high_elo_query = [
        ("variant", "standard"),
        ("fen", fen),
        ("speeds", speeds_text.as_str()),
        ("ratings", high_ratings),
        ("moves", moves_text.as_str()),
    ];
    let high_elo = fetch_explorer(EXPLORER_URL, &high_elo_query, token.as_deref())?;
    let masters_query = [
        ("fen", fen),
        ("moves", moves_text.as_str()),
        ("topGames", "15"),
    ];
    let masters = fetch_explorer(MASTERS_URL, &masters_query, token.as_deref())?;
    let (selected, missing) = select_candidates(candidates, &explorer, &masters, &high_elo);

    let cloud = if *include_cloud {
        let requested_cloud_lines = u8::try_from(selected.len()).unwrap_or(5).clamp(1, 5);
        let requested_cloud_lines = requested_cloud_lines.to_string();
        let cloud_query = [
            ("fen", fen),
            ("multiPv", requested_cloud_lines.as_str()),
            ("variant", "standard"),
        ];
        http::get_optional_text_with_query(
            CLOUD_EVAL_URL,
            "application/json",
            &cloud_query,
            token.as_deref(),
        )?
        .map(|text| serde_json::from_str::<CloudEvaluation>(&text))
        .transpose()?
    } else {
        None
    };

    let engine_moves: Vec<String> = selected
        .iter()
        .take(10)
        .map(|candidate| candidate.move_data.uci.clone())
        .collect();
    let cloud_complete = cloud.as_ref().is_some_and(|evaluation| {
        engine_moves
            .iter()
            .all(|uci| evaluation_for_move(evaluation, uci).is_some())
    });
    let local_requested = *force_local || (*include_cloud && !cloud_complete);
    let local_engine_moves: Vec<String> = if *force_local || cloud.is_none() {
        engine_moves.clone()
    } else {
        engine_moves
            .iter()
            .filter(|uci| {
                cloud
                    .as_ref()
                    .is_none_or(|evaluation| evaluation_for_move(evaluation, uci).is_none())
            })
            .cloned()
            .collect()
    };
    let local = if local_requested {
        engine::analyse(fen, &local_engine_moves, *engine_depth, *engine_lines)?
    } else {
        None
    };

    Ok(render_report(
        fen,
        &selected,
        &ReportEvidence {
            missing: &missing,
            ratings,
            speeds,
            explorer: &explorer,
            cloud: cloud.as_ref(),
            local: local.as_ref(),
            cloud_requested: *include_cloud,
            local_requested,
        },
    ))
}

pub struct ReportOptions<'a> {
    pub candidates: &'a [String],
    pub ratings: &'a [u16],
    pub speeds: &'a [String],
    pub move_limit: u8,
    pub include_cloud: bool,
    pub force_local: bool,
    pub engine_depth: u8,
    pub engine_lines: u8,
}

fn fetch_explorer(
    url: &str,
    query: &[(&str, &str)],
    token: Option<&str>,
) -> AppResult<ExplorerResponse> {
    let Some(text) = http::get_optional_text_with_query(url, "application/json", query, token)?
    else {
        return Ok(ExplorerResponse {
            white: 0,
            draws: 0,
            black: 0,
            moves: Vec::new(),
            top_games: Vec::new(),
        });
    };
    Ok(serde_json::from_str(&text)?)
}

fn select_candidates(
    requested: &[String],
    practical: &ExplorerResponse,
    masters: &ExplorerResponse,
    high_elo: &ExplorerResponse,
) -> (Vec<CandidateMove>, Vec<String>) {
    let mut selected = Vec::new();
    let mut missing = Vec::new();

    for name in requested {
        let found = [&practical.moves, &masters.moves, &high_elo.moves]
            .into_iter()
            .flat_map(|moves| moves.iter())
            .find(|item| name == &item.uci || name == &item.san);
        if let Some(item) = found {
            push_candidate(&mut selected, item, true, practical, masters, high_elo);
        } else {
            missing.push(name.clone());
        }
    }

    // Consensus strong-player choices come first, in master popularity order.
    // Master-only choices then precede high-Elo-only choices.
    for item in masters.moves.iter().filter(|item| {
        high_elo
            .moves
            .iter()
            .any(|high_elo_move| high_elo_move.uci == item.uci)
    }) {
        if selected.len() >= TARGET_CANDIDATES {
            break;
        }
        push_candidate(&mut selected, item, false, practical, masters, high_elo);
    }
    for source in [&masters.moves, &high_elo.moves] {
        for item in source {
            if selected.len() >= TARGET_CANDIDATES {
                break;
            }
            push_candidate(&mut selected, item, false, practical, masters, high_elo);
        }
    }

    (selected, missing)
}

fn push_candidate(
    selected: &mut Vec<CandidateMove>,
    item: &ExplorerMove,
    requested: bool,
    practical: &ExplorerResponse,
    masters: &ExplorerResponse,
    high_elo: &ExplorerResponse,
) {
    if selected
        .iter()
        .any(|candidate| candidate.move_data.uci == item.uci)
    {
        return;
    }
    let practical_move = practical
        .moves
        .iter()
        .find(|practical_move| practical_move.uci == item.uci)
        .unwrap_or(item);
    let master_game = masters
        .top_games
        .iter()
        .find(|game| game.uci.as_deref() == Some(item.uci.as_str()))
        .cloned()
        .or_else(|| {
            masters
                .moves
                .iter()
                .find(|master_move| master_move.uci == item.uci)
                .and_then(|master_move| master_move.game.clone())
        });
    selected.push(CandidateMove {
        move_data: practical_move.clone(),
        requested,
        masters: masters
            .moves
            .iter()
            .any(|master_move| master_move.uci == item.uci),
        high_elo: high_elo
            .moves
            .iter()
            .any(|high_elo_move| high_elo_move.uci == item.uci),
        master_game,
    });
}

struct ReportEvidence<'a> {
    missing: &'a [String],
    ratings: &'a [u16],
    speeds: &'a [String],
    explorer: &'a ExplorerResponse,
    cloud: Option<&'a CloudEvaluation>,
    local: Option<&'a engine::Evaluation>,
    cloud_requested: bool,
    local_requested: bool,
}

fn render_report(fen: &str, candidates: &[CandidateMove], evidence: &ReportEvidence<'_>) -> String {
    let ReportEvidence {
        missing,
        ratings,
        speeds,
        explorer,
        cloud,
        local,
        cloud_requested,
        local_requested,
    } = evidence;
    let total = explorer.white + explorer.draws + explorer.black;
    let mut report = format!(
        "# Lichess position report\n\nFEN: `{fen}`  \nRatings: `{}`  \nSpeeds: `{}`  \nPosition games: **{total}** ({}/{}/{}, White/Draw/Black)\n\n",
        join_display(ratings),
        speeds.join(","),
        explorer.white,
        explorer.draws,
        explorer.black,
    );
    report.push_str(
        "| Move | Basis | Games | Share | White | Draw | Black | White score | Black score | Cloud | Local SF |\n\
         |---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );

    for candidate in candidates {
        let item = &candidate.move_data;
        let games = item.games();
        let share = percentage(games, total);
        let white_score = score(item.white, item.draws, games);
        let black_score = score(item.black, item.draws, games);
        let evaluation = cloud
            .and_then(|evaluation| evaluation_for_move(evaluation, &item.uci))
            .unwrap_or_else(|| "—".to_owned());
        let local_evaluation = local
            .and_then(|evaluation| engine_evaluation_for_move(evaluation, &item.uci))
            .unwrap_or_else(|| "—".to_owned());
        let move_name = format!("`{}` (`{}`)", item.san, item.uci);
        let move_name = if candidate.requested {
            format!("**{move_name}**")
        } else {
            move_name
        };
        report.push_str(&format!(
            "| {} | {} | {} | {:.1}% | {} | {} | {} | {:.1}% | {:.1}% | {} | {} |\n",
            move_name,
            candidate.basis(),
            games,
            share,
            item.white,
            item.draws,
            item.black,
            white_score,
            black_score,
            evaluation,
            local_evaluation,
        ));
    }

    if !missing.is_empty() {
        report.push_str(&format!(
            "\nNot returned by the practical, master, or high-Elo explorer: {}. This can mean zero games or that the move fell outside the requested move limit.\n",
            missing
                .iter()
                .map(|move_name| format!("`{move_name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let master_games: Vec<_> = candidates
        .iter()
        .filter_map(|candidate| {
            candidate
                .master_game
                .as_ref()
                .map(|game| (candidate.move_data.san.as_str(), game))
        })
        .collect();
    if !master_games.is_empty() {
        report.push_str("\n## Representative master games\n\n");
        for (move_name, game) in master_games {
            let result = match game.winner.as_deref() {
                Some("white") => "1-0",
                Some("black") => "0-1",
                _ => "1/2-1/2",
            };
            report.push_str(&format!(
                "- **{}:** [{} ({}) – {} ({}), {}: {}](https://lichess.org/{})\n",
                move_name,
                game.white.name,
                game.white.rating,
                game.black.name,
                game.black.rating,
                game.year,
                result,
                game.id,
            ));
        }
        report.push_str(
            "\nThese games illustrate how strong players continued from the position; they do not by themselves prove that their first move is the best repertoire choice.\n",
        );
    }

    match (*cloud_requested, cloud) {
        (true, Some(evaluation)) => report.push_str(&format!(
            "\nCloud evaluation: depth {}, {} kN. Positive values favor White and negative values favor Black. Values are cached Lichess evaluations at the root position; only returned MultiPV moves can be matched.\n",
            evaluation.depth, evaluation.knodes
        )),
        (true, None) => report.push_str(
            "\nCloud evaluation: unavailable for this exact position. A local engine check is still required.\n",
        ),
        (false, _) => report.push_str("\nCloud evaluation: skipped.\n"),
    }
    if let Some(evaluation) = local {
        report.push_str(&format!(
            "\nLocal engine: {}, depth {}, {} nodes. Scores use White's point of view.\n",
            evaluation.name, evaluation.depth, evaluation.nodes
        ));
        report.push_str("\nLocal engine lines:\n\n");
        for pv in &evaluation.pvs {
            let score = if let Some(cp) = pv.cp {
                format!("{:+.2}", cp as f64 / 100.0)
            } else if let Some(mate) = pv.mate {
                format!("M{mate:+}")
            } else {
                "—".to_owned()
            };
            report.push_str(&format!("- `{score}`: `{}`\n", pv.moves));
        }
    } else if *local_requested {
        report.push_str("\nLocal engine: `stockfish` was not found.\n");
    }
    report.push_str(
        "\nDatabase results are descriptive, not proof that a move is objectively best. Consider sample size, selection effects, engine soundness, and the resulting plans.\n",
    );
    report
}

fn engine_evaluation_for_move(evaluation: &engine::Evaluation, uci: &str) -> Option<String> {
    let pv = evaluation
        .pvs
        .iter()
        .find(|pv| pv.moves.split_whitespace().next() == Some(uci))?;
    if let Some(cp) = pv.cp {
        Some(format!("{:+.2}", cp as f64 / 100.0))
    } else {
        pv.mate.map(|mate| format!("M{mate:+}"))
    }
}

fn evaluation_for_move(evaluation: &CloudEvaluation, uci: &str) -> Option<String> {
    let pv = evaluation
        .pvs
        .iter()
        .find(|pv| pv.moves.split_whitespace().next() == Some(uci))?;
    if let Some(cp) = pv.cp {
        Some(format!("{:+.2}", cp as f64 / 100.0))
    } else {
        pv.mate.map(|mate| format!("M{mate:+}"))
    }
}

fn validate_fen(fen: &str) -> AppResult<()> {
    let fields: Vec<_> = fen.split_whitespace().collect();
    if fields.len() != 6 || !matches!(fields[1], "w" | "b") {
        return Err("--fen must be a full six-field FEN with side to move".into());
    }
    Ok(())
}

fn validate_filters(ratings: &[u16], speeds: &[String], move_limit: u8) -> AppResult<()> {
    const ALLOWED_SPEEDS: &[&str] = &[
        "ultraBullet",
        "bullet",
        "blitz",
        "rapid",
        "classical",
        "correspondence",
    ];
    if ratings.is_empty() {
        return Err("at least one --ratings group is required".into());
    }
    if speeds.is_empty()
        || speeds
            .iter()
            .any(|speed| !ALLOWED_SPEEDS.contains(&speed.as_str()))
    {
        return Err(format!("--speeds must use only: {}", ALLOWED_SPEEDS.join(",")).into());
    }
    if move_limit == 0 {
        return Err("--moves must be at least 1".into());
    }
    Ok(())
}

fn percentage(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * part as f64 / total as f64
    }
}

fn score(wins: u64, draws: u64, games: u64) -> f64 {
    if games == 0 {
        0.0
    } else {
        100.0 * (wins as f64 + draws as f64 / 2.0) / games as f64
    }
}

fn join_display<T: ToString>(values: &[T]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn load_lichess_token() -> AppResult<Option<String>> {
    if let Ok(token) = env::var("LICHESS_TOKEN") {
        return nonempty_token(token, "LICHESS_TOKEN");
    }

    let Some(path) = default_token_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    read_token_file(&path).map(Some)
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
    let token = fs::read_to_string(path)?;
    nonempty_token(token, &path.display().to_string())?
        .ok_or_else(|| "token unexpectedly missing".into())
}

fn nonempty_token(token: String, source: &str) -> AppResult<Option<String>> {
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err(format!("Lichess token in {source} is empty").into());
    }
    Ok(Some(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explorer() -> ExplorerResponse {
        ExplorerResponse {
            white: 70,
            draws: 10,
            black: 20,
            moves: vec![
                ExplorerMove {
                    uci: "f6d5".to_owned(),
                    san: "Nd5".to_owned(),
                    white: 35,
                    draws: 5,
                    black: 10,
                    game: None,
                },
                ExplorerMove {
                    uci: "f6g4".to_owned(),
                    san: "Ng4".to_owned(),
                    white: 35,
                    draws: 5,
                    black: 10,
                    game: None,
                },
            ],
            top_games: Vec::new(),
        }
    }

    #[test]
    fn renders_candidate_frequency_results_and_matching_cloud_line() {
        let explorer = explorer();
        let (selected, missing) =
            select_candidates(&["Nd5".to_owned()], &explorer, &explorer, &explorer);
        let cloud = CloudEvaluation {
            depth: 28,
            knodes: 1234,
            pvs: vec![CloudPv {
                moves: "f6d5 g5f3".to_owned(),
                cp: Some(23),
                mate: None,
            }],
        };
        let ratings = [1400, 1600];
        let speeds = ["rapid".to_owned()];
        let report = render_report(
            "example w - - 0 1",
            &selected,
            &ReportEvidence {
                missing: &missing,
                ratings: &ratings,
                speeds: &speeds,
                explorer: &explorer,
                cloud: Some(&cloud),
                local: None,
                cloud_requested: true,
                local_requested: false,
            },
        );
        assert!(report.contains("| **`Nd5` (`f6d5`)** | requested + masters + high Elo"));
        assert!(report.contains("| 50 | 50.0% | 35 | 5 | 10 | 75.0% | 25.0% | +0.23 |"));
        assert!(report.contains("| `Ng4` (`f6g4`) | masters + high Elo"));
    }

    #[test]
    fn reports_candidates_missing_from_the_response() {
        let explorer = explorer();
        let (selected, missing) =
            select_candidates(&["Be7".to_owned()], &explorer, &explorer, &explorer);
        let ratings = [1600];
        let speeds = ["classical".to_owned()];
        let report = render_report(
            "example w - - 0 1",
            &selected,
            &ReportEvidence {
                missing: &missing,
                ratings: &ratings,
                speeds: &speeds,
                explorer: &explorer,
                cloud: None,
                local: None,
                cloud_requested: true,
                local_requested: true,
            },
        );
        assert!(
            report.contains("Not returned by the practical, master, or high-Elo explorer: `Be7`")
        );
        assert!(report.contains("local engine check is still required"));
    }

    #[test]
    fn validates_filters_before_calling_the_api() {
        assert!(validate_fen("not a fen").is_err());
        assert!(validate_filters(&[1600], &["rapid".to_owned()], 10).is_ok());
        assert!(validate_filters(&[1600], &["daily".to_owned()], 10).is_err());
        assert!(validate_filters(&[1600], &["rapid".to_owned()], 0).is_err());
    }

    #[test]
    fn parses_the_explorer_and_cloud_api_shapes() {
        let explorer: ExplorerResponse = serde_json::from_str(
            r#"{"white":12,"draws":3,"black":9,"moves":[{"uci":"f6d5","san":"Nd5","averageRating":1660,"white":5,"draws":1,"black":6,"game":{"id":"abcdefgh","winner":"black","white":{"name":"White","rating":2500},"black":{"name":"Black","rating":2510},"year":2024}}],"topGames":[]}"#,
        )
        .expect("explorer response");
        assert_eq!(explorer.moves[0].games(), 12);
        assert_eq!(explorer.moves[0].game.as_ref().unwrap().id, "abcdefgh");

        let cloud: CloudEvaluation = serde_json::from_str(
            r#"{"fen":"ignored","knodes":1000,"depth":30,"pvs":[{"moves":"f6d5 g5f3","cp":18}]}"#,
        )
        .expect("cloud response");
        assert_eq!(
            evaluation_for_move(&cloud, "f6d5").as_deref(),
            Some("+0.18")
        );
    }

    #[test]
    fn rejects_empty_tokens() {
        assert!(nonempty_token("  \n".to_owned(), "test").is_err());
        assert_eq!(
            nonempty_token(" secret\n".to_owned(), "test")
                .expect("valid token")
                .as_deref(),
            Some("secret")
        );
    }
}
