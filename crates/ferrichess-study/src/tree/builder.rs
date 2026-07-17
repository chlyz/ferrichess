use std::collections::HashMap;

use shakmaty::{Chess, Move, Position};

use crate::{
    domain::{DepthLimit, PositionKey, RepertoireSide},
    raw::{EmbeddedMove, ParsedMove, RawParser},
};

use super::{
    CommentVariationDecision, MoveTree, NodeId, TreeError, policy::decide_comment_variation,
};

/// Why a nonblank source line was retained as a comment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentReason {
    /// The line did not begin with an explicit move number.
    Prose,
    /// The line uses `--` as an instructional placeholder for an irrelevant move.
    InstructionalNullMove,
    /// The line's explicit move number precedes the current mainline position.
    BacktrackedMove,
    /// No legal move could be read at the current position.
    NoLegalMove,
    /// Legal moves were followed by text that could not be parsed as moves.
    UnparsedTail,
    /// The line explicitly attaches a comment to an exact mainline ply.
    ExactPly,
}

/// The position-aware classification of one raw source line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassifiedRawLine {
    Blank,
    Mainline {
        starting_ply: Option<u32>,
        moves: Vec<ParsedMove>,
    },
    Comment {
        text: String,
        reason: CommentReason,
        starting_ply: Option<u32>,
        moves: Vec<ParsedMove>,
        leftover: String,
    },
}

/// A classified source line with its one-based line number.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawLineRecord {
    pub line_number: usize,
    pub original: String,
    pub classification: ClassifiedRawLine,
}

/// A move tree and the trace used to construct it from raw text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltRawTree {
    pub tree: MoveTree,
    pub lines: Vec<RawLineRecord>,
}

/// Builds one mainline move tree from raw repertoire text.
#[derive(Clone, Debug)]
pub struct RawTreeBuilder {
    parser: RawParser,
    repertoire_side: RepertoireSide,
    depth_limit: DepthLimit,
    comment_variations: bool,
}

impl RawTreeBuilder {
    #[must_use]
    pub fn new(repertoire_side: RepertoireSide) -> Self {
        Self {
            parser: RawParser::new(),
            repertoire_side,
            depth_limit: DepthLimit::Unlimited,
            comment_variations: false,
        }
    }

    #[must_use]
    pub const fn with_depth_limit(mut self, depth_limit: DepthLimit) -> Self {
        self.depth_limit = depth_limit;
        self
    }

    /// Enables compatible numbered move sequences in comments as variations.
    #[must_use]
    pub const fn with_comment_variations(mut self, enabled: bool) -> Self {
        self.comment_variations = enabled;
        self
    }

