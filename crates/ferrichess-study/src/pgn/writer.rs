use shakmaty::{Position, san::SanPlus};

use super::{PgnDocument, PgnError};

/// Deterministic writer for Python-compatible PGN bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct PgnWriter;

impl PgnWriter {
    /// Renders one game like `str(chess.pgn.Game)`, without a final newline.
    pub fn render(document: &PgnDocument) -> Result<String, PgnError> {
        let mut output = String::new();
        for header in document.headers().iter() {
            output.push('[');
            output.push_str(&header.name);
            output.push_str(" \"");
            push_escaped_header(&mut output, &header.value);
            output.push_str("\"]\n");
        }
        output.push('\n');
        output.push_str(&render_linear_movetext(document)?);
        Ok(output)
    }

    /// Renders the exact bytes written for one standalone Python-oracle game.
    pub fn render_file(document: &PgnDocument) -> Result<Vec<u8>, PgnError> {
        let mut rendered = Self::render(document)?;
        rendered.push_str("\n\n");
        Ok(rendered.into_bytes())
    }

    /// Renders a multi-game PGN file, using the same separator as individual files.
    pub fn render_documents(documents: &[PgnDocument]) -> Result<Vec<u8>, PgnError> {
        let mut output = Vec::new();
        for document in documents {
            output.extend(Self::render_file(document)?);
        }
        Ok(output)
    }
}

fn render_linear_movetext(document: &PgnDocument) -> Result<String, PgnError> {
    let tree = document.tree();
    let mut output = MovetextWriter::new();
    let root = tree.root();
    render_comments(tree, root, &mut output)?;
    render_from_node(
        tree,
        root,
        tree.starting_position().clone(),
        true,
        &mut output,
    )?;

    output.write_token(&format!("{} ", document.result()));
    Ok(output.finish())
}

fn render_from_node(
    tree: &crate::tree::MoveTree,
    mut parent_id: crate::tree::NodeId,
    mut position: shakmaty::Chess,
    mut force_move_number: bool,
    output: &mut MovetextWriter,
) -> Result<(), PgnError> {
    loop {
        let parent = tree
            .node(parent_id)
            .ok_or(crate::tree::TreeError::UnknownNode(parent_id))?;
        let Some((&main_child, alternatives)) = parent.children().split_first() else {
            return Ok(());
        };
        let has_alternatives = !alternatives.is_empty();
        let main_has_comment = render_move(tree, main_child, &position, force_move_number, output)?;

        for &alternative in alternatives {
            output.write_token("( ");
            let alternative_has_comment = render_move(tree, alternative, &position, true, output)?;
            let chess_move = tree
                .node(alternative)
                .and_then(crate::tree::Node::chess_move)
                .ok_or(crate::tree::TreeError::MissingMove(alternative))?;
            let mut alternative_position = position.clone();
            alternative_position.play_unchecked(chess_move);
            render_from_node(
                tree,
                alternative,
                alternative_position,
                alternative_has_comment,
                output,
            )?;
            output.write_token(") ");
        }

        let chess_move = tree
            .node(main_child)
            .and_then(crate::tree::Node::chess_move)
            .ok_or(crate::tree::TreeError::MissingMove(main_child))?;
        position.play_unchecked(chess_move);
        parent_id = main_child;
        force_move_number = main_has_comment || has_alternatives;
    }
}

fn render_move(
    tree: &crate::tree::MoveTree,
    child_id: crate::tree::NodeId,
    position: &shakmaty::Chess,
    force_move_number: bool,
    output: &mut MovetextWriter,
) -> Result<bool, PgnError> {
    let child = tree
        .node(child_id)
        .ok_or(crate::tree::TreeError::UnknownNode(child_id))?;
    let chess_move = child
        .chess_move()
        .ok_or(crate::tree::TreeError::MissingMove(child_id))?;
    let mut san = SanPlus::from_move(position.clone(), chess_move).to_string();
    for annotation in child.annotations() {
        if annotation.evaluation_nag().is_none() {
            san.push_str(annotation.suffix());
        }
    }
    if position.turn().is_white() {
        output.write_token(&format!("{}. ", position.fullmoves()));
    } else if force_move_number {
        output.write_token(&format!("{}... ", position.fullmoves()));
    }
    output.write_token(&format!("{san} "));
    for nag in child
        .annotations()
        .iter()
        .filter_map(|annotation| annotation.evaluation_nag())
    {
        output.write_token(&format!("${nag} "));
    }
    render_comments(tree, child_id, output)
}

