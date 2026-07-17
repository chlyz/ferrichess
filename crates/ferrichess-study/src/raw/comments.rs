use shakmaty::Chess;

use super::{
    parser::{RawParser, parse_embedded_sequence},
    scanner::{position_ply, read_move_number},
};

pub(crate) fn normalize(
    parser: &RawParser,
    text: &str,
    position: Option<&Chess>,
    positions_by_ply: Option<&[Chess]>,
) -> String {
    let mut text = split_glued_move_numbers(text);
    if let Some(position) = position {
        text = format_embedded_sequences(parser, text, position, positions_by_ply);
    }
    text = format_comment_move_numbers(&text);
    text = format_bare_black_markers(&text);
    text = split_glued_san(&text);
    text = collapse_whitespace(&text);
    remove_space_before_punctuation(&text)
}

pub(crate) fn split_glued_move_numbers(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor].is_ascii_digit() {
            let digit_start = cursor;
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
            let number_start = if cursor - digit_start >= 2
                && is_rank(bytes[digit_start])
                && should_split_before_number(bytes, digit_start + 1)
            {
                output.push(bytes[digit_start] as char);
                digit_start + 1
            } else {
                digit_start
            };
            if bytes.get(cursor) == Some(&b'.')
                && should_split_before_number(bytes, number_start)
                && !output.ends_with(char::is_whitespace)
            {
                output.push(' ');
            }
            output.push_str(&text[number_start..cursor]);
        } else {
            let ch = text[cursor..].chars().next().expect("cursor is in bounds");
            output.push(ch);
            cursor += ch.len_utf8();
        }
    }
    output
}

fn should_split_before_number(bytes: &[u8], start: usize) -> bool {
    if start >= 3 && &bytes[start - 3..start] == b"O-O" {
        return true;
    }
    if start < 2 || !is_file(bytes[start - 2]) || !is_rank(bytes[start - 1]) {
        return false;
    }
    let before_square = start
        .checked_sub(3)
        .and_then(|index| bytes.get(index))
        .copied();
    if before_square
        .is_none_or(|byte| !(b'a'..=b'w').contains(&byte) && byte != b'y' && byte != b'z')
    {
        return true;
    }
    if start >= 3 && is_piece(bytes[start - 3]) {
        return true;
    }
    start >= 4 && is_piece(bytes[start - 4]) && is_file(bytes[start - 3])
}

fn format_embedded_sequences(
    parser: &RawParser,
    mut text: String,
    position: &Chess,
    positions_by_ply: Option<&[Chess]>,
) -> String {
    let mut work = position.clone();
    let mut cursor = 0;
    while cursor < text.len() {
        let Some(number) = read_move_number(&text, cursor) else {
            cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let sequence_position = number
            .starting_ply
            .filter(|ply| *ply != position_ply(&work))
            .and_then(|ply| positions_by_ply.and_then(|items| items.get(ply as usize)))
            .cloned()
            .unwrap_or_else(|| work.clone());
        let (moves, end) = parse_embedded_sequence(&text, cursor, &sequence_position);
        if moves.is_empty() {
            cursor = number.end;
            continue;
        }
        if moves.len() < 2 {
            cursor = end;
        } else {
            let formatted = parser.format_move_sequence(&sequence_position, &moves);
            let (replacement, next) = replace_with_spacing(&text, cursor, end, &formatted);
            text = replacement;
            cursor = next;
        }
        work = sequence_position;
        use shakmaty::Position;
        for item in moves {
            work.play_unchecked(item.chess_move);
        }
    }
    text
}

fn replace_with_spacing(
    text: &str,
    start: usize,
    end: usize,
    replacement: &str,
) -> (String, usize) {
    let prefix = &text[..start];
    let suffix = &text[end..];
    let left_padding = prefix
        .chars()
        .next_back()
        .is_some_and(|ch| !ch.is_whitespace() && !"([{``".contains(ch));
    let right_padding = suffix
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace() && !".,;:!?)]}".contains(ch));
    let mut output = String::with_capacity(text.len() + replacement.len());
    output.push_str(prefix);
    if left_padding {
        output.push(' ');
    }
    output.push_str(replacement);
    if right_padding {
        output.push(' ');
    }
    let next = output.len();
    output.push_str(suffix);
    (output, next)
}

fn format_comment_move_numbers(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor].is_ascii_digit() && (cursor == 0 || !bytes[cursor - 1].is_ascii_digit()) {
            let start = cursor;
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) && cursor - start < 3 {
                cursor += 1;
            }
            if bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                output.push_str(&text[start..cursor]);
                continue;
            }
            let number_end = cursor;
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            let marker_end = if bytes.get(cursor..cursor + 3) == Some(b"...") {
                cursor + 3
            } else if bytes.get(cursor) == Some(&b'.') {
                cursor + 1
            } else {
                output.push_str(&text[start..number_end]);
                cursor = number_end;
                continue;
            };
            let mut san_start = marker_end;
            while bytes.get(san_start).is_some_and(u8::is_ascii_whitespace) {
                san_start += 1;
            }
            if looks_like_san_start(bytes, san_start)
                && !looks_like_coordinate(text, start, number_end)
            {
                if !output.is_empty() && !output.ends_with(char::is_whitespace) {
                    output.push(' ');
                }
                output.push_str(&text[start..number_end]);
                output.push_str(&text[cursor..marker_end]);
                output.push(' ');
                cursor = san_start;
                continue;
            }
            output.push_str(&text[start..number_end]);
            cursor = number_end;
            continue;
        }
        let ch = text[cursor..].chars().next().expect("cursor is in bounds");
        output.push(ch);
        cursor += ch.len_utf8();
    }
    output
}