    /// Parses raw text without performing any filesystem or rendering work.
    pub fn build(&self, text: &str) -> Result<BuiltRawTree, TreeError> {
        let source_lines: Vec<&str> = text.lines().collect();
        let (final_clean_ply, repertoire_moves) = self.scan_clean_mainline(&source_lines);
        let mut tree = MoveTree::new();
        let mut position = Chess::default();
        let mut positions_by_ply = vec![position.clone()];
        let mut nodes_by_ply = vec![tree.root()];
        let mut current_node = Some(tree.root());
        let mut records = Vec::new();
        let mut exact_ply_comments = Vec::new();
        let maximum_ply = self.maximum_ply();

        for (line_index, original) in source_lines.iter().copied().enumerate() {
            let stripped = original.trim();
            let classification = if stripped.is_empty() {
                ClassifiedRawLine::Blank
            } else if let Some((ply, text)) = parse_ply_comment_directive(stripped) {
                exact_ply_comments.push((line_index + 1, ply, text.to_owned()));
                ClassifiedRawLine::Comment {
                    text: text.to_owned(),
                    reason: CommentReason::ExactPly,
                    starting_ply: Some(ply),
                    moves: Vec::new(),
                    leftover: String::new(),
                }
            } else if self.parser.has_instructional_null_move(stripped) {
                let normalized = self.parser.normalize_comment_text(
                    stripped,
                    Some(&position),
                    Some(&positions_by_ply),
                );
                self.attach_comment(&mut tree, current_node, &normalized)?;
                ClassifiedRawLine::Comment {
                    text: normalized,
                    reason: CommentReason::InstructionalNullMove,
                    starting_ply: self.parser.starting_ply_for_move_line(stripped),
                    moves: Vec::new(),
                    leftover: String::new(),
                }
            } else if !self.parser.is_move_line(stripped) {
                let normalized = self.parser.normalize_comment_text(
                    stripped,
                    Some(&position),
                    Some(&positions_by_ply),
                );
                self.attach_comment(&mut tree, current_node, &normalized)?;
                self.maybe_add_comment_variation(
                    &mut tree,
                    &nodes_by_ply,
                    &positions_by_ply,
                    current_node,
                    stripped,
                    &source_lines,
                    line_index,
                    &position,
                    final_clean_ply,
                    maximum_ply,
                    &repertoire_moves,
                )?;
                ClassifiedRawLine::Comment {
                    text: normalized,
                    reason: CommentReason::Prose,
                    starting_ply: None,
                    moves: Vec::new(),
                    leftover: String::new(),
                }
            } else {
                let starting_ply = self.parser.starting_ply_for_move_line(stripped);
                if starting_ply.is_some_and(|ply| ply < position_ply(&position)) {
                    let normalized = self.parser.normalize_comment_text(
                        stripped,
                        Some(&position),
                        Some(&positions_by_ply),
                    );
                    self.attach_comment(&mut tree, current_node, &normalized)?;
                    self.maybe_add_comment_variation(
                        &mut tree,
                        &nodes_by_ply,
                        &positions_by_ply,
                        current_node,
                        stripped,
                        &source_lines,
                        line_index,
                        &position,
                        final_clean_ply,
                        maximum_ply,
                        &repertoire_moves,
                    )?;
                    ClassifiedRawLine::Comment {
                        text: normalized,
                        reason: CommentReason::BacktrackedMove,
                        starting_ply,
                        moves: Vec::new(),
                        leftover: String::new(),
                    }
                } else {
                    let parsed = self.parser.parse_move_line(stripped, &position);
                    if parsed.moves.is_empty() || !parsed.leftover.is_empty() {
                        let reason = if parsed.moves.is_empty() {
                            CommentReason::NoLegalMove
                        } else {
                            CommentReason::UnparsedTail
                        };
                        let normalized = self.parser.normalize_comment_text(
                            stripped,
                            Some(&position),
                            Some(&positions_by_ply),
                        );
                        self.attach_comment(&mut tree, current_node, &normalized)?;
                        self.maybe_add_comment_variation(
                            &mut tree,
                            &nodes_by_ply,
                            &positions_by_ply,
                            current_node,
                            stripped,
                            &source_lines,
                            line_index,
                            &position,
                            final_clean_ply,
                            maximum_ply,
                            &repertoire_moves,
                        )?;
                        ClassifiedRawLine::Comment {
                            text: normalized,
                            reason,
                            starting_ply,
                            moves: parsed.moves,
                            leftover: parsed.leftover,
                        }
                    } else {
                        for parsed_move in &parsed.moves {
                            let next_ply = position_ply(&position) + 1;
                            if maximum_ply.is_none_or(|limit| next_ply <= limit) {
                                let parent = current_node.expect(
                                    "an included move cannot follow an excluded mainline move",
                                );
                                let child = find_child(&tree, parent, parsed_move.chess_move)
                                    .map_or_else(
                                        || tree.add_child(parent, parsed_move.chess_move),
                                        Ok,
                                    )?;
                                tree.promote_child(parent, child)?;
                                if let Some(annotation) = parsed_move.annotation {
                                    tree.node_mut(child)
                                        .ok_or(TreeError::UnknownNode(child))?
                                        .add_annotation(annotation);
                                }
                                current_node = Some(child);
                            } else {
                                current_node = None;
                            }

                            position.play_unchecked(parsed_move.chess_move);
                            positions_by_ply.push(position.clone());
                            if let Some(node) = current_node {
                                nodes_by_ply.push(node);
                            }
                        }
                        ClassifiedRawLine::Mainline {
                            starting_ply,
                            moves: parsed.moves,
                        }
                    }
                }
            };

            records.push(RawLineRecord {
                line_number: line_index + 1,
                original: original.to_owned(),
                classification,
            });
        }

        for (line_number, ply, text) in exact_ply_comments {
            if let Some(&node) = nodes_by_ply.get(ply as usize) {
                self.attach_comment(&mut tree, Some(node), &text)?;
            } else if maximum_ply.is_none_or(|limit| ply <= limit) {
                return Err(TreeError::PlyCommentOutOfBounds {
                    line_number,
                    ply,
                    available_plies: nodes_by_ply.len().saturating_sub(1) as u32,
                });
            }
        }

        tree.validate()?;
        Ok(BuiltRawTree {
            tree,
            lines: records,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn maybe_add_comment_variation(
        &self,
        tree: &mut MoveTree,
        nodes_by_ply: &[NodeId],
        positions_by_ply: &[Chess],
        current_node: Option<NodeId>,
        text: &str,
        source_lines: &[&str],
        line_index: usize,
        current_position: &Chess,
        final_clean_ply: u32,
        maximum_ply: Option<u32>,
        repertoire_moves: &HashMap<PositionKey, Move>,
    ) -> Result<(), TreeError> {
        if !self.comment_variations
            || current_node.is_none()
            || !self.has_later_clean_move_line(&source_lines[line_index + 1..], current_position)
        {
            return Ok(());
        }
        let Some(start_ply) = self.parser.first_numbered_ply(text) else {
            return Ok(());
        };
        let (anchor, anchor_position) = if let (Some(&node), Some(position)) = (
            nodes_by_ply.get(start_ply as usize),
            positions_by_ply.get(start_ply as usize),
        ) {
            (node, position.clone())
        } else if start_ply == position_ply(current_position) {
            (
                current_node.expect("checked above"),
                current_position.clone(),
            )
        } else {
            return Ok(());
        };
        let moves = self
            .parser
            .parse_embedded_move_sequences(text, &anchor_position);
        let variation_end = start_ply.saturating_add(moves.len() as u32);
        if moves.is_empty()
            || variation_end > final_clean_ply
            || maximum_ply.is_some_and(|limit| variation_end > limit)
        {
            return Ok(());
        }
        self.add_variation_line(tree, anchor, anchor_position, &moves, repertoire_moves)
    }

    fn add_variation_line(
        &self,
        tree: &mut MoveTree,
        anchor: NodeId,
        position: Chess,
        moves: &[EmbeddedMove],
        repertoire_moves: &HashMap<PositionKey, Move>,
    ) -> Result<(), TreeError> {
        if decide_comment_variation(
            tree,
            anchor,
            &position,
            moves,
            self.repertoire_side,
            repertoire_moves,
        ) != CommentVariationDecision::Accepted
        {
            return Ok(());
        }
        let mut parent = anchor;
        for item in moves {
            let child = match find_child(tree, parent, item.chess_move) {
                Some(child) => child,
                None => {
                    let child = tree.add_child(parent, item.chess_move)?;
                    if let Some(annotation) = item.annotation {
                        tree.node_mut(child)
                            .ok_or(TreeError::UnknownNode(child))?
                            .add_annotation(annotation);
                    }
                    child
                }
            };
            parent = child;
        }
        Ok(())
    }

    fn scan_clean_mainline(&self, lines: &[&str]) -> (u32, HashMap<PositionKey, Move>) {
        let mut position = Chess::default();
        let mut choices = HashMap::new();
        for line in lines {
            let stripped = line.trim();
            if !self.parser.is_move_line(stripped)
                || self
                    .parser
                    .starting_ply_for_move_line(stripped)
                    .is_some_and(|ply| ply < position_ply(&position))
            {
                continue;
            }
            let parsed = self.parser.parse_move_line(stripped, &position);
            if parsed.moves.is_empty() || !parsed.leftover.is_empty() {
                continue;
            }
            for item in parsed.moves {
                if self.repertoire_side.is_repertoire_turn(position.turn()) {
                    choices.insert(PositionKey::from_position(&position), item.chess_move);
                }
                position.play_unchecked(item.chess_move);
            }
        }
        (position_ply(&position), choices)
    }

    fn has_later_clean_move_line(&self, lines: &[&str], position: &Chess) -> bool {
        for line in lines {
            let stripped = line.trim();
            if !self.parser.is_move_line(stripped)
                || self
                    .parser
                    .starting_ply_for_move_line(stripped)
                    .is_some_and(|ply| ply < position_ply(position))
            {
                continue;
            }
            let parsed = self.parser.parse_move_line(stripped, position);
            return !parsed.moves.is_empty() && parsed.leftover.is_empty();
        }
        false
    }

    fn attach_comment(
        &self,
        tree: &mut MoveTree,
        node: Option<NodeId>,
        text: &str,
    ) -> Result<(), TreeError> {
        if text.is_empty() || node.is_none() {
            return Ok(());
        }
        let node = node.expect("checked above");
        tree.node_mut(node)
            .ok_or(TreeError::UnknownNode(node))?
            .add_comment(text);
        Ok(())
    }

    fn maximum_ply(&self) -> Option<u32> {
        let DepthLimit::RepertoireMoves(moves) = self.depth_limit else {
            return None;
        };
        if moves == 0 {
            return Some(0);
        }
        Some(match self.repertoire_side {
            RepertoireSide::White => moves.saturating_mul(2).saturating_sub(1),
            RepertoireSide::Black => moves.saturating_mul(2),
        })
    }
}

fn find_child(tree: &MoveTree, parent: NodeId, chess_move: Move) -> Option<NodeId> {
    tree.node(parent)?
        .children()
        .iter()
        .copied()
        .find(|&child| {
            tree.node(child)
                .and_then(super::Node::chess_move)
                .is_some_and(|candidate| candidate == chess_move)
        })
}

fn position_ply(position: &Chess) -> u32 {
    (position.fullmoves().get() - 1) * 2 + u32::from(position.turn().is_black())
}

fn parse_ply_comment_directive(line: &str) -> Option<(u32, &str)> {
    let remainder = line.strip_prefix("@@PlyComment@@")?;
    let (ply, text) = remainder.split_once("@@")?;
    Some((ply.parse().ok()?, text))
}

#[cfg(test)]
mod tests {
    use shakmaty::{Chess, Position, san::SanPlus};

    use crate::{
        domain::{Annotation, DepthLimit, RepertoireSide},
        tree::{ClassifiedRawLine, CommentReason, MoveTree, RawTreeBuilder},
    };

    fn mainline_sans(tree: &MoveTree) -> Vec<String> {
        let mut position = Chess::default();
        let mut node = tree.root();
        let mut sans = Vec::new();
        while let Some(&child) = tree.node(node).unwrap().children().first() {
            let chess_move = tree.node(child).unwrap().chess_move().unwrap();
            sans.push(SanPlus::from_move(position.clone(), chess_move).to_string());
            position.play_unchecked(chess_move);
            node = child;
        }
        sans
    }

    fn final_mainline_node(tree: &MoveTree) -> crate::tree::NodeId {
        let mut node = tree.root();
        while let Some(&child) = tree.node(node).unwrap().children().first() {
            node = child;
        }
        node
    }

    fn node_after(tree: &MoveTree, sans: &[&str]) -> (crate::tree::NodeId, Chess) {
        let mut position = Chess::default();
        let mut node = tree.root();
        for expected in sans {
            let child = tree
                .node(node)
                .unwrap()
                .children()
                .iter()
                .copied()
                .find(|&child| {
                    let chess_move = tree.node(child).unwrap().chess_move().unwrap();
                    SanPlus::from_move(position.clone(), chess_move).to_string() == *expected
                })
                .unwrap_or_else(|| panic!("missing {expected} after {sans:?}"));
            let chess_move = tree.node(child).unwrap().chess_move().unwrap();
            position.play_unchecked(chess_move);
            node = child;
        }
        (node, position)
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

    #[test]
    fn builds_glued_mainline_and_preserves_annotations() {
        let built = RawTreeBuilder::new(RepertoireSide::White)
            .build("1. e4e5?2. Nf3?!Nc6")
            .unwrap();

        assert_eq!(mainline_sans(&built.tree), ["e4", "e5", "Nf3", "Nc6"]);
        let ids: Vec<_> = built.tree.traverse().collect();
        assert_eq!(
            built.tree.node(ids[2]).unwrap().annotations(),
            &[Annotation::Mistake].into_iter().collect()
        );
        assert_eq!(
            built.tree.node(ids[3]).unwrap().annotations(),
            &[Annotation::Dubious].into_iter().collect()
        );
    }

    #[test]
    fn attaches_exact_ply_comments_without_splitting_a_black_mainline() {
        let built = RawTreeBuilder::new(RepertoireSide::Black)
            .build(concat!(
                "1. e4 1... c6 2. d4 d5\n",
                "@@PlyComment@@1@@[%cal Ge2e4]\n",
                "@@PlyComment@@2@@[%csl Rc6]"
            ))
            .unwrap();
        assert_eq!(mainline_sans(&built.tree), ["e4", "c6", "d4", "d5"]);
        let e4 = built.tree.traverse().nth(1).unwrap();
        let c6 = built.tree.traverse().nth(2).unwrap();
        assert_eq!(
            built.tree.node(e4).unwrap().comments()[0].as_str(),
            "[%cal Ge2e4]"
        );
        assert_eq!(
            built.tree.node(c6).unwrap().comments()[0].as_str(),
            "[%csl Rc6]"
        );
        assert!(matches!(
            built.lines[1].classification,
            ClassifiedRawLine::Comment {
                reason: CommentReason::ExactPly,
                starting_ply: Some(1),
                ..
            }
        ));
    }

    #[test]
    fn rejects_an_exact_ply_comment_beyond_the_mainline() {
        let error = RawTreeBuilder::new(RepertoireSide::White)
            .build("1. e4 e5\n@@PlyComment@@3@@[%cal Ge2e4]")
            .unwrap_err();
        assert!(matches!(
            error,
            crate::tree::TreeError::PlyCommentOutOfBounds {
                line_number: 2,
                ply: 3,
                available_plies: 2
            }
        ));
    }

    #[test]
    fn classifies_backtracked_and_partial_move_lines_as_comments() {
        let input = concat!(
            "1. e4c52. Nf3Nc63. d4cxd44. Nxd4e5\n",
            "4...e65.Nc3 transposes to the Taimanov variation.\n",
            "5. Nb5\n",
            "plain prose"
        );
        let built = RawTreeBuilder::new(RepertoireSide::White)
            .build(input)
            .unwrap();

        assert_eq!(
            mainline_sans(&built.tree),
            ["e4", "c5", "Nf3", "Nc6", "d4", "cxd4", "Nxd4", "e5", "Nb5"]
        );
        assert!(matches!(
            &built.lines[1].classification,
            ClassifiedRawLine::Comment {
                reason: CommentReason::BacktrackedMove,
                ..
            }
        ));
        assert!(matches!(
            &built.lines[3].classification,
            ClassifiedRawLine::Comment {
                reason: CommentReason::Prose,
                ..
            }
        ));
        let comments = built
            .tree
            .node(final_mainline_node(&built.tree))
            .unwrap()
            .comments();
        assert_eq!(comments[0].as_str(), "plain prose");
        let e5 = built.tree.traverse().nth(8).unwrap();
        assert_eq!(
            built.tree.node(e5).unwrap().comments()[0].as_str(),
            "4... e6 5. Nc3 transposes to the Taimanov variation."
        );
    }

    #[test]
    fn retains_a_partial_numbered_line_as_one_normalized_comment() {
        let input = "1. e4e52. Nf3Nc63. d4exd44. Nxd4Bc5\n4...Nf6 foo bar baz5.Nc3Bb4";
        let built = RawTreeBuilder::new(RepertoireSide::White)
            .build(input)
            .unwrap();

        assert!(matches!(
            &built.lines[1].classification,
            ClassifiedRawLine::Comment {
                reason: CommentReason::BacktrackedMove,
                ..
            }
        ));
        let final_node = final_mainline_node(&built.tree);
        assert_eq!(
            built.tree.node(final_node).unwrap().comments()[0].as_str(),
            "4... Nf6 foo bar baz 5. Nc3 Bb4"
        );
    }

    #[test]
    fn keeps_instructional_null_moves_out_of_the_tree() {
        let input = concat!(
            "1. e4c5\n",
            "2. Nf3\n",
            "2... --3. d4cxd44. Nxd4\n",
            "2... e6",
        );
        let built = RawTreeBuilder::new(RepertoireSide::White)
            .with_comment_variations(true)
            .build(input)
            .unwrap();

        assert_eq!(mainline_sans(&built.tree), ["e4", "c5", "Nf3", "e6"]);
        assert!(matches!(
            &built.lines[2].classification,
            ClassifiedRawLine::Comment {
                text,
                reason: CommentReason::InstructionalNullMove,
                ..
            } if text == "2... -- 3. d4 cxd4 4. Nxd4"
        ));
        let nf3 = built.tree.traverse().nth(3).unwrap();
        assert!(built.tree.node(nf3).unwrap().children().len() == 1);
    }

    #[test]
    fn depth_limit_counts_moves_by_the_repertoire_side() {
        let input = "1. e4e52. Nf3Nc63. Bb5a64. Ba4Nf6";
        let white = RawTreeBuilder::new(RepertoireSide::White)
            .with_depth_limit(DepthLimit::RepertoireMoves(2))
            .build(input)
            .unwrap();
        let black = RawTreeBuilder::new(RepertoireSide::Black)
            .with_depth_limit(DepthLimit::RepertoireMoves(2))
            .build(input)
            .unwrap();

        assert_eq!(mainline_sans(&white.tree), ["e4", "e5", "Nf3"]);
        assert_eq!(mainline_sans(&black.tree), ["e4", "e5", "Nf3", "Nc6"]);
    }

    #[test]
    fn zero_depth_keeps_only_root_comments_before_the_line() {
        let input = "intro\n1. e4e5\ntrailing";
        let built = RawTreeBuilder::new(RepertoireSide::White)
            .with_depth_limit(DepthLimit::RepertoireMoves(0))
            .build(input)
            .unwrap();

        assert_eq!(built.tree.len(), 1);
        assert_eq!(
            built.tree.node(built.tree.root()).unwrap().comments().len(),
            1
        );
        assert_eq!(
            built.tree.node(built.tree.root()).unwrap().comments()[0].as_str(),
            "intro"
        );
    }

    #[test]
    fn comment_variations_are_explicitly_optional() {
        let input = "1. e4e5\nWhite may also try 2.Nf3Nc6 here.\n2. Bc4Bc5";
        let disabled = RawTreeBuilder::new(RepertoireSide::Black)
            .build(input)
            .unwrap();
        let enabled = RawTreeBuilder::new(RepertoireSide::Black)
            .with_comment_variations(true)
            .build(input)
            .unwrap();

        let (disabled_e5, disabled_position) = node_after(&disabled.tree, &["e4", "e5"]);
        let (enabled_e5, enabled_position) = node_after(&enabled.tree, &["e4", "e5"]);
        assert_eq!(
            child_sans(&disabled.tree, disabled_e5, &disabled_position),
            ["Bc4"]
        );
        assert_eq!(
            child_sans(&enabled.tree, enabled_e5, &enabled_position),
            ["Bc4", "Nf3"]
        );
        let (nf3, nf3_position) = node_after(&enabled.tree, &["e4", "e5", "Nf3"]);
        assert_eq!(child_sans(&enabled.tree, nf3, &nf3_position), ["Nc6"]);
    }

    #[test]
    fn explicit_repertoire_move_rejects_comment_alternative() {
        let input = "1. e4\n1...c52.Nf3 would be alpha beta.\n1... e5";
        let built = RawTreeBuilder::new(RepertoireSide::Black)
            .with_comment_variations(true)
            .build(input)
            .unwrap();

        let (e4, position) = node_after(&built.tree, &["e4"]);
        assert_eq!(child_sans(&built.tree, e4, &position), ["e5"]);
    }

    #[test]
    fn bad_repertoire_annotation_rejects_the_whole_comment_line() {
        let input = concat!(
            "1. e4c62. d4d5\n",
            "The line 3.e5Bf54.Nf3Nf6? is not our recommendation.\n",
            "3. exd5"
        );
        let built = RawTreeBuilder::new(RepertoireSide::Black)
            .with_comment_variations(true)
            .build(input)
            .unwrap();

        let (d5, position) = node_after(&built.tree, &["e4", "c6", "d4", "d5"]);
        assert_eq!(child_sans(&built.tree, d5, &position), ["exd5"]);
    }

    #[test]
    fn bad_opponent_annotation_is_preserved_on_a_variation() {
        let input = "1. e4\n1...c5? allows alpha beta.\n1... e5";
        let built = RawTreeBuilder::new(RepertoireSide::White)
            .with_comment_variations(true)
            .build(input)
            .unwrap();

        let (e4, position) = node_after(&built.tree, &["e4"]);
        assert_eq!(child_sans(&built.tree, e4, &position), ["e5", "c5"]);
        let (c5, _) = node_after(&built.tree, &["e4", "c5"]);
        assert!(
            built
                .tree
                .node(c5)
                .unwrap()
                .annotations()
                .contains(&Annotation::Mistake)
        );
    }

    #[test]
    fn final_comments_and_overlong_variations_stay_comment_only() {
        let final_comment = RawTreeBuilder::new(RepertoireSide::White)
            .with_comment_variations(true)
            .build(concat!(
                "1. d4d52. c4e6\n",
                "This can continue with3.Nc3Nf64.Bg5, but it is just an example."
            ))
            .unwrap();
        let (e6, e6_position) = node_after(&final_comment.tree, &["d4", "d5", "c4", "e6"]);
        assert!(child_sans(&final_comment.tree, e6, &e6_position).is_empty());

        let overlong = RawTreeBuilder::new(RepertoireSide::Black)
            .with_comment_variations(true)
            .build(concat!(
                "1. e4e52. d4\n",
                "This can transpose after2.Nf3Nc63.d4exd4.\n",
                "2... exd43. c3"
            ))
            .unwrap();
        let (e5, e5_position) = node_after(&overlong.tree, &["e4", "e5"]);
        assert_eq!(child_sans(&overlong.tree, e5, &e5_position), ["d4"]);
    }
}
