use std::{collections::BTreeSet, error::Error, fmt};

use shakmaty::{Chess, Move};

use crate::domain::Annotation;

/// A stable identifier into a [`MoveTree`] arena.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(usize);

impl NodeId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// One independently deduplicable piece of comment text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommentFragment(String);

impl CommentFragment {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl<T: Into<String>> From<T> for CommentFragment {
    fn from(text: T) -> Self {
        Self::new(text)
    }
}

/// A node in the move-tree arena.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    parent: Option<NodeId>,
    chess_move: Option<Move>,
    children: Vec<NodeId>,
    comments: Vec<CommentFragment>,
    annotations: BTreeSet<Annotation>,
}

impl Node {
    fn root() -> Self {
        Self {
            parent: None,
            chess_move: None,
            children: Vec::new(),
            comments: Vec::new(),
            annotations: BTreeSet::new(),
        }
    }

    fn child(parent: NodeId, chess_move: Move) -> Self {
        Self {
            parent: Some(parent),
            chess_move: Some(chess_move),
            children: Vec::new(),
            comments: Vec::new(),
            annotations: BTreeSet::new(),
        }
    }

    #[must_use]
    pub const fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    #[must_use]
    pub const fn chess_move(&self) -> Option<Move> {
        self.chess_move
    }

    #[must_use]
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    #[must_use]
    pub fn comments(&self) -> &[CommentFragment] {
        &self.comments
    }

    #[must_use]
    pub const fn annotations(&self) -> &BTreeSet<Annotation> {
        &self.annotations
    }

    pub fn add_comment(&mut self, comment: impl Into<CommentFragment>) -> bool {
        let comment = comment.into();
        if self.comments.contains(&comment) {
            false
        } else {
            self.comments.push(comment);
            true
        }
    }

    pub fn add_annotation(&mut self, annotation: Annotation) -> bool {
        self.annotations.insert(annotation)
    }
}

/// An owned, ordered arena of chess moves rooted at an explicit position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveTree {
    starting_position: Chess,
    nodes: Vec<Node>,
    root: NodeId,
}

impl Default for MoveTree {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveTree {
    #[must_use]
    pub fn new() -> Self {
        Self::from_position(Chess::default())
    }

    #[must_use]
    pub fn from_position(starting_position: Chess) -> Self {
        Self {
            starting_position,
            nodes: vec![Node::root()],
            root: NodeId(0),
        }
    }

    #[must_use]
    pub const fn starting_position(&self) -> &Chess {
        &self.starting_position
    }

    #[must_use]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0)
    }

    pub fn add_child(&mut self, parent: NodeId, chess_move: Move) -> Result<NodeId, TreeError> {
        if self.node(parent).is_none() {
            return Err(TreeError::UnknownNode(parent));
        }

        let child = NodeId(self.nodes.len());
        self.nodes.push(Node::child(parent, chess_move));
        self.nodes[parent.0].children.push(child);
        Ok(child)
    }

    /// Moves an existing direct child to the main-variation position.
    pub fn promote_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), TreeError> {
        let node = self
            .nodes
            .get_mut(parent.0)
            .ok_or(TreeError::UnknownNode(parent))?;
        let index = node
            .children
            .iter()
            .position(|candidate| *candidate == child)
            .ok_or(TreeError::NotAChild { parent, child })?;
        node.children[..=index].rotate_right(1);
        Ok(())
    }

    /// Returns node identifiers in deterministic depth-first pre-order.
    pub fn traverse(&self) -> impl Iterator<Item = NodeId> + '_ {
        let mut stack = vec![self.root];
        std::iter::from_fn(move || {
            let id = stack.pop()?;
            stack.extend(self.nodes[id.0].children.iter().rev().copied());
            Some(id)
        })
    }

    /// Checks all arena ownership, reachability, and parent/child invariants.
    pub fn validate(&self) -> Result<(), TreeError> {
        let root = self
            .nodes
            .get(self.root.0)
            .ok_or(TreeError::UnknownNode(self.root))?;
        if root.parent.is_some() || root.chess_move.is_some() {
            return Err(TreeError::InvalidRoot);
        }

        let mut seen = vec![false; self.nodes.len()];
        let mut stack = vec![self.root];
        while let Some(parent) = stack.pop() {
            if seen[parent.0] {
                return Err(TreeError::DuplicateOrCyclicNode(parent));
            }
            seen[parent.0] = true;

            for &child in &self.nodes[parent.0].children {
                let child_node = self
                    .nodes
                    .get(child.0)
                    .ok_or(TreeError::UnknownNode(child))?;
                if child_node.parent != Some(parent) {
                    return Err(TreeError::ParentMismatch {
                        child,
                        expected: parent,
                        actual: child_node.parent,
                    });
                }
                if child_node.chess_move.is_none() {
                    return Err(TreeError::MissingMove(child));
                }
                stack.push(child);
            }
        }

        if let Some(index) = seen.iter().position(|visited| !visited) {
            return Err(TreeError::UnreachableNode(NodeId(index)));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreeError {
    UnknownNode(NodeId),
    NotAChild {
        parent: NodeId,
        child: NodeId,
    },
    InvalidRoot,
    DuplicateOrCyclicNode(NodeId),
    ParentMismatch {
        child: NodeId,
        expected: NodeId,
        actual: Option<NodeId>,
    },
    MissingMove(NodeId),
    UnreachableNode(NodeId),
    PlyCommentOutOfBounds {
        line_number: usize,
        ply: u32,
        available_plies: u32,
    },
}

impl fmt::Display for TreeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode(node) => write!(formatter, "unknown node {}", node.index()),
            Self::NotAChild { parent, child } => write!(
                formatter,
                "node {} is not a child of node {}",
                child.index(),
                parent.index()
            ),
            Self::InvalidRoot => formatter.write_str("root node has a parent or move"),
            Self::DuplicateOrCyclicNode(node) => write!(
                formatter,
                "node {} is referenced more than once or forms a cycle",
                node.index()
            ),
            Self::ParentMismatch {
                child,
                expected,
                actual,
            } => write!(
                formatter,
                "node {} has parent {actual:?}, expected node {}",
                child.index(),
                expected.index()
            ),
            Self::MissingMove(node) => {
                write!(formatter, "non-root node {} has no move", node.index())
            }
            Self::UnreachableNode(node) => {
                write!(formatter, "node {} is unreachable", node.index())
            }
            Self::PlyCommentOutOfBounds {
                line_number,
                ply,
                available_plies,
            } => write!(
                formatter,
                "line {line_number} targets ply {ply}, but the mainline has {available_plies} plies"
            ),
        }
    }
}

