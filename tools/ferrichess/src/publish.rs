use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use ferrichess_config::Config;
use ferrichess_pgn_index::{IndexedGame, Occurrence, parse_games};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{download_study, load_lichess_token, save_study};

type PublishResult<T> = Result<T, Box<dyn Error>>;

const MAX_STUDY_CHAPTERS: usize = 64;

#[derive(Debug, Eq, PartialEq)]
struct ChapterMetadata {
    name: String,
    orientation: String,
    annotator: Option<String>,
    chapter_id: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
struct PublishPlan {
    remote_sha256: String,
    candidate_sha256: String,
    baseline_matches_remote: bool,
    remote_chapters: usize,
    candidate_chapters: usize,
    renamed: usize,
    orientation_changes: usize,
    added: usize,
    removed: usize,
    candidate_matches_remote_suffix: bool,
}

pub fn publish_study(
    config: &Config,
    name: &str,
    candidate_path: &Path,
    replace_all: bool,
    expected_remote_sha256: Option<&str>,
    confirm_study_id: Option<&str>,
) -> PublishResult<()> {
    let study = config
        .studies
        .get(name)
        .ok_or_else(|| format!("unknown configured study {name:?}"))?;
    let baseline_path = study.directory.join("study.pgn");
    let baseline = fs::read_to_string(&baseline_path).map_err(|error| {
        format!(
            "cannot read last-pulled snapshot {}: {error}; pull and review the study first",
            baseline_path.display()
        )
    })?;
    let candidate = fs::read_to_string(candidate_path)?;
    let candidate_documents = split_documents(&candidate)?;
    if candidate_documents.len() > MAX_STUDY_CHAPTERS {
        return Err(format!(
            "candidate has {} chapters; Lichess studies allow at most {MAX_STUDY_CHAPTERS}",
            candidate_documents.len()
        )
        .into());
    }
    if candidate_documents.len() == MAX_STUDY_CHAPTERS {
        return Err(
            "guarded replacement currently supports at most 63 candidate chapters because Lichess automatically creates an empty chapter when the last old chapter is deleted"
                .into(),
        );
    }
    let candidate_games = parse_games(candidate.as_bytes())?;
    validate_documents(&candidate_documents, &candidate_games, false)?;

    let token = load_lichess_token()?;
    let remote = download_study(&study.study_id, &token)?;
    let remote_documents = split_documents(&remote)?;
    let remote_games = parse_games(remote.as_bytes())?;
    validate_documents(&remote_documents, &remote_games, true)?;
    let plan = build_plan(
        &baseline,
        &remote,
        &remote_games,
        &candidate,
        &candidate_games,
    )?;
    print_plan(name, &study.study_id, candidate_path, &plan);

    if !replace_all {
        println!(
            "plan only: Lichess was not modified; replacement requires --replace-all, --expected-remote-sha256, and --confirm-study-id"
        );
        return Ok(());
    }
    if !plan.baseline_matches_remote {
        return Err(
            "live Lichess study differs from the last pulled study.pgn; pull, review, and commit those remote changes before publishing"
                .into(),
        );
    }
    if expected_remote_sha256 != Some(plan.remote_sha256.as_str()) {
        return Err("expected remote SHA-256 does not match the current live study".into());
    }
    if confirm_study_id != Some(study.study_id.as_str()) {
        return Err("--confirm-study-id does not match the configured Lichess study ID".into());
    }
    if semantic_study_eq(&remote_games, &candidate_games)? {
        println!("{name}: candidate already matches Lichess; nothing to replace");
        return Ok(());
    }

    let candidate_matches_remote_suffix = plan.candidate_matches_remote_suffix;

    let state_directory = study.directory.join(".ferrichess-publish");
    fs::create_dir_all(&state_directory)?;
    let backup_path = state_directory.join(format!("remote-{}.pgn", plan.remote_sha256));
    fs::write(&backup_path, &remote)?;
    let journal_path = state_directory.join(format!("replace-{}.journal", plan.remote_sha256));
    fs::write(&journal_path, "backup saved; no remote mutations yet\n")?;

    if candidate_matches_remote_suffix {
        delete_superseded_prefix(
            &study.study_id,
            &token,
            &remote_games,
            remote_games.len() - candidate_games.len(),
            &journal_path,
        )?;
    } else {
        replace_all_chapters(
            &study.study_id,
            &token,
            &remote_games,
            &candidate_documents,
            &candidate_games,
            &journal_path,
        )?;
    }

    let final_remote = download_study(&study.study_id, &token)?;
    let final_games = parse_games(final_remote.as_bytes())?;
    if !semantic_study_eq(&final_games, &candidate_games)? {
        write_journal(&journal_path, "final verification failed; backup retained")?;
        return Err(format!(
            "final Lichess verification failed; recovery backup: {}",
            backup_path.display()
        )
        .into());
    }
    let stats = save_study(&study.directory, &final_remote)?;
    write_journal(
        &journal_path,
        &format!(
            "complete; verified {} chapters and refreshed study.pgn",
            stats.chapters
        ),
    )?;
    println!(
        "{name}: replaced and verified {} chapters; backup: {}",
        stats.chapters,
        backup_path.display()
    );
    Ok(())
}

fn build_plan(
    baseline: &str,
    remote: &str,
    remote_games: &[IndexedGame],
    candidate: &str,
    candidate_games: &[IndexedGame],
) -> PublishResult<PublishPlan> {
    let shared = remote_games.len().min(candidate_games.len());
    let mut renamed = 0;
    let mut orientation_changes = 0;
    for index in 0..shared {
        let remote = metadata(&remote_games[index], true)?;
        let candidate = metadata(&candidate_games[index], false)?;
        renamed += usize::from(remote.name != candidate.name);
        orientation_changes += usize::from(remote.orientation != candidate.orientation);
    }
    let candidate_matches_remote_suffix = remote_games.len() > candidate_games.len()
        && semantic_study_eq(
            &remote_games[remote_games.len() - candidate_games.len()..],
            candidate_games,
        )?;
    Ok(PublishPlan {
        remote_sha256: sha256(remote),
        candidate_sha256: sha256(candidate),
        baseline_matches_remote: baseline == remote,
        remote_chapters: remote_games.len(),
        candidate_chapters: candidate_games.len(),
        renamed,
        orientation_changes,
        added: candidate_games.len().saturating_sub(remote_games.len()),
        removed: remote_games.len().saturating_sub(candidate_games.len()),
        candidate_matches_remote_suffix,
    })
}

fn print_plan(name: &str, study_id: &str, candidate: &Path, plan: &PublishPlan) {
    println!("publish plan for {name} ({study_id})");
    println!("  candidate: {}", candidate.display());
    println!("  remote SHA-256: {}", plan.remote_sha256);
    println!("  candidate SHA-256: {}", plan.candidate_sha256);
    println!(
        "  live matches last pull: {}",
        if plan.baseline_matches_remote {
            "yes"
        } else {
            "NO"
        }
    );
    println!(
        "  chapters: {} remote -> {} candidate",
        plan.remote_chapters, plan.candidate_chapters
    );
    println!(
        "  structural changes: {} renamed, {} orientation, {} added, {} removed",
        plan.renamed, plan.orientation_changes, plan.added, plan.removed
    );
    if plan.candidate_matches_remote_suffix {
        println!(
            "  recovery: candidate is already a verified suffix; apply will delete only the superseded prefix"
        );
    }
}

fn delete_superseded_prefix(
    study_id: &str,
    token: &str,
    remote_games: &[IndexedGame],
    prefix_len: usize,
    journal: &Path,
) -> PublishResult<()> {
    write_journal(
        journal,
        &format!("resuming verified import; deleting {prefix_len} superseded prefix chapters"),
    )?;
    for game in remote_games.iter().take(prefix_len) {
        let chapter = metadata(game, true)?;
        let chapter_id = chapter
            .chapter_id
            .ok_or("remote chapter has no ChapterURL identifier")?;
        delete_chapter(study_id, &chapter_id, token)?;
        write_journal(
            journal,
            &format!("deleted superseded prefix chapter {chapter_id}"),
        )?;
    }
    write_journal(
        journal,
        "all superseded prefix chapters deleted; final verification pending",
    )?;
    Ok(())
}

fn replace_all_chapters(
    study_id: &str,
    token: &str,
    remote_games: &[IndexedGame],
    candidate_documents: &[String],
    candidate_games: &[IndexedGame],
    journal: &Path,
) -> PublishResult<()> {
    let old_ids = remote_games
        .iter()
        .map(|game| {
            metadata(game, true)?
                .chapter_id
                .ok_or_else(|| "remote chapter has no ChapterURL identifier".into())
        })
        .collect::<PublishResult<Vec<_>>>()?;
    let capacity_deletions = old_ids
        .len()
        .saturating_add(candidate_documents.len())
        .saturating_sub(MAX_STUDY_CHAPTERS);
    for chapter_id in old_ids.iter().take(capacity_deletions) {
        delete_chapter(study_id, chapter_id, token)?;
        write_journal(
            journal,
            &format!("deleted old chapter {chapter_id} for capacity"),
        )?;
    }

    let batches = orientation_batches(candidate_documents, candidate_games)?;
    let mut imported = 0;
    for (orientation, pgn) in batches {
        imported += import_chapters(study_id, token, &orientation, &pgn)?;
        write_journal(journal, &format!("imported {imported} candidate chapters"))?;
    }
    if imported != candidate_documents.len() {
        return Err(format!(
            "Lichess reported {imported} imported chapters; expected {}",
            candidate_documents.len()
        )
        .into());
    }

    let intermediate = download_study(study_id, token)?;
    let intermediate_games = parse_games(intermediate.as_bytes())?;
    if intermediate_games.len() < candidate_games.len() {
        return Err(format!(
            "imported candidate chapters failed verification: Lichess has only {} total chapters after importing {} candidates; old chapters retained where possible",
            intermediate_games.len(),
            candidate_games.len()
        )
        .into());
    }
    let imported = &intermediate_games[intermediate_games.len() - candidate_games.len()..];
    if let Some(difference) = semantic_study_difference(imported, candidate_games)? {
        return Err(format!(
            "imported candidate chapters failed verification: {difference}; old chapters retained where possible"
        )
        .into());
    }
    write_journal(journal, "all imported candidate chapters verified")?;

    for chapter_id in old_ids.iter().skip(capacity_deletions) {
        delete_chapter(study_id, chapter_id, token)?;
        write_journal(journal, &format!("deleted superseded chapter {chapter_id}"))?;
    }
    write_journal(
        journal,
        "all superseded chapters deleted; final verification pending",
    )?;
    Ok(())
}

fn orientation_batches(
    documents: &[String],
    games: &[IndexedGame],
) -> PublishResult<Vec<(String, String)>> {
    let mut batches: Vec<(String, String)> = Vec::new();
    for (document, game) in documents.iter().zip(games) {
        let orientation = metadata(game, false)?.orientation;
        if let Some((_, pgn)) = batches
            .last_mut()
            .filter(|(existing, _)| *existing == orientation)
        {
            pgn.push('\n');
            pgn.push_str(document);
        } else {
            batches.push((orientation, document.clone()));
        }
    }
    Ok(batches)
}

fn import_chapters(
    study_id: &str,
    token: &str,
    orientation: &str,
    pgn: &str,
) -> PublishResult<usize> {
    let url = format!("https://lichess.org/api/study/{study_id}/import-pgn");
    let mut response = ureq::post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "ferrichess/0.1 (guarded study publisher)")
        .send_form([("pgn", pgn), ("orientation", orientation)])?;
    let text = response.body_mut().read_to_string()?;
    let value: Value = serde_json::from_str(&text)?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(format!("Lichess import failed: {error}").into());
    }
    value
        .get("chapters")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| "Lichess import response contains no chapters array".into())
}

