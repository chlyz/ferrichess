//! Core types for converting compact chess study text into move trees and PGN.

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
