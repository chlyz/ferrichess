//! Stable JSON diagnostics for differential compatibility checks.

use std::collections::HashMap;

use serde::Serialize;
use shakmaty::{Chess, EnPassantMode, Position, fen::Fen, uci::UciMove};

use crate::{
    Annotation,
    tree::{BuiltRawTree, ClassifiedRawLine, CommentReason, MoveTree, NodeId},
};

#[derive(Serialize)]
struct ParserTrace<'a> {
    lines: Vec<TraceLine<'a>>,
}

#[derive(Serialize)]
struct TraceLine<'a> {
    line_number: usize,
    original: &'a str,
    classification: &'static str,
    reason: Option<&'static str>,
    starting_ply: Option<u32>,
    accepted_moves: Vec<String>,
    annotations: Vec<&'static str>,
    leftover: &'a str,
    normalized_comment: Option<&'a str>,
}

#[derive(Serialize)]
struct CanonicalTree {
    nodes: Vec<CanonicalNode>,
}

#[derive(Serialize)]
struct CanonicalNode {
    id: usize,
    uci: Option<String>,
    children: Vec<usize>,
    comment_fragments: Vec<String>,
    rendered_comment: String,
    annotations: Vec<&'static str>,
    position_key: String,
}

/// Serializes the source-line classifications used to build one raw move tree.
pub fn parser_trace_json(built: &BuiltRawTree) -> Result<String, serde_json::Error> {
    let lines = built
        .lines
        .iter()
        .map(|record| {
            let (classification, reason, starting_ply, moves, leftover, comment) =
                match &record.classification {
                    ClassifiedRawLine::Blank => ("blank", None, None, &[][..], "", None),
                    ClassifiedRawLine::Mainline {
                        starting_ply,
                        moves,
                    } => ("mainline", None, *starting_ply, moves.as_slice(), "", None),
                    ClassifiedRawLine::Comment {
                        text,
                        reason,
                        starting_ply,
                        moves,
                        leftover,
                    } => (
                        "comment",
                        Some(reason_name(*reason)),
                        *starting_ply,
                        moves.as_slice(),
                        leftover.as_str(),
                        Some(text.as_str()),
                    ),
                };
            TraceLine {
                line_number: record.line_number,
                original: &record.original,
                classification,
                reason,
                starting_ply,
                accepted_moves: moves
                    .iter()
                    .map(|item| {
                        UciMove::from_move(item.chess_move, shakmaty::CastlingMode::Standard)
                            .to_string()
                    })
                    .collect(),
                annotations: moves
                    .iter()
                    .map(|item| annotation_name(item.annotation))
                    .collect(),
                leftover,
                normalized_comment: comment,
            }
        })
        .collect();
    serde_json::to_string_pretty(&ParserTrace { lines })
}

/// Serializes a move tree using deterministic pre-order node identifiers.
pub fn canonical_tree_json(tree: &MoveTree) -> Result<String, serde_json::Error> {
    let traversal: Vec<_> = tree.traverse().collect();
    let stable_ids: HashMap<_, _> = traversal
        .iter()
        .copied()
        .enumerate()
        .map(|(stable, arena)| (arena, stable))
        .collect();
    let mut positions = HashMap::new();
    collect_positions(
        tree,
        tree.root(),
        tree.starting_position().clone(),
        &mut positions,
    );

    let nodes = traversal
        .iter()
        .enumerate()
        .map(|(id, &arena_id)| {
            let node = tree
                .node(arena_id)
                .expect("traversal only yields valid nodes");
            let fragments: Vec<_> = node
                .comments()
                .iter()
                .map(|fragment| fragment.as_str().to_owned())
                .collect();
            CanonicalNode {
                id,
                uci: node.chess_move().map(|chess_move| {
                    UciMove::from_move(chess_move, shakmaty::CastlingMode::Standard).to_string()
                }),
                children: node
                    .children()
                    .iter()
                    .map(|child| stable_ids[child])
                    .collect(),
                rendered_comment: fragments.join(" "),
                comment_fragments: fragments,
                annotations: node
                    .annotations()
                    .iter()
                    .copied()
                    .map(|annotation| annotation.suffix())
                    .collect(),
                position_key: position_key(
                    positions
                        .get(&arena_id)
                        .expect("every traversed node has a position"),
                ),
            }
        })
        .collect();
    serde_json::to_string_pretty(&CanonicalTree { nodes })
}

fn collect_positions(
    tree: &MoveTree,
    node_id: NodeId,
    position: Chess,
    output: &mut HashMap<NodeId, Chess>,
) {
    output.insert(node_id, position.clone());
    let node = tree.node(node_id).expect("valid tree traversal");
    for &child_id in node.children() {
        let mut child_position = position.clone();
        let chess_move = tree
            .node(child_id)
            .and_then(crate::tree::Node::chess_move)
            .expect("validated child has a move");
        child_position.play_unchecked(chess_move);
        collect_positions(tree, child_id, child_position, output);
    }
}

fn position_key(position: &Chess) -> String {
    Fen::from_position(position, EnPassantMode::Legal)
        .to_string()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}

const fn reason_name(reason: CommentReason) -> &'static str {
    match reason {
        CommentReason::Prose => "prose",
        CommentReason::InstructionalNullMove => "instructional_null_move",
        CommentReason::BacktrackedMove => "backtracked_move",
        CommentReason::NoLegalMove => "no_legal_move",
        CommentReason::UnparsedTail => "unparsed_tail",
        CommentReason::ExactPly => "exact_ply",
    }
}

fn annotation_name(annotation: Option<Annotation>) -> &'static str {
    annotation.map_or("", Annotation::suffix)
}

#[cfg(test)]
mod tests {
    use crate::{RawTreeBuilder, RepertoireSide};

    use super::{canonical_tree_json, parser_trace_json};

    #[test]
    fn diagnostics_use_stable_uci_moves_and_position_keys() {
        let built = RawTreeBuilder::new(RepertoireSide::White)
            .build("note\n1. e4!e5\n2. Nf3 tail")
            .unwrap();
        let trace = parser_trace_json(&built).unwrap();
        assert!(trace.contains("\"accepted_moves\": [\n        \"g1f3\""));
        assert!(trace.contains("\"reason\": \"unparsed_tail\""));

        let tree = canonical_tree_json(&built.tree).unwrap();
        assert!(tree.contains("\"uci\": \"e2e4\""));
        assert!(tree.contains("rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq -"));
    }
}
