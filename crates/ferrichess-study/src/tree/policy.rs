use std::collections::HashMap;

use shakmaty::{Chess, Move, Position};

use crate::{
    domain::{Annotation, PositionKey, RepertoireSide},
    raw::EmbeddedMove,
};

use super::{MoveTree, NodeId};

/// Why a parsed comment variation was accepted or rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentVariationDecision {
    Accepted,
    BadRepertoireAnnotation,
    ConflictsWithExplicitRepertoireMove,
    ConflictsWithExistingRepertoireMove,
}

pub(crate) fn decide_comment_variation(
    tree: &MoveTree,
    anchor: NodeId,
    position: &Chess,
    moves: &[EmbeddedMove],
    repertoire_side: RepertoireSide,
    repertoire_moves: &HashMap<PositionKey, Move>,
) -> CommentVariationDecision {
    let mut position = position.clone();
    let mut target = Some(anchor);
    for item in moves {
        let existing = target.and_then(|node| find_child(tree, node, item.chess_move));
        if repertoire_side.is_repertoire_turn(position.turn()) {
            if item.annotation.is_some_and(is_bad_repertoire_annotation) {
                return CommentVariationDecision::BadRepertoireAnnotation;
            }
            if repertoire_moves
                .get(&PositionKey::from_position(&position))
                .is_some_and(|selected| *selected != item.chess_move)
            {
                return CommentVariationDecision::ConflictsWithExplicitRepertoireMove;
            }
            if target
                .and_then(|node| tree.node(node))
                .is_some_and(|node| !node.children().is_empty())
                && existing.is_none()
            {
                return CommentVariationDecision::ConflictsWithExistingRepertoireMove;
            }
        }
        position.play_unchecked(item.chess_move);
        target = existing;
    }
    CommentVariationDecision::Accepted
}

fn find_child(tree: &MoveTree, parent: NodeId, chess_move: Move) -> Option<NodeId> {
    tree.node(parent)?
        .children()
        .iter()
        .copied()
        .find(|&child| tree.node(child).and_then(super::Node::chess_move) == Some(chess_move))
}

const fn is_bad_repertoire_annotation(annotation: Annotation) -> bool {
    matches!(
        annotation,
        Annotation::Mistake | Annotation::Blunder | Annotation::Dubious
    )
}
