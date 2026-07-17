use std::{collections::HashMap, error::Error, fmt};

use shakmaty::{Chess, Move, Position};

use crate::domain::{PositionKey, RepertoireSide, SourceId};

use super::{MoveTree, NodeId, TreeError};

/// One repertoire-side move rejected while merging a source tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeConflict {
    pub position: PositionKey,
    pub selected_move: Move,
    pub selected_source: SourceId,
    pub rejected_move: Move,
    pub rejected_source: SourceId,
}

/// Observable results from merging one source tree.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MergeReport {
    pub added_nodes: usize,
    pub merged_nodes: usize,
    pub conflicts: Vec<MergeConflict>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectedMove {
    chess_move: Move,
    source: SourceId,
}

/// Stateful, deterministic policy for merging raw lines into one repertoire tree.
///
/// A session remembers which source first selected the repertoire move at each
/// position. Opponent alternatives remain ordered branches. A later source's
/// main variation is promoted when it is accepted, matching the current Python
/// ordering behavior.
#[derive(Clone, Debug)]
pub struct MoveTreeMerger {
    repertoire_side: RepertoireSide,
    selected_moves: HashMap<PositionKey, SelectedMove>,
    has_merged_source: bool,
}

impl MoveTreeMerger {
    #[must_use]
    pub fn new(repertoire_side: RepertoireSide) -> Self {
        Self {
            repertoire_side,
            selected_moves: HashMap::new(),
            has_merged_source: false,
        }
    }

    /// Merges a source tree into `target`, returning all policy conflicts.
    ///
    /// The first call requires an empty target so every selected move has known
    /// provenance. Subsequent calls must use the same target and session.
    pub fn merge(
        &mut self,
        target: &mut MoveTree,
        source: &MoveTree,
        source_id: impl Into<SourceId>,
    ) -> Result<MergeReport, MergeError> {
        if PositionKey::from_position(target.starting_position())
            != PositionKey::from_position(source.starting_position())
        {
            return Err(MergeError::StartingPositionMismatch);
        }
        if !self.has_merged_source && target.len() != 1 {
            return Err(MergeError::TargetHasUntrackedMoves);
        }

        let source_id = source_id.into();
        let mut report = MergeReport::default();
        self.merge_node(
            target,
            target.root(),
            source,
            source.root(),
            target.starting_position().clone(),
            &source_id,
            &mut report,
        )?;
        target.validate()?;
        self.has_merged_source = true;
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    fn merge_node(
        &mut self,
        target: &mut MoveTree,
        target_parent: NodeId,
        source: &MoveTree,
        source_parent: NodeId,
        position: Chess,
        source_id: &SourceId,
        report: &mut MergeReport,
    ) -> Result<(), MergeError> {
        let source_children = source
            .node(source_parent)
            .ok_or(TreeError::UnknownNode(source_parent))?
            .children()
            .to_vec();

        for (index, source_child) in source_children.into_iter().enumerate() {
            let chess_move = source
                .node(source_child)
                .ok_or(TreeError::UnknownNode(source_child))?
                .chess_move()
                .ok_or(TreeError::MissingMove(source_child))?;
            if !position.is_legal(chess_move) {
                return Err(MergeError::IllegalMove {
                    source: source_id.clone(),
                    node: source_child,
                });
            }

            if self.repertoire_side.is_repertoire_turn(position.turn()) {
                let key = PositionKey::from_position(&position);
                if let Some(selected) = self.selected_moves.get(&key) {
                    if selected.chess_move != chess_move {
                        report.conflicts.push(MergeConflict {
                            position: key,
                            selected_move: selected.chess_move,
                            selected_source: selected.source.clone(),
                            rejected_move: chess_move,
                            rejected_source: source_id.clone(),
                        });
                        continue;
                    }
                } else {
                    self.selected_moves.insert(
                        key,
                        SelectedMove {
                            chess_move,
                            source: source_id.clone(),
                        },
                    );
                }
            }

            let existing = find_child(target, target_parent, chess_move);
            let target_child = if let Some(child) = existing {
                report.merged_nodes += 1;
                child
            } else {
                report.added_nodes += 1;
                target.add_child(target_parent, chess_move)?
            };
            merge_node_content(target, target_child, source, source_child)?;
            if index == 0 {
                target.promote_child(target_parent, target_child)?;
            }

            let mut child_position = position.clone();
            child_position.play_unchecked(chess_move);
            self.merge_node(
                target,
                target_child,
                source,
                source_child,
                child_position,
                source_id,
                report,
            )?;
        }
        Ok(())
    }
}

fn find_child(tree: &MoveTree, parent: NodeId, chess_move: Move) -> Option<NodeId> {
    tree.node(parent)?
        .children()
        .iter()
        .copied()
        .find(|&child| tree.node(child).and_then(super::Node::chess_move) == Some(chess_move))
}

fn merge_node_content(
    target: &mut MoveTree,
    target_node: NodeId,
    source: &MoveTree,
    source_node: NodeId,
) -> Result<(), TreeError> {
    let source_node = source
        .node(source_node)
        .ok_or(TreeError::UnknownNode(source_node))?;
    let source_comment = rendered_comment(source_node);
    let target_comment = target
        .node(target_node)
        .ok_or(TreeError::UnknownNode(target_node))
        .map(rendered_comment)?;
    let complete_comment_already_present =
        !source_comment.is_empty() && source_comment == target_comment;
    let target_node = target
        .node_mut(target_node)
        .ok_or(TreeError::UnknownNode(target_node))?;
    if !complete_comment_already_present {
        for comment in source_node.comments() {
            target_node.add_comment(comment.clone());
        }
    }
    for &annotation in source_node.annotations() {
        target_node.add_annotation(annotation);
    }
    Ok(())
}

fn rendered_comment(node: &super::Node) -> String {
    node.comments()
        .iter()
        .map(super::CommentFragment::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergeError {
    StartingPositionMismatch,
    TargetHasUntrackedMoves,
    IllegalMove { source: SourceId, node: NodeId },
    Tree(TreeError),
}

impl fmt::Display for MergeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StartingPositionMismatch => {
                formatter.write_str("source and target starting positions differ")
            }
            Self::TargetHasUntrackedMoves => formatter.write_str(
                "the first merge target contains moves whose source provenance is unknown",
            ),
            Self::IllegalMove { source, node } => write!(
                formatter,
                "source {} contains an illegal move at node {}",
                source.as_path().display(),
                node.index()
            ),
            Self::Tree(error) => error.fmt(formatter),
        }
    }
}

