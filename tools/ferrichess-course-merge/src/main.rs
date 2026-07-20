use std::{
    collections::{BTreeSet, HashMap},
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use ferrichess_config::Config;
use ferrichess_pgn_index::{IndexedGame, parse_games};
use ferrichess_study::{
    Annotation, Headers, MoveTree, MoveTreeMerger, PgnDocument, PgnWriter, RepertoireSide, SourceId,
};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position, uci::UciMove};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    course_id: String,
    course_title: String,
    #[serde(default = "default_side")]
    repertoire_side: RepertoireSide,
    groups: Vec<Group>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Group {
    id: String,
    title: String,
    chapters: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    course_id: &'a str,
    source_root: String,
    groups: &'a [Group],
    outputs: Vec<String>,
    conflicts: Vec<ConflictReport>,
}

#[derive(Debug, Serialize)]
struct ConflictReport {
    output: String,
    count: usize,
    details: Vec<String>,
}

fn default_side() -> RepertoireSide {
    RepertoireSide::White
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let source_root = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: ferrichess-course-merge SOURCE_ROOT MANIFEST.json OUTPUT_DIRECTORY")?,
    );
    let manifest_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: ferrichess-course-merge SOURCE_ROOT MANIFEST.json OUTPUT_DIRECTORY")?,
    );
    let output_root = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: ferrichess-course-merge SOURCE_ROOT MANIFEST.json OUTPUT_DIRECTORY")?,
    );
    if arguments.next().is_some() {
        return Err("too many arguments".into());
    }

    let manifest: Manifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
    validate_manifest(&manifest, &source_root)?;
    let config = Config::load_default()?;
    let annotator = config.lichess.username.as_deref();

    let mut documents = Vec::new();
    let mut conflicts = Vec::new();
    for group in &manifest.groups {
        let mut tree = MoveTree::new();
        let mut merger = MoveTreeMerger::new(manifest.repertoire_side);
        let mut group_conflicts = Vec::new();
        for chapter in &group.chapters {
            let path = source_root.join(chapter).join(format!("{chapter}.pgn"));
            let games = parse_games(&fs::read(&path)?)?;
            if games.len() != 1 {
                return Err(format!(
                    "{} contains {} games; repertoire chapters must contain exactly one",
                    path.display(),
                    games.len()
                )
                .into());
            }
            let chapter_tree = tree_from_game(&games[0])?;
            group_conflicts.extend(
                merger
                    .merge(&mut tree, &chapter_tree, SourceId::from(chapter.clone()))?
                    .conflicts,
            );
        }
        if !group_conflicts.is_empty() {
            conflicts.push(ConflictReport {
                output: group.id.clone(),
                count: group_conflicts.len(),
                details: group_conflicts
                    .iter()
                    .map(|conflict| format!("{conflict:?}"))
                    .collect(),
            });
        }
        documents.push(PgnDocument::from_tree(
            aggregate_headers(
                group,
                &manifest.course_title,
                manifest.repertoire_side,
                annotator,
            ),
            tree,
            "*",
        )?);
    }

    fs::create_dir_all(&output_root)?;
    let mut outputs = Vec::new();
    for (group, document) in manifest.groups.iter().zip(&documents) {
        let filename = format!("{}.pgn", group.id);
        fs::write(
            output_root.join(&filename),
            PgnWriter::render_file(document)?,
        )?;
        outputs.push(filename);
    }
    fs::write(
        output_root.join("course.pgn"),
        PgnWriter::render_documents(&documents)?,
    )?;
    outputs.push("course.pgn".to_owned());

    let report = Report {
        course_id: &manifest.course_id,
        source_root: source_root.display().to_string(),
        groups: &manifest.groups,
        outputs,
        conflicts,
    };
    fs::write(
        output_root.join("merge-report.json"),
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    fs::copy(&manifest_path, output_root.join("merge-manifest.json"))?;

    println!(
        "Merged {} source chapters into {} Lichess-friendly chapters in {}",
        manifest
            .groups
            .iter()
            .map(|group| group.chapters.len())
            .sum::<usize>(),
        documents.len(),
        output_root.display()
    );
    Ok(())
}