fn delete_chapter(study_id: &str, chapter_id: &str, token: &str) -> PublishResult<()> {
    let url = format!("https://lichess.org/api/study/{study_id}/{chapter_id}");
    ureq::delete(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "ferrichess/0.1 (guarded study publisher)")
        .call()?;
    Ok(())
}

fn validate_documents(
    documents: &[String],
    games: &[IndexedGame],
    require_chapter_id: bool,
) -> PublishResult<()> {
    if documents.is_empty() || documents.len() != games.len() {
        return Err("PGN must contain one or more separable chapters".into());
    }
    for game in games {
        let chapter = metadata(game, require_chapter_id)?;
        if !matches!(chapter.orientation.as_str(), "white" | "black") {
            return Err(format!(
                "chapter {:?} has invalid or missing Orientation",
                chapter.name
            )
            .into());
        }
    }
    Ok(())
}

fn semantic_study_eq(left: &[IndexedGame], right: &[IndexedGame]) -> PublishResult<bool> {
    Ok(semantic_study_difference(left, right)?.is_none())
}

fn semantic_study_difference(
    left: &[IndexedGame],
    right: &[IndexedGame],
) -> PublishResult<Option<String>> {
    if left.len() != right.len() {
        return Ok(Some(format!(
            "chapter count differs: {} != {}",
            left.len(),
            right.len()
        )));
    }
    for (chapter_index, (left, right)) in left.iter().zip(right).enumerate() {
        let left_metadata = metadata(left, false)?;
        let right_metadata = metadata(right, false)?;
        if left_metadata.name != right_metadata.name {
            return Ok(Some(format!(
                "chapter {} name differs: {:?} != {:?}",
                chapter_index + 1,
                left_metadata.name,
                right_metadata.name
            )));
        }
        if left_metadata.orientation != right_metadata.orientation {
            return Ok(Some(format!(
                "chapter {} orientation differs: {:?} != {:?}",
                chapter_index + 1,
                left_metadata.orientation,
                right_metadata.orientation
            )));
        }
        if left_metadata.annotator != right_metadata.annotator {
            return Ok(Some(format!(
                "chapter {} annotator differs: {:?} != {:?}",
                chapter_index + 1,
                left_metadata.annotator,
                right_metadata.annotator
            )));
        }
        if let Some(difference) = occurrence_difference(&left.occurrences, &right.occurrences) {
            return Ok(Some(format!(
                "chapter {} {:?}: {difference}",
                chapter_index + 1,
                left_metadata.name
            )));
        }
    }
    Ok(None)
}

