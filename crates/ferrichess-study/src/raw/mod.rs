mod comments;
mod parser;
mod scanner;

pub use parser::{EmbeddedMove, ParsedMove, RawLineParse, RawParser, SourceSpan};

/// Normalizes prose without using chess-position context.
#[must_use]
pub fn normalize_comment_text(text: &str) -> String {
    RawParser::new().normalize_comment_text(text, None, None)
}
