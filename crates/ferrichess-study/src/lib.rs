//! Convert compact chess study text into validated move trees and deterministic
//! PGN.
//!
//! The normal conversion path is [`convert_single_raw`] followed by
//! [`PgnWriter::render`]:
//!
//! ```
//! use ferrichess_study::{
//!     convert_single_raw, PgnWriter, RepertoireRole, RepertoireSide,
//!     SingleRawMetadata,
//! };
//!
//! let metadata = SingleRawMetadata {
//!     course_title: "Example study".to_owned(),
//!     event: "Open games".to_owned(),
//!     chapter_slug: "open-games".to_owned(),
//!     index: "001".to_owned(),
//!     repertoire_side: RepertoireSide::White,
//!     repertoire_role: RepertoireRole::Main,
//! };
//!
//! let document = convert_single_raw("1. e4e52. Nf3Nc6", &metadata)?;
//! let pgn = PgnWriter::render(&document)?;
//! assert!(pgn.contains("1. e4 e5 2. Nf3 Nc6 *"));
//! # Ok::<(), ferrichess_study::PgnError>(())
//! ```
//!
//! See the crate README for the compact input format, conversion model, and
//! pre-1.0 compatibility policy.

pub mod course;
pub mod diagnostics;
pub mod domain;
pub mod pgn;
pub mod raw;
pub mod tree;

#[cfg(test)]
mod test_support;

pub use course::{
    ChapterMetadata, CourseDocumentError, CourseDocuments, CourseInfo, CourseKind, CourseMetadata,
    CurriculumClassification, DocumentMerge, LichessChapter, LichessMetadata, MetadataError,
    NamedDocument, RawCourseLine, RepertoireSettings, SourceAttribution, generate_course_documents,
};
pub use diagnostics::{canonical_tree_json, parser_trace_json};
pub use domain::{Annotation, DepthLimit, PositionKey, RepertoireRole, RepertoireSide, SourceId};
pub use pgn::{
    Header, Headers, PgnDocument, PgnError, PgnWriter, SingleRawMetadata, convert_single_raw,
};
pub use raw::{EmbeddedMove, ParsedMove, RawLineParse, RawParser, SourceSpan};
pub use tree::{
    BuiltRawTree, ClassifiedRawLine, CommentFragment, CommentReason, CommentVariationDecision,
    MergeConflict, MergeError, MergeReport, MoveTree, MoveTreeMerger, Node, NodeId, RawLineRecord,
    RawTreeBuilder, TreeError,
};
