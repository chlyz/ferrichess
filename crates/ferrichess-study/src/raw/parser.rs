use shakmaty::{Chess, Move, Position};

use crate::domain::Annotation;

use super::scanner::{match_legal_san, position_ply, read_move_number};

/// A byte span in one source line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// One legal move recognized from raw text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedMove {
    pub chess_move: Move,
    pub annotation: Option<Annotation>,
    pub span: SourceSpan,
}

/// Moves parsed from one raw line plus its normalized unparsed tail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawLineParse {
    pub moves: Vec<ParsedMove>,
    pub leftover: String,
}

/// A move found in an explicitly numbered sequence inside prose.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddedMove {
    pub chess_move: Move,
    pub annotation: Option<Annotation>,
}

#[derive(Clone, Debug, Default)]
pub struct RawParser;

impl RawParser {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn is_move_line(&self, line: &str) -> bool {
        let trimmed = line.trim_start();
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0 && trimmed[digits..].trim_start().starts_with('.')
    }

    #[must_use]
    pub fn starting_ply_for_move_line(&self, line: &str) -> Option<u32> {
        let offset = line.len() - line.trim_start().len();
        read_move_number(line, offset).and_then(|number| number.starting_ply)
    }

    pub(crate) fn has_instructional_null_move(&self, text: &str) -> bool {
        let mut cursor = 0;
        while cursor < text.len() {
            if let Some(number) = read_move_number(text, cursor) {
                let marker = text[number.end..].trim_start();
                if marker.starts_with("--") {
                    return true;
                }
                cursor = number.end;
            } else {
                cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
            }
        }
        false
    }

    /// Returns the ply of the first explicit move number found in text.
    #[must_use]
    pub fn first_numbered_ply(&self, text: &str) -> Option<u32> {
        let text = super::comments::split_glued_move_numbers(text);
        let mut cursor = 0;
        while cursor < text.len() {
            if let Some(number) = read_move_number(&text, cursor)
                && let Some(ply) = number.starting_ply
            {
                return Some(ply);
            }
            cursor = next_char_boundary(&text, cursor);
        }
        None
    }

    #[must_use]
    pub fn parse_move_line(&self, line: &str, position: &Chess) -> RawLineParse {
        let mut work = position.clone();
        let mut moves = Vec::new();
        let mut pos = 0;

        while pos < line.len() {
            if let Some(number) = read_move_number(line, pos) {
                pos = number.end;
                continue;
            }
            if line.as_bytes()[pos].is_ascii_whitespace() {
                pos += 1;
                continue;
            }
            if result_token_at(line, pos) {
                pos = line.len();
                break;
            }
            let Some(found) = match_legal_san(line, pos, &work) else {
                break;
            };
            let start = pos;
            pos = found.end;
            work.play_unchecked(found.chess_move);
            moves.push(ParsedMove {
                chess_move: found.chess_move,
                annotation: found.annotation,
                span: SourceSpan { start, end: pos },
            });
        }

        RawLineParse {
            moves,
            leftover: collapse_whitespace(
                super::comments::split_glued_move_numbers(&line[pos..]).trim(),
            ),
        }
    }

    #[must_use]
    pub fn parse_embedded_move_sequences(&self, text: &str, position: &Chess) -> Vec<EmbeddedMove> {
        let text = super::comments::split_glued_move_numbers(text);
        let mut work = position.clone();
        let mut moves = Vec::new();
        let mut cursor = 0;
        while cursor < text.len() {
            if read_move_number(&text, cursor).is_none() {
                cursor = next_char_boundary(&text, cursor);
                continue;
            }
            let (parsed, end) = parse_embedded_sequence(&text, cursor, &work);
            if parsed.is_empty() {
                cursor = next_char_boundary(&text, cursor);
                continue;
            }
            for item in parsed {
                work.play_unchecked(item.chess_move);
                moves.push(item);
            }
            cursor = end;
        }
        moves
    }

    #[must_use]
    pub fn normalize_comment_text(
        &self,
        text: &str,
        position: Option<&Chess>,
        positions_by_ply: Option<&[Chess]>,
    ) -> String {
        super::comments::normalize(self, text, position, positions_by_ply)
    }