impl Error for MergeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Tree(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TreeError> for MergeError {
    fn from(error: TreeError) -> Self {
        Self::Tree(error)
    }
}

#[cfg(test)]
mod tests {
    use shakmaty::{Chess, Position, san::SanPlus};

    use crate::{
        domain::{Annotation, RepertoireSide},
        tree::{MoveTree, RawTreeBuilder},
    };

    use super::{MergeError, MoveTreeMerger};

    fn build(side: RepertoireSide, text: &str) -> MoveTree {
        RawTreeBuilder::new(side).build(text).unwrap().tree
    }

    fn child_sans(tree: &MoveTree, node: crate::tree::NodeId, position: &Chess) -> Vec<String> {
        tree.node(node)
            .unwrap()
            .children()
            .iter()
            .map(|&child| {
                SanPlus::from_move(
                    position.clone(),
                    tree.node(child).unwrap().chess_move().unwrap(),
                )
                .to_string()
            })
            .collect()
    }

    fn node_after(tree: &MoveTree, sans: &[&str]) -> (crate::tree::NodeId, Chess) {
        let mut node = tree.root();
        let mut position = Chess::default();
        for expected in sans {
            let child = tree
                .node(node)
                .unwrap()
                .children()
                .iter()
                .copied()
                .find(|&child| {
                    SanPlus::from_move(
                        position.clone(),
                        tree.node(child).unwrap().chess_move().unwrap(),
                    )
                    .to_string()
                        == *expected
                })
                .unwrap();
            position.play_unchecked(tree.node(child).unwrap().chess_move().unwrap());
            node = child;
        }
        (node, position)
    }

    #[test]
    fn merges_opponent_branches_and_promotes_the_later_mainline() {
        let first = build(RepertoireSide::White, "1. e4c52. Nf3");
        let second = build(RepertoireSide::White, "1. e4e52. Nf3");
        let mut target = MoveTree::new();
        let mut merger = MoveTreeMerger::new(RepertoireSide::White);

        merger.merge(&mut target, &first, "001.raw").unwrap();
        merger.merge(&mut target, &second, "002.raw").unwrap();

        let (e4, position) = node_after(&target, &["e4"]);
        assert_eq!(child_sans(&target, e4, &position), ["e5", "c5"]);
    }

    #[test]
    fn reports_and_rejects_a_conflicting_repertoire_move() {
        let first = build(RepertoireSide::Black, "1. e4e52. Nf3Nc6");
        let second = build(RepertoireSide::Black, "1. e4c52. Nf3d6");
        let mut target = MoveTree::new();
        let mut merger = MoveTreeMerger::new(RepertoireSide::Black);

        merger.merge(&mut target, &first, "001.raw").unwrap();
        let report = merger.merge(&mut target, &second, "002.raw").unwrap();

        assert_eq!(report.conflicts.len(), 1);
        let conflict = &report.conflicts[0];
        assert_eq!(conflict.selected_source.as_path().to_str(), Some("001.raw"));
        assert_eq!(conflict.rejected_source.as_path().to_str(), Some("002.raw"));
        let (e4, position) = node_after(&target, &["e4"]);
        assert_eq!(child_sans(&target, e4, &position), ["e5"]);
    }

    #[test]
    fn merges_comments_and_annotations_on_shared_nodes() {
        let first = build(RepertoireSide::White, "1. e4!e5\nshared\n2. Nf3");
        let second = build(RepertoireSide::White, "1. e4?!e5\nshared\nextra\n2. Nf3");
        let mut target = MoveTree::new();
        let mut merger = MoveTreeMerger::new(RepertoireSide::White);

        merger.merge(&mut target, &first, "001.raw").unwrap();
        merger.merge(&mut target, &second, "002.raw").unwrap();

        let (e4, _) = node_after(&target, &["e4"]);
        assert_eq!(
            target.node(e4).unwrap().annotations(),
            &[Annotation::Good, Annotation::Dubious]
                .into_iter()
                .collect()
        );
        let (e5, _) = node_after(&target, &["e4", "e5"]);
        let comments: Vec<_> = target
            .node(e5)
            .unwrap()
            .comments()
            .iter()
            .map(|comment| comment.as_str())
            .collect();
        assert_eq!(comments, ["shared", "extra"]);
    }

    #[test]
    fn deduplicates_the_same_comment_with_different_fragment_boundaries() {
        let first = build(
            RepertoireSide::White,
            "1. e4e5\nAlpha. Beta.\nGamma.\n2. Nf3",
        );
        let second = build(
            RepertoireSide::White,
            "1. e4e5\nAlpha.\nBeta. Gamma.\n2. Bc4",
        );
        let mut target = MoveTree::new();
        let mut merger = MoveTreeMerger::new(RepertoireSide::White);

        merger.merge(&mut target, &first, "001.raw").unwrap();
        merger.merge(&mut target, &second, "002.raw").unwrap();

        let (e5, _) = node_after(&target, &["e4", "e5"]);
        let comments: Vec<_> = target
            .node(e5)
            .unwrap()
            .comments()
            .iter()
            .map(|comment| comment.as_str())
            .collect();
        assert_eq!(comments, ["Alpha. Beta.", "Gamma."]);
    }

    #[test]
    fn rejects_an_initial_target_with_unknown_provenance() {
        let source = build(RepertoireSide::White, "1. e4");
        let mut target = build(RepertoireSide::White, "1. d4");
        let mut merger = MoveTreeMerger::new(RepertoireSide::White);

        assert_eq!(
            merger.merge(&mut target, &source, "001.raw"),
            Err(MergeError::TargetHasUntrackedMoves)
        );
    }
}
