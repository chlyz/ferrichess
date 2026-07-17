//! Versioned course metadata, validation, and filesystem-independent assembly.

mod documents;
mod metadata;

pub use documents::{
    CourseDocumentError, CourseDocuments, DocumentMerge, NamedDocument, RawCourseLine,
    generate_course_documents,
};

pub use metadata::{
    ChapterMetadata, CourseInfo, CourseKind, CourseMetadata, CurriculumClassification,
    LichessChapter, LichessMetadata, MetadataError, RepertoireSettings, SourceAttribution,
};