fn occurrence_difference(left: &[Occurrence], right: &[Occurrence]) -> Option<String> {
    if left.len() != right.len() {
        return Some(format!(
            "position occurrence count differs: {} != {}",
            left.len(),
            right.len()
        ));
    }
    for (index, (left, right)) in left.iter().zip(right).enumerate() {
        macro_rules! compare_field {
            ($field:ident) => {
                if left.$field != right.$field {
                    return Some(format!(
                        "occurrence {} field {} differs: {:?} != {:?}",
                        index,
                        stringify!($field),
                        left.$field,
                        right.$field
                    ));
                }
            };
        }
        compare_field!(fen);
        compare_field!(parent_fen);
        compare_field!(ply);
        compare_field!(san_path);
        compare_field!(uci_path);
        compare_field!(incoming_san);
        compare_field!(incoming_uci);
        let left_comments = normalized_comments(&left.comments);
        let right_comments = normalized_comments(&right.comments);
        if left_comments != right_comments {
            return Some(format!(
                "occurrence {index} comments differ: {left_comments:?} != {right_comments:?}"
            ));
        }
        compare_field!(nags);
    }
    None
}

fn normalized_comments(comments: &[String]) -> (String, Vec<String>) {
    let combined = comments.join(" ");
    let mut remaining = combined.as_str();
    let mut prose = String::new();
    let mut directives = Vec::new();
    while let Some(start) = remaining.find("[%") {
        prose.push_str(&remaining[..start]);
        let Some(relative_end) = remaining[start..].find(']') else {
            prose.push_str(&remaining[start..]);
            remaining = "";
            break;
        };
        let end = start + relative_end + 1;
        directives.push(normalize_whitespace(&remaining[start..end]));
        prose.push(' ');
        remaining = &remaining[end..];
    }
    prose.push_str(remaining);
    directives.sort();
    directives.dedup();
    (normalize_whitespace(&prose), directives)
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn metadata(game: &IndexedGame, require_chapter_id: bool) -> PublishResult<ChapterMetadata> {
    let name = game
        .headers
        .get("ChapterName")
        .or_else(|| game.headers.get("Event"))
        .filter(|name| !name.trim().is_empty())
        .ok_or("chapter has no ChapterName or Event")?
        .trim()
        .to_owned();
    let orientation = game
        .headers
        .get("Orientation")
        .map(|side| side.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let annotator = game.headers.get("Annotator").map(|value| value.to_owned());
    let chapter_id = game
        .headers
        .get("ChapterURL")
        .and_then(|url| url.rsplit('/').next())
        .filter(|id| id.len() == 8 && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .map(str::to_owned);
    if require_chapter_id && chapter_id.is_none() {
        return Err(format!("remote chapter {name:?} has no valid ChapterURL").into());
    }
    Ok(ChapterMetadata {
        name,
        orientation,
        annotator,
        chapter_id,
    })
}

fn split_documents(pgn: &str) -> PublishResult<Vec<String>> {
    if !pgn.starts_with("[Event ") {
        return Err("candidate and exported PGNs must begin each chapter with an Event tag".into());
    }
    let mut starts = vec![0];
    starts.extend(pgn.match_indices("\n[Event ").map(|(index, _)| index + 1));
    let mut documents = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().copied().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(pgn.len());
        documents.push(format!("{}\n", pgn[start..end].trim_end()));
    }
    Ok(documents)
}

fn sha256(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_journal(path: &Path, state: &str) -> PublishResult<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{state}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REMOTE: &str = concat!(
        "[Event \"One\"]\n",
        "[ChapterName \"One\"]\n",
        "[ChapterURL \"https://lichess.org/study/abcdefgh/12345678\"]\n",
        "[Orientation \"white\"]\n",
        "[Annotator \"https://lichess.org/@/owner\"]\n\n",
        "1. e4 e5 *\n\n",
        "[Event \"Two\"]\n",
        "[ChapterName \"Two\"]\n",
        "[ChapterURL \"https://lichess.org/study/abcdefgh/abcdefgh\"]\n",
        "[Orientation \"black\"]\n",
        "[Annotator \"https://lichess.org/@/owner\"]\n\n",
        "1. d4 d5 *\n",
    );

    const CANDIDATE: &str = concat!(
        "[Event \"One renamed\"]\n",
        "[ChapterName \"One renamed\"]\n",
        "[Orientation \"White\"]\n",
        "[Annotator \"https://lichess.org/@/owner\"]\n\n",
        "1. e4 e5 *\n",
    );

    #[test]
    fn splits_multi_chapter_pgn_without_losing_content() {
        let documents = split_documents(REMOTE).unwrap();
        assert_eq!(documents.len(), 2);
        assert!(documents[0].contains("1. e4 e5 *"));
        assert!(documents[1].contains("1. d4 d5 *"));
    }

    #[test]
    fn plan_detects_drift_renames_and_removals() {
        let remote_games = parse_games(REMOTE.as_bytes()).unwrap();
        let candidate_games = parse_games(CANDIDATE.as_bytes()).unwrap();
        let plan = build_plan(
            "different baseline",
            REMOTE,
            &remote_games,
            CANDIDATE,
            &candidate_games,
        )
        .unwrap();
        assert!(!plan.baseline_matches_remote);
        assert_eq!(plan.renamed, 1);
        assert_eq!(plan.removed, 1);
        assert_eq!(plan.added, 0);
    }

    #[test]
    fn rename_on_lichess_after_the_last_pull_is_remote_drift() {
        let live = REMOTE.replacen(
            "[ChapterName \"One\"]",
            "[ChapterName \"Renamed on Lichess\"]",
            1,
        );
        let live_games = parse_games(live.as_bytes()).unwrap();
        let candidate_games = parse_games(REMOTE.as_bytes()).unwrap();
        let plan = build_plan(REMOTE, &live, &live_games, REMOTE, &candidate_games).unwrap();

        assert!(!plan.baseline_matches_remote);
        assert_eq!(plan.renamed, 1);
    }

    #[test]
    fn plan_recognizes_a_verified_candidate_suffix_after_an_interrupted_import() {
        let remote = format!("{REMOTE}\n{CANDIDATE}");
        let remote_games = parse_games(remote.as_bytes()).unwrap();
        let candidate_games = parse_games(CANDIDATE.as_bytes()).unwrap();
        let plan =
            build_plan(&remote, &remote, &remote_games, CANDIDATE, &candidate_games).unwrap();

        assert!(plan.candidate_matches_remote_suffix);
    }

    #[test]
    fn fingerprint_is_sha256() {
        assert_eq!(
            sha256("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn orientation_batches_preserve_order() {
        let documents = split_documents(REMOTE).unwrap();
        let games = parse_games(REMOTE.as_bytes()).unwrap();
        let batches = orientation_batches(&documents, &games).unwrap();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].0, "white");
        assert_eq!(batches[1].0, "black");
    }

    #[test]
    fn semantic_comparison_ignores_lichess_comment_fragment_merging() {
        let separate = parse_games(
            b"[Event \"One\"]\n[Orientation \"White\"]\n\n1. e4 { prose } { [%cal Ge2e4] } *\n",
        )
        .unwrap();
        let merged = parse_games(
            b"[Event \"One\"]\n[Orientation \"white\"]\n\n1. e4 { [%cal Ge2e4] prose } *\n",
        )
        .unwrap();

        assert!(semantic_study_eq(&separate, &merged).unwrap());
    }
}