fn render_comments(
    tree: &crate::tree::MoveTree,
    node_id: crate::tree::NodeId,
    output: &mut MovetextWriter,
) -> Result<bool, PgnError> {
    let node = tree
        .node(node_id)
        .ok_or(crate::tree::TreeError::UnknownNode(node_id))?;
    if node.comments().is_empty() {
        return Ok(false);
    }
    let comment = node
        .comments()
        .iter()
        .map(|fragment| fragment.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .replace('}', "");
    let comment = combine_graphical_directives(&comment);
    output.write_token(&format!("{{ {} }} ", comment.trim()));
    Ok(true)
}

fn combine_graphical_directives(comment: &str) -> String {
    let mut prose = String::new();
    let mut arrows: Vec<&str> = Vec::new();
    let mut squares: Vec<&str> = Vec::new();
    let mut remaining = comment;
    while let Some(start) = remaining.find("[%") {
        prose.push_str(&remaining[..start]);
        let Some(relative_end) = remaining[start..].find(']') else {
            prose.push_str(&remaining[start..]);
            remaining = "";
            break;
        };
        let end = start + relative_end + 1;
        let directive = &remaining[start..end];
        let destination = if let Some(values) = directive
            .strip_prefix("[%cal ")
            .and_then(|value| value.strip_suffix(']'))
        {
            Some((&mut arrows, values))
        } else {
            directive
                .strip_prefix("[%csl ")
                .and_then(|value| value.strip_suffix(']'))
                .map(|values| (&mut squares, values))
        };
        if let Some((items, values)) = destination {
            for item in values
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                if !items.contains(&item) {
                    items.push(item);
                }
            }
            prose.push(' ');
        } else {
            prose.push_str(directive);
        }
        remaining = &remaining[end..];
    }
    prose.push_str(remaining);

    let mut parts = Vec::new();
    if !arrows.is_empty() {
        parts.push(format!("[%cal {}]", arrows.join(",")));
    }
    if !squares.is_empty() {
        parts.push(format!("[%csl {}]", squares.join(",")));
    }
    let prose = prose.split_whitespace().collect::<Vec<_>>().join(" ");
    if !prose.is_empty() {
        parts.push(prose);
    }
    parts.join(" ")
}

struct MovetextWriter {
    output: String,
}

impl MovetextWriter {
    const fn new() -> Self {
        Self {
            output: String::new(),
        }
    }

    fn write_token(&mut self, token: &str) {
        self.output.push_str(token);
    }

    fn finish(self) -> String {
        self.output.trim_end().to_owned()
    }
}

fn push_escaped_header(output: &mut String, value: &str) {
    for character in value.chars() {
        if matches!(character, '\\' | '"') {
            output.push('\\');
        }
        output.push(character);
    }
}

#[cfg(test)]
mod tests {
    use shakmaty::{Chess, Move, Position, Role, Square};

    use crate::{
        domain::{RepertoireRole, RepertoireSide},
        pgn::{Headers, PgnDocument, PgnWriter, SingleRawMetadata, convert_single_raw},
        test_support::{FRENCH_PREFIX, ITALIAN_CHECK, ITALIAN_PREFIX},
        tree::{MoveTree, MoveTreeMerger, RawTreeBuilder},
    };

    fn compact(source: &str) -> String {
        source.replace(". ", ".").replace(' ', "")
    }

    fn simple_metadata() -> SingleRawMetadata {
        SingleRawMetadata {
            course_title: "Example Collection".to_owned(),
            event: "Example Chapter".to_owned(),
            chapter_slug: "white-open".to_owned(),
            index: "001".to_owned(),
            repertoire_side: RepertoireSide::White,
            repertoire_role: RepertoireRole::Main,
        }
    }

    #[test]
    fn simple_raw_file_matches_recorded_python_oracle_bytes() {
        let document = convert_single_raw(&compact(FRENCH_PREFIX), &simple_metadata()).unwrap();
        let actual = PgnWriter::render_file(&document).unwrap();
        let python_oracle = concat!(
            "[Event \"Example Chapter\"]\n",
            "[Site \"?\"]\n",
            "[Date \"????.??.??\"]\n",
            "[Round \"001\"]\n",
            "[White \"001\"]\n",
            "[Black \"Example Collection\"]\n",
            "[Result \"*\"]\n",
            "[Chapter \"White Open\"]\n",
            "[Orientation \"White\"]\n",
            "[RepertoireSide \"White\"]\n",
            "[RepertoireRole \"Main\"]\n",
            "\n",
            "1. e4 e6 2. d4 b6 3. a3 Bb7 *\n\n",
        );
        assert_eq!(actual, python_oracle.as_bytes());
    }

    #[test]
    fn render_has_no_final_newline_but_file_has_two() {
        let document = convert_single_raw("1. e4", &simple_metadata()).unwrap();
        assert!(PgnWriter::render(&document).unwrap().ends_with("1. e4 *"));
        assert!(
            PgnWriter::render_file(&document)
                .unwrap()
                .ends_with(b"*\n\n")
        );
    }