fn looks_like_coordinate(text: &str, start: usize, number_end: usize) -> bool {
    number_end - start == 1
        && start >= 1
        && is_file(text.as_bytes()[start - 1])
        && (start < 2 || !text.as_bytes()[start - 2].is_ascii_lowercase())
}

fn format_bare_black_markers(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if let Some(san_start) = spaced_ellipsis_san_start(bytes, cursor) {
            let numbered_move = cursor > 0 && bytes[cursor - 1].is_ascii_digit();
            output.push_str("...");
            if numbered_move {
                output.push(' ');
            }
            cursor = san_start;
            continue;
        }
        let ch = text[cursor..].chars().next().expect("cursor is in bounds");
        output.push(ch);
        cursor += ch.len_utf8();
    }
    output
}

fn spaced_ellipsis_san_start(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    for dot_index in 0..3 {
        if dot_index > 0 {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
        }
        if bytes.get(cursor) != Some(&b'.') {
            return None;
        }
        cursor += 1;
    }
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    looks_like_san_start(bytes, cursor).then_some(cursor)
}

fn split_glued_san(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        let previous_is_boundary =
            cursor >= 2 && is_file(bytes[cursor - 2]) && is_rank(bytes[cursor - 1])
                || cursor >= 3 && &bytes[cursor - 3..cursor] == b"O-O"
                || cursor >= 1 && matches!(bytes[cursor - 1], b'+' | b'#' | b'!' | b'?');
        if previous_is_boundary
            && looks_like_san_start(bytes, cursor)
            && !output.ends_with(char::is_whitespace)
        {
            output.push(' ');
        }
        let ch = text[cursor..].chars().next().expect("cursor is in bounds");
        output.push(ch);
        cursor += ch.len_utf8();
    }
    output
}

fn looks_like_san_start(bytes: &[u8], pos: usize) -> bool {
    if bytes.get(pos..pos + 3) == Some(b"O-O") {
        return true;
    }
    let Some(&first) = bytes.get(pos) else {
        return false;
    };
    if is_piece(first) {
        return (pos + 1..=pos + 3).any(|destination| {
            bytes.get(destination).is_some_and(|byte| is_file(*byte))
                && bytes
                    .get(destination + 1)
                    .is_some_and(|byte| is_rank(*byte))
                && bytes[pos + 1..destination]
                    .iter()
                    .all(|byte| is_file(*byte) || is_rank(*byte) || *byte == b'x')
        });
    }
    if !is_file(first) {
        return false;
    }
    if bytes.get(pos + 1) == Some(&b'x') {
        return bytes.get(pos + 2).is_some_and(|byte| is_file(*byte))
            && bytes.get(pos + 3).is_some_and(|byte| is_rank(*byte));
    }
    bytes.get(pos + 1).is_some_and(|byte| is_rank(*byte))
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn remove_space_before_punctuation(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let chars: Vec<_> = text.chars().collect();
    for (index, &ch) in chars.iter().enumerate() {
        let punctuation = matches!(ch, ',' | ';' | ':' | '!' | '?')
            || ch == '.' && chars.get(index + 1) != Some(&'.');
        if punctuation && output.ends_with(' ') {
            output.pop();
        }
        output.push(ch);
    }
    output
}

const fn is_file(byte: u8) -> bool {
    matches!(byte, b'a'..=b'h')
}

const fn is_rank(byte: u8) -> bool {
    matches!(byte, b'1'..=b'8')
}

const fn is_piece(byte: u8) -> bool {
    matches!(byte, b'K' | b'Q' | b'R' | b'B' | b'N')
}

#[cfg(test)]
mod tests {
    use super::{format_bare_black_markers, split_glued_move_numbers};

    #[test]
    fn splits_destination_squares_and_castling_from_following_numbers() {
        assert_eq!(
            split_glued_move_numbers("e4c62.d4 O-O8.Be3"),
            "e4c6 2.d4 O-O 8.Be3"
        );
        assert_eq!(split_glued_move_numbers("alpha12.Ng5"), "alpha12.Ng5");
    }

    #[test]
    fn distinguishes_numbered_moves_from_black_move_markers_in_prose() {
        assert_eq!(
            format_bare_black_markers("13... Rf8 then ... Rf8"),
            "13... Rf8 then ...Rf8"
        );
        assert_eq!(
            format_bare_black_markers("with .. .Rf8 or . .. Bb7"),
            "with ...Rf8 or ...Bb7"
        );
    }
}