    pub(crate) fn format_move_sequence(&self, position: &Chess, moves: &[EmbeddedMove]) -> String {
        use shakmaty::san::SanPlus;

        let mut work = position.clone();
        let mut parts: Vec<String> = Vec::new();
        for item in moves {
            let mut san = SanPlus::from_move(work.clone(), item.chess_move).to_string();
            if let Some(annotation) = item.annotation {
                if annotation.evaluation_nag().is_some() {
                    san.push(' ');
                }
                san.push_str(annotation.ascii_suffix());
            }
            let fullmove = work.fullmoves().get();
            if work.turn().is_white() {
                parts.push(format!("{fullmove}. {san}"));
            } else if parts
                .last()
                .is_some_and(|part| part.starts_with(&format!("{fullmove}. ")))
            {
                parts.last_mut().expect("checked above").push(' ');
                parts.last_mut().expect("checked above").push_str(&san);
            } else {
                parts.push(format!("{fullmove}... {san}"));
            }
            work.play_unchecked(item.chess_move);
        }
        parts.join(" ")
    }
}

pub(crate) fn parse_embedded_sequence(
    text: &str,
    pos: usize,
    position: &Chess,
) -> (Vec<EmbeddedMove>, usize) {
    let mut work = position.clone();
    let mut moves = Vec::new();
    let mut cursor = pos;
    loop {
        cursor = skip_whitespace(text, cursor);
        let token_start = cursor;
        if let Some(number) = read_move_number(text, cursor) {
            if number
                .starting_ply
                .is_some_and(|ply| ply != position_ply(&work))
            {
                return (moves, token_start);
            }
            cursor = number.end;
        }
        cursor = skip_whitespace(text, cursor);
        let Some(found) = match_legal_san(text, cursor, &work) else {
            return (moves, token_start);
        };
        cursor = found.end;
        work.play_unchecked(found.chess_move);
        moves.push(EmbeddedMove {
            chess_move: found.chess_move,
            annotation: found.annotation,
        });
        if cursor >= text.len() {
            return (moves, cursor);
        }
        if text.as_bytes()[cursor].is_ascii_alphabetic() {
            continue;
        }
        cursor = skip_whitespace(text, cursor);
    }
}

fn result_token_at(text: &str, pos: usize) -> bool {
    ["1-0", "0-1", "1/2-1/2", "*"]
        .iter()
        .any(|result| text[pos..].starts_with(result))
}

pub(crate) fn skip_whitespace(text: &str, mut pos: usize) -> usize {
    while text
        .as_bytes()
        .get(pos)
        .is_some_and(u8::is_ascii_whitespace)
    {
        pos += 1;
    }
    pos
}