    #[test]
    fn renders_comments_without_wrapping_like_python_game_string() {
        let raw = concat!(
            "1. e4e6\n",
            "This ordinary comment is long enough to move onto its own line ",
            "instead of being split internally.\n",
            "2. d4b6",
        );
        let document = convert_single_raw(raw, &simple_metadata()).unwrap();
        let rendered = PgnWriter::render(&document).unwrap();
        let movetext = rendered.split_once("\n\n").unwrap().1;

        assert_eq!(
            movetext,
            concat!(
                "1. e4 e6 { This ordinary comment is long enough to move onto its own line ",
                "instead of being split internally. } 2. d4 b6 *",
            ),
        );
    }

    #[test]
    fn combines_graphical_directives_so_lichess_preserves_every_shape() {
        let document = convert_single_raw(
            concat!(
                "1. e4\n",
                "@@PlyComment@@1@@[%cal Ge2e4,Ge2e4]\n",
                "@@PlyComment@@1@@Center control.\n",
                "@@PlyComment@@1@@[%cal Gd2d4]\n",
                "@@PlyComment@@1@@[%csl Gd4]\n",
                "@@PlyComment@@1@@[%csl Re4]",
            ),
            &simple_metadata(),
        )
        .unwrap();

        assert!(
            PgnWriter::render(&document)
                .unwrap()
                .ends_with("1. e4 { [%cal Ge2e4,Gd2d4] [%csl Gd4,Re4] Center control. } *")
        );
    }

    #[test]
    fn comment_before_black_move_forces_the_move_number() {
        let document = convert_single_raw(
            "1. e4\nA note after White's move.\n1... e6",
            &simple_metadata(),
        )
        .unwrap();

        assert!(
            PgnWriter::render(&document)
                .unwrap()
                .ends_with("1. e4 { A note after White's move. } 1... e6 *")
        );
    }

    #[test]
    fn keeps_long_movetext_unwrapped_like_python_game_string() {
        let document = convert_single_raw(&compact(ITALIAN_PREFIX), &simple_metadata()).unwrap();
        let movetext = PgnWriter::render(&document)
            .unwrap()
            .split_once("\n\n")
            .unwrap()
            .1
            .to_owned();

        assert_eq!(movetext, "1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 4. Nc3 Bc5 *");
    }

    #[test]
    fn variation_before_a_black_mainline_continuation_forces_its_move_number() {
        let first = RawTreeBuilder::new(RepertoireSide::Black)
            .build("1. e4e62. Nf3d5")
            .unwrap()
            .tree;
        let second = RawTreeBuilder::new(RepertoireSide::Black)
            .build("1. e4e62. d4b6")
            .unwrap()
            .tree;
        let mut tree = MoveTree::new();
        let mut merger = MoveTreeMerger::new(RepertoireSide::Black);
        merger.merge(&mut tree, &first, "001.raw").unwrap();
        merger.merge(&mut tree, &second, "002.raw").unwrap();
        let document = PgnDocument {
            headers: Headers::new(),
            tree,
            result: "*".to_owned(),
        };

        assert_eq!(
            PgnWriter::render(&document).unwrap(),
            "\n1. e4 e6 2. d4 ( 2. Nf3 d5 ) 2... b6 *"
        );
    }

    #[test]
    fn movetext_numbering_uses_the_tree_starting_position() {
        let e4 = Move::Normal {
            role: Role::Pawn,
            from: Square::E2,
            capture: None,
            to: Square::E4,
            promotion: None,
        };
        let e6 = Move::Normal {
            role: Role::Pawn,
            from: Square::E7,
            capture: None,
            to: Square::E6,
            promotion: None,
        };
        let after_e4 = Chess::default().play(e4).unwrap();
        let mut tree = MoveTree::from_position(after_e4);
        tree.add_child(tree.root(), e6).unwrap();
        let document = PgnDocument {
            headers: Headers::new(),
            tree,
            result: "*".to_owned(),
        };

        assert_eq!(PgnWriter::render(&document).unwrap(), "\n1... e6 *");
    }

    #[test]
    fn renders_position_evaluation_as_a_separate_numeric_nag() {
        let document = convert_single_raw("1. e4 +-", &simple_metadata()).unwrap();

        assert!(
            PgnWriter::render(&document)
                .unwrap()
                .ends_with("1. e4 $18 *")
        );
    }

    #[test]
    fn keeps_check_separate_from_position_evaluation() {
        let document =
            convert_single_raw(&format!("{ITALIAN_CHECK} +-"), &simple_metadata()).unwrap();
        let rendered = PgnWriter::render(&document).unwrap();

        assert!(rendered.ends_with("11. Nxc7+ $18 *"), "{rendered}");
    }
}
