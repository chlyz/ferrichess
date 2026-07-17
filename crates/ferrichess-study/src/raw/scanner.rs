use shakmaty::{Chess, Move, Position, san::SanPlus};

use crate::domain::Annotation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MoveNumber {
    pub end: usize,
    pub starting_ply: Option<u32>,
}

pub(crate) fn read_move_number(text: &str, pos: usize) -> Option<MoveNumber> {
    let bytes = text.as_bytes();
    let mut cursor = pos;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let number_end = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }

    let dots = if bytes.get(cursor..cursor + 3) == Some(b"...") {
        cursor += 3;
        3
    } else if number_end != pos && bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        1
    } else {
        return None;
    };

    let starting_ply = if number_end == pos {
        None
    } else {
        text[pos..number_end]
            .parse::<u32>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|number| number.checked_mul(2))
            .and_then(|ply| ply.checked_add(u32::from(dots == 3)))
    };
    Some(MoveNumber {
        end: cursor,
        starting_ply,
    })
}

#[derive(Clone, Debug)]
pub(crate) struct SanMatch {
    pub chess_move: Move,
    pub end: usize,
    pub annotation: Option<Annotation>,
}

pub(crate) fn match_legal_san(text: &str, pos: usize, position: &Chess) -> Option<SanMatch> {
    let mut candidates: Vec<_> = position
        .legal_moves()
        .into_iter()
        .map(|chess_move| {
            let san = SanPlus::from_move(position.clone(), chess_move).to_string();
            (san, chess_move)
        })
        .collect();
    candidates.sort_by_key(|(san, _)| std::cmp::Reverse(san.len()));

    for (san, chess_move) in candidates {
        let without_check = san.trim_end_matches(['+', '#']);
        for candidate in [san.as_str(), without_check] {
            if candidate.is_empty() || !san_prefix_matches(text, pos, candidate) {
                continue;
            }
            let mut end = pos + candidate.len();
            while text
                .as_bytes()
                .get(end)
                .is_some_and(|byte| matches!(byte, b'+' | b'#'))
            {
                if evaluation_at(text, end).is_some() {
                    break;
                }
                end += 1;
            }
            if text.as_bytes().get(end) == Some(&b'-') {
                continue;
            }
            let suffix_start = end;
            while text
                .as_bytes()
                .get(end)
                .is_some_and(|byte| matches!(byte, b'!' | b'?'))
            {
                end += 1;
            }
            let annotation = if end > suffix_start {
                Annotation::from_suffix(&text[suffix_start..end])
            } else if let Some((annotation, evaluation_end)) =
                evaluation_at(text, skip_ascii_whitespace(text, end))
            {
                end = evaluation_end;
                Some(annotation)
            } else {
                None
            };
            return Some(SanMatch {
                chess_move,
                end,
                annotation,
            });
        }
    }
    None
}

fn skip_ascii_whitespace(text: &str, mut pos: usize) -> usize {
    while text
        .as_bytes()
        .get(pos)
        .is_some_and(u8::is_ascii_whitespace)
    {
        pos += 1;
    }
    pos
}

fn evaluation_at(text: &str, pos: usize) -> Option<(Annotation, usize)> {
    [
        "<=>", "+/-", "-/+", "=∞", "=~", "+–", "–+", "+-", "-+", "+=", "=+", "⩲", "⩱", "±", "∓",
        "∞", "⇆", "~", "=",
    ]
    .into_iter()
    .find_map(|suffix| {
        text[pos..].starts_with(suffix).then(|| {
            (
                Annotation::from_suffix(suffix).expect("listed evaluation suffix"),
                pos + suffix.len(),
            )
        })
    })
}

fn san_prefix_matches(text: &str, pos: usize, san: &str) -> bool {
    let Some(raw) = text.get(pos..pos + san.len()) else {
        return false;
    };
    raw.bytes()
        .zip(san.bytes())
        .all(|(raw, expected)| raw == expected || (raw == b'0' && expected == b'O'))
}

pub(crate) fn position_ply(position: &Chess) -> u32 {
    (position.fullmoves().get() - 1) * 2 + u32::from(position.turn().is_black())
}

#[cfg(test)]
mod tests {
    use shakmaty::{Chess, Position, san::San};

    use super::match_legal_san;

    #[test]
    fn parses_position_evaluations_after_check_suffixes() {
        let mut position = Chess::default();
        for san in ["f3", "e5", "g4"] {
            let chess_move = San::from_ascii(san.as_bytes())
                .unwrap()
                .to_move(&position)
                .unwrap();
            position.play_unchecked(chess_move);
        }

        let found = match_legal_san("Qh4#+–", 0, &position).unwrap();
        assert_eq!(found.end, "Qh4#+–".len());
        assert_eq!(
            found.annotation,
            Some(crate::domain::Annotation::WhiteWinning)
        );

        let spaced = match_legal_san("Qh4# +–", 0, &position).unwrap();
        assert_eq!(spaced.end, "Qh4# +–".len());
        assert_eq!(spaced.annotation, found.annotation);
    }
}