fn next_char_boundary(text: &str, pos: usize) -> usize {
    pos + text[pos..].chars().next().map_or(1, char::len_utf8)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use shakmaty::{Chess, Position, san::San};

    use crate::domain::Annotation;

    use super::RawParser;

    fn position_after(sans: &[&str]) -> Chess {
        let mut position = Chess::default();
        for san in sans {
            let chess_move = San::from_ascii(san.as_bytes())
                .unwrap()
                .to_move(&position)
                .unwrap();
            position.play_unchecked(chess_move);
        }
        position
    }

    fn positions_after_each(sans: &[&str]) -> Vec<Chess> {
        let mut positions = vec![Chess::default()];
        for san in sans {
            let mut position = positions.last().unwrap().clone();
            let chess_move = San::from_ascii(san.as_bytes())
                .unwrap()
                .to_move(&position)
                .unwrap();
            position.play_unchecked(chess_move);
            positions.push(position);
        }
        positions
    }

    fn rendered_sans(position: &Chess, parsed: &super::RawLineParse) -> Vec<String> {
        use shakmaty::san::SanPlus;

        let mut work = position.clone();
        parsed
            .moves
            .iter()
            .map(|item| {
                let san = SanPlus::from_move(work.clone(), item.chess_move).to_string();
                work.play_unchecked(item.chess_move);
                san
            })
            .collect()
    }

    #[test]
    fn recognizes_move_lines_and_starting_ply() {
        let parser = RawParser::new();
        assert!(parser.is_move_line("  12...Nf6"));
        assert_eq!(parser.starting_ply_for_move_line("  12...Nf6"), Some(23));
        assert_eq!(parser.starting_ply_for_move_line("12. Nf3"), Some(22));
        assert!(!parser.is_move_line("after 12.Nf3"));
    }

    #[test]
    fn parses_glued_moves_without_mutating_position() {
        let parser = RawParser::new();
        let position = Chess::default();
        let parsed = parser.parse_move_line("1. e4c62. d4d53. Nc3", &position);
        assert_eq!(
            rendered_sans(&position, &parsed),
            ["e4", "c6", "d4", "d5", "Nc3"]
        );
        assert_eq!(parsed.moves[0].span, super::SourceSpan { start: 3, end: 5 });
        assert_eq!(parsed.moves[1].span, super::SourceSpan { start: 5, end: 7 });
        assert!(parsed.leftover.is_empty());
        assert_eq!(position, Chess::default());
    }

    #[test]
    fn retains_the_normalized_unparseable_tail() {
        let parser = RawParser::new();
        let position = position_after(&["e4", "e5", "Nf3", "Nc6", "d4", "exd4", "Nxd4"]);
        let parsed = parser.parse_move_line("4...Nf6 foo bar baz5.Nc3Bb46.Nxc6", &position);
        assert_eq!(rendered_sans(&position, &parsed), ["Nf6"]);
        assert_eq!(parsed.leftover, "foo bar baz5.Nc3Bb4 6.Nxc6");
    }

    #[test]
    fn parses_castling_checks_and_typed_annotations() {
        let parser = RawParser::new();
        let castle_position = position_after(&["e4", "e5", "Nf3", "Nc6", "Bc4", "Bc5"]);
        let parsed = parser.parse_move_line("4. 0-0 Nf6 5. Re1?!", &castle_position);
        assert_eq!(
            rendered_sans(&castle_position, &parsed),
            ["O-O", "Nf6", "Re1"]
        );
        assert_eq!(parsed.moves[2].annotation, Some(Annotation::Dubious));

        let mate_position = position_after(&["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6"]);
        let parsed = parser.parse_move_line("4. Qxf7#?!", &mate_position);
        assert_eq!(rendered_sans(&mate_position, &parsed), ["Qxf7#"]);
        assert_eq!(parsed.moves[0].annotation, Some(Annotation::Dubious));
    }

    #[test]
    fn evaluation_suffixes_do_not_desynchronize_the_mainline() {
        let parser = RawParser::new();
        let position = Chess::default();
        let parsed = parser.parse_move_line(
            "1.e4c52.Nf3g63.c3d54.exd5Qxd55.d4Bg76.Na3cxd47.Bc4Qe4+\
             8.Be3Nh69.Nb5O-O⇆10.cxd4Nf511.Nc7",
            &position,
        );

        assert!(parsed.leftover.is_empty());
        assert_eq!(parsed.moves.len(), 21);
        assert_eq!(parsed.moves[17].annotation, Some(Annotation::Counterplay));
        assert_eq!(
            rendered_sans(&position, &parsed)[18..],
            ["cxd4", "Nf5", "Nc7"]
        );
    }

    #[test]
    fn ignores_result_tokens_and_everything_after_them() {
        let parsed = RawParser::new().parse_move_line("1. e4 e5 1-0 ignored", &Chess::default());
        assert_eq!(rendered_sans(&Chess::default(), &parsed), ["e4", "e5"]);
        assert!(parsed.leftover.is_empty());
    }

    #[test]
    fn normalizes_comments_without_position_context() {
        let parser = RawParser::new();
        let cases = [
            (
                "foo bar5.Nf3 baz, when5...Nc66.Nc3a6 quux. lorem ipsum dolor sit amet.",
                "foo bar 5. Nf3 baz, when 5... Nc6 6. Nc3 a6 quux. lorem ipsum dolor sit amet.",
            ),
            ("foo 4. bar baz/018.baz.", "foo 4. bar baz/018.baz."),
            (
                "alpha12.Ng5 beta12...Nxc3.",
                "alpha 12. Ng5 beta 12... Nxc3.",
            ),
            ("try6...Ngf67.O-OBg7", "try 6... Ngf6 7. O-O Bg7"),
            ("after17.Bf4 or17.Bc1", "after 17. Bf4 or 17. Bc1"),
            (
                "prioritize playing ...c5 before ...e6.",
                "prioritize playing ...c5 before ...e6.",
            ),
            ("foo ...a6-b5 ... Qc7.", "foo ...a6-b5 ...Qc7."),
            (
                "after16.Nxc5?gxh5 and 8.Be2?dxc4",
                "after 16. Nxc5? gxh5 and 8. Be2? dxc4",
            ),
            ("foo bar baz 0.00.", "foo bar baz 0.00."),
            ("foo bar 1. e4 2.0 baz.", "foo bar 1. e4 2.0 baz."),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                parser.normalize_comment_text(raw, None, None),
                expected,
                "{raw}"
            );
        }
    }

    #[test]
    fn formats_legal_embedded_sequences_conservatively() {
        let parser = RawParser::new();
        let position = position_after(&["e4", "e5", "Nf3", "Nc6", "d4", "exd4", "Nxd4", "Nf6"]);
        assert_eq!(
            parser.normalize_comment_text("alpha5.Nc3Bb46.Nxc6 beta", Some(&position), None),
            "alpha 5. Nc3 Bb4 6. Nxc6 beta"
        );
        assert_eq!(
            parser.normalize_comment_text("alpha beta 4.Nxe4 gamma.", Some(&position), None),
            "alpha beta 4. Nxe4 gamma."
        );
    }

    #[test]
    fn preserves_annotations_in_embedded_sequences() {
        let parser = RawParser::new();
        let position = position_after(&[
            "c4", "e6", "Nf3", "d5", "g3", "Nf6", "Bg2", "dxc4", "Qa4+", "Bd7", "Qxc4", "c5", "d4",
            "Nc6",
        ]);
        assert_eq!(
            parser.normalize_comment_text("after8.O-Ocxd4!", Some(&position), None),
            "after 8. O-O cxd4!"
        );
    }

    #[test]
    fn separates_and_ascii_normalizes_evaluations_in_comment_sequences() {
        let parser = RawParser::new();
        assert_eq!(
            parser.normalize_comment_text("after1.f3e52.g4Qh4# +–", Some(&Chess::default()), None,),
            "after 1. f3 e5 2. g4 Qh4# +-"
        );
    }

    #[test]
    fn canonicalizes_redundant_check_and_mate_suffixes() {
        let parser = RawParser::new();
        assert_eq!(
            parser.normalize_comment_text("after1.f3e52.g4Qh4+#", Some(&Chess::default()), None),
            "after 1. f3 e5 2. g4 Qh4#"
        );
    }

    #[test]
    fn preserves_spacing_after_check_sequences() {
        let parser = RawParser::new();
        let position = position_after(&[
            "d4", "d5", "c4", "e6", "Nf3", "Nf6", "g3", "dxc4", "Bg2", "c5", "O-O", "Nc6", "Qa4",
            "Bd7", "Qxc4", "cxd4", "Nxd4", "Rc8",
        ]);
        assert_eq!(
            parser.normalize_comment_text("foo10.Nxc6Bxc611.Bxc6+Rxc6 bar", Some(&position), None,),
            "foo 10. Nxc6 Bxc6 11. Bxc6+ Rxc6 bar"
        );
    }

    #[test]
    fn uses_numbered_backtrack_context_for_comment_sequences() {
        let parser = RawParser::new();
        let positions = positions_after_each(&["e4", "c6", "Nc3", "d5", "Qf3"]);
        assert_eq!(
            parser.normalize_comment_text(
                "alpha 3.Qe2. beta after3...dxe44.Nxe4Nd7??5.Nd6#.",
                positions.last(),
                Some(&positions),
            ),
            "alpha 3. Qe2. beta after 3... dxe4 4. Nxe4 Nd7?? 5. Nd6#."
        );
    }

    #[test]
    fn does_not_split_queenside_castling() {
        let parser = RawParser::new();
        let position = position_after(&[
            "e4", "e5", "Nf3", "Nc6", "d4", "exd4", "Nxd4", "Bc5", "Nb3", "Bb6", "Nc3", "Nf6",
            "Bd3", "O-O",
        ]);
        assert_eq!(
            parser.normalize_comment_text(
                "foo 7.Qe2 bar with7...O-O8.Be3d59.O-O-Od4.",
                Some(&position),
                None,
            ),
            "foo 7. Qe2 bar with 7... O-O 8. Be3 d5 9. O-O-O d4."
        );
    }

    #[test]
    fn advances_context_across_prose_and_respects_conflicting_numbers() {
        let parser = RawParser::new();
        let position = position_after(&[
            "f4", "d5", "Nf3", "Nf6", "e3", "c5", "b3", "Nc6", "Bb5", "Bd7", "Bb2", "e6", "O-O",
            "Bd6",
        ]);
        assert_eq!(
            parser.normalize_comment_text(
                "foo bar:8.Bxc6Bxc69.Ne5Rc8 baz quux lorem10.d3O-O11.Nd2 ipsum11...Be8!",
                Some(&position),
                None,
            ),
            "foo bar: 8. Bxc6 Bxc6 9. Ne5 Rc8 baz quux lorem 10. d3 O-O 11. Nd2 ipsum 11... Be8!"
        );

        let conflict_position = position_after(&[
            "e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6", "Ng5", "d5", "exd5", "Na5", "Bb5+", "c6",
            "dxc6", "bxc6", "Ba4", "h6", "Nf3", "e4", "Qe2", "Be6",
        ]);
        assert_eq!(
            parser.normalize_comment_text(
                "foo after12.Bxc6+Nxc613.Nxc6 and13...Qc5 bar14.Nxe7",
                Some(&conflict_position),
                None,
            ),
            "foo after 12. Bxc6+ Nxc6 13. Nxc6 and 13... Qc5 bar 14. Nxe7"
        );
    }

    #[test]
    fn preserves_unicode_prose_while_normalizing_moves() {
        assert_eq!(
            RawParser::new().normalize_comment_text("idé:1.e4e5 — fortsätt", None, None),
            "idé: 1. e4 e5 — fortsätt"
        );
    }
}