fn validate_manifest(manifest: &Manifest, source_root: &Path) -> Result<(), Box<dyn Error>> {
    if manifest.groups.is_empty() {
        return Err("manifest must contain at least one group".into());
    }
    let mut ids = BTreeSet::new();
    let mut chapters = BTreeSet::new();
    for group in &manifest.groups {
        if !ids.insert(group.id.as_str()) {
            return Err(format!("duplicate group id {:?}", group.id).into());
        }
        if group.chapters.is_empty() {
            return Err(format!("group {:?} contains no chapters", group.id).into());
        }
        for chapter in &group.chapters {
            if !chapters.insert(chapter.as_str()) {
                return Err(format!("chapter {chapter:?} occurs in more than one group").into());
            }
            let pgn = source_root.join(chapter).join(format!("{chapter}.pgn"));
            if !pgn.is_file() {
                return Err(format!("chapter PGN {} does not exist", pgn.display()).into());
            }
        }
    }
    Ok(())
}

fn tree_from_game(game: &IndexedGame) -> Result<MoveTree, Box<dyn Error>> {
    if game.headers.contains_key("FEN") {
        return Err("nonstandard starting positions are not supported yet".into());
    }
    let mut tree = MoveTree::new();
    let root = tree.root();
    for comment in &game.occurrences[0].comments {
        tree.node_mut(root)
            .expect("root exists")
            .add_comment(comment);
    }
    let mut paths = HashMap::from([(String::new(), (root, Chess::default()))]);
    for occurrence in game.occurrences.iter().skip(1) {
        let (parent_path, _) = occurrence.san_path.rsplit_once(' ').unwrap_or(("", ""));
        let (parent, position) = paths
            .get(parent_path)
            .cloned()
            .ok_or_else(|| format!("missing parent path {parent_path:?}"))?;
        let uci = occurrence
            .incoming_uci
            .as_deref()
            .ok_or("non-root occurrence has no incoming UCI move")?
            .parse::<UciMove>()?;
        let chess_move = uci.to_move(&position)?;
        let child = tree
            .node(parent)
            .and_then(|node| {
                node.children().iter().copied().find(|child| {
                    tree.node(*child).and_then(|node| node.chess_move()) == Some(chess_move)
                })
            })
            .map_or_else(|| tree.add_child(parent, chess_move), Ok)?;
        let node = tree.node_mut(child).expect("new or existing child exists");
        for comment in &occurrence.comments {
            node.add_comment(comment);
        }
        for &nag in &occurrence.nags {
            if let Some(annotation) = annotation_from_nag(nag) {
                node.add_annotation(annotation);
            }
        }
        let mut child_position = position;
        child_position.play_unchecked(chess_move);
        paths.insert(occurrence.san_path.clone(), (child, child_position));
    }
    tree.validate()?;
    Ok(tree)
}

const fn annotation_from_nag(nag: u8) -> Option<Annotation> {
    match nag {
        1 => Some(Annotation::Good),
        2 => Some(Annotation::Mistake),
        3 => Some(Annotation::Brilliant),
        4 => Some(Annotation::Blunder),
        5 => Some(Annotation::Interesting),
        6 => Some(Annotation::Dubious),
        10 => Some(Annotation::Equal),
        12 => Some(Annotation::EqualWithCounterplay),
        13 => Some(Annotation::Unclear),
        14 => Some(Annotation::WhiteSlightAdvantage),
        15 => Some(Annotation::BlackSlightAdvantage),
        16 => Some(Annotation::WhiteAdvantage),
        17 => Some(Annotation::BlackAdvantage),
        18 => Some(Annotation::WhiteWinning),
        19 => Some(Annotation::BlackWinning),
        132 => Some(Annotation::Counterplay),
        _ => None,
    }
}

fn aggregate_headers(
    group: &Group,
    course_title: &str,
    side: RepertoireSide,
    annotator: Option<&str>,
) -> Headers {
    let side = match side {
        RepertoireSide::White => "White",
        RepertoireSide::Black => "Black",
    };
    let mut headers = Headers::new();
    headers.insert("Event", &group.title);
    headers.insert("Site", "?");
    headers.insert("Date", "????.??.??");
    headers.insert("Round", "-");
    headers.insert("White", "Lines");
    headers.insert("Black", course_title);
    headers.insert("Result", "*");
    headers.insert("Chapter", &group.title);
    headers.insert("SourceCourse", course_title);
    headers.insert("Orientation", side);
    headers.insert("RepertoireSide", side);
    headers.insert("RepertoireRole", "Main");
    if let Some(annotator) = annotator {
        headers.insert("Annotator", annotator);
    }
    headers
}