impl Error for TreeError {}

#[cfg(test)]
mod tests {
    use shakmaty::{Chess, Move, Position, Role, Square};

    use crate::domain::Annotation;

    use super::{CommentFragment, MoveTree, NodeId, TreeError};

    fn normal(from: Square, to: Square, role: Role) -> Move {
        Move::Normal {
            role,
            from,
            capture: None,
            to,
            promotion: None,
        }
    }

    #[test]
    fn tree_retains_its_explicit_starting_position() {
        let after_e4 = Chess::default()
            .play(normal(Square::E2, Square::E4, Role::Pawn))
            .unwrap();
        let tree = MoveTree::from_position(after_e4.clone());

        assert_eq!(tree.starting_position(), &after_e4);
    }

    #[test]
    fn insertion_and_traversal_are_deterministic() {
        let mut tree = MoveTree::new();
        let root = tree.root();
        let e4 = tree
            .add_child(root, normal(Square::E2, Square::E4, Role::Pawn))
            .unwrap();
        let d4 = tree
            .add_child(root, normal(Square::D2, Square::D4, Role::Pawn))
            .unwrap();
        let e5 = tree
            .add_child(e4, normal(Square::E7, Square::E5, Role::Pawn))
            .unwrap();

        assert_eq!(tree.traverse().collect::<Vec<_>>(), [root, e4, e5, d4]);
        assert_eq!(tree.node(e5).unwrap().parent(), Some(e4));
        assert_eq!(tree.validate(), Ok(()));
    }

    #[test]
    fn promotion_changes_only_explicit_child_order() {
        let mut tree = MoveTree::new();
        let root = tree.root();
        let e4 = tree
            .add_child(root, normal(Square::E2, Square::E4, Role::Pawn))
            .unwrap();
        let d4 = tree
            .add_child(root, normal(Square::D2, Square::D4, Role::Pawn))
            .unwrap();

        tree.promote_child(root, d4).unwrap();
        assert_eq!(tree.node(root).unwrap().children(), [d4, e4]);
        assert_eq!(tree.validate(), Ok(()));
    }

    #[test]
    fn promotion_rejects_non_children() {
        let mut tree = MoveTree::new();
        let root = tree.root();
        let e4 = tree
            .add_child(root, normal(Square::E2, Square::E4, Role::Pawn))
            .unwrap();
        let e5 = tree
            .add_child(e4, normal(Square::E7, Square::E5, Role::Pawn))
            .unwrap();

        assert_eq!(
            tree.promote_child(root, e5),
            Err(TreeError::NotAChild {
                parent: root,
                child: e5
            })
        );
    }

    #[test]
    fn comments_deduplicate_without_losing_order() {
        let mut tree = MoveTree::new();
        let root = tree.root();
        let node = tree.node_mut(root).unwrap();

        assert!(node.add_comment("first"));
        assert!(node.add_comment("second"));
        assert!(!node.add_comment("first"));
        assert_eq!(
            node.comments(),
            [
                CommentFragment::new("first"),
                CommentFragment::new("second")
            ]
        );
    }

    #[test]
    fn annotations_are_typed_and_sorted() {
        let mut tree = MoveTree::new();
        let node = tree.node_mut(tree.root()).unwrap();
        node.add_annotation(Annotation::Dubious);
        node.add_annotation(Annotation::Good);
        node.add_annotation(Annotation::Good);

        assert_eq!(
            node.annotations().iter().copied().collect::<Vec<_>>(),
            [Annotation::Good, Annotation::Dubious]
        );
    }

    #[test]
    fn unknown_parent_is_rejected_without_mutating_the_arena() {
        let mut tree = MoveTree::new();
        let result = tree.add_child(NodeId(99), normal(Square::E2, Square::E4, Role::Pawn));

        assert_eq!(result, Err(TreeError::UnknownNode(NodeId(99))));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.validate(), Ok(()));
    }

    #[test]
    fn validation_detects_a_broken_parent_link() {
        let mut tree = MoveTree::new();
        let root = tree.root();
        let child = tree
            .add_child(root, normal(Square::E2, Square::E4, Role::Pawn))
            .unwrap();
        tree.nodes[child.index()].parent = Some(child);

        assert_eq!(
            tree.validate(),
            Err(TreeError::ParentMismatch {
                child,
                expected: root,
                actual: Some(child),
            })
        );
    }
}
