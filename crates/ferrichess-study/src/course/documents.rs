use std::{cmp::Ordering, error::Error, fmt};

use shakmaty::Chess;

use crate::{
    PgnDocument, PgnError, RepertoireRole, RepertoireSide, SingleRawMetadata, SourceId,
    convert_single_raw,
    pgn::Headers,
    tree::{MergeConflict, MoveTree, MoveTreeMerger},
};

use super::{CourseKind, CourseMetadata};

/// One raw source supplied to the pure course-conversion boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawCourseLine {
    pub source: SourceId,
    pub chapter_id: String,
    pub index: String,
    pub text: String,
}

/// A generated document and the stable name used by output adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedDocument {
    pub name: String,
    pub document: PgnDocument,
}

/// Merge diagnostics associated with one aggregate output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentMerge {
    pub name: String,
    pub conflicts: Vec<MergeConflict>,
}

/// All local aggregate documents for a repertoire course.
///
/// `chapters` are also the games in the multi-game course PGN, in declared
/// metadata order. `full` is ordered Black then White, matching the Python
/// converter's output order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CourseDocuments {
    pub course_id: String,
    pub chapters: Vec<NamedDocument>,
    pub full: Vec<NamedDocument>,
    pub merges: Vec<DocumentMerge>,
}

impl CourseDocuments {
    /// Returns the chapter games that make up the course PGN.
    pub fn course_games(&self) -> impl ExactSizeIterator<Item = &PgnDocument> {
        self.chapters.iter().map(|chapter| &chapter.document)
    }
}

/// Converts raw text to chapter, side-full, and ordered course documents.
///
/// This boundary performs no discovery, filesystem writes, or rendering. Raw
/// lines are sorted by their Python-compatible numeric index within each
/// chapter; chapter order comes from `course.json` metadata.
pub fn generate_course_documents(
    metadata: &CourseMetadata,
    raw_lines: &[RawCourseLine],
) -> Result<CourseDocuments, CourseDocumentError> {
    let course = metadata
        .course
        .as_ref()
        .ok_or(CourseDocumentError::MissingCourse)?;
    if course.kind != CourseKind::Repertoire {
        return Err(CourseDocumentError::UnsupportedCourseKind(course.kind));
    }
    let default_side = metadata
        .repertoire_side()
        .ok_or(CourseDocumentError::MissingRepertoireSide)?;

    for raw in raw_lines {
        if !metadata
            .chapters
            .iter()
            .any(|chapter| chapter.id == raw.chapter_id)
        {
            return Err(CourseDocumentError::UnknownChapter(raw.chapter_id.clone()));
        }
    }

    let mut chapters = Vec::new();
    let mut merges = Vec::new();
    for chapter in &metadata.chapters {
        let mut sources: Vec<_> = raw_lines
            .iter()
            .filter(|raw| raw.chapter_id == chapter.id)
            .collect();
        sources.sort_by(|left, right| compare_indexes(&left.index, &right.index));
        if sources.is_empty() {
            continue;
        }

        let side = side_for_chapter(&chapter.id).unwrap_or(default_side);
        let role = role_for_chapter(&chapter.id);
        let title = title_from_slug(&chapter.id);
        let mut tree = MoveTree::new();
        let mut merger = MoveTreeMerger::new(side);
        let mut conflicts = Vec::new();
        for raw in sources {
            let line = convert_single_raw(
                &raw.text,
                &SingleRawMetadata {
                    course_title: course.title.clone(),
                    event: title.clone(),
                    chapter_slug: chapter.id.clone(),
                    index: raw.index.clone(),
                    repertoire_side: side,
                    repertoire_role: role,
                },
            )?;
            conflicts.extend(
                merger
                    .merge(&mut tree, line.tree(), raw.source.clone())?
                    .conflicts,
            );
        }

        let name = chapter.id.clone();
        chapters.push(NamedDocument {
            name: name.clone(),
            document: PgnDocument::new(
                aggregate_headers(&title, &course.title, side, role),
                tree,
                "*",
            ),
        });
        merges.push(DocumentMerge { name, conflicts });
    }

    let mut full = Vec::new();
    for side in [RepertoireSide::Black, RepertoireSide::White] {
        let selected: Vec<_> = chapters
            .iter()
            .filter(|chapter| document_side(&chapter.document) == Some(side))
            .collect();
        if selected.is_empty() {
            continue;
        }
        let mut tree = MoveTree::from_position(Chess::default());
        let mut merger = MoveTreeMerger::new(side);
        let mut conflicts = Vec::new();
        for chapter in selected {
            conflicts.extend(
                merger
                    .merge(
                        &mut tree,
                        chapter.document.tree(),
                        SourceId::from(chapter.name.clone()),
                    )?
                    .conflicts,
            );
        }
        let side_name = side_text(side);
        let name = format!("{}-full", side_name.to_lowercase());
        full.push(NamedDocument {
            name: name.clone(),
            document: PgnDocument::new(
                aggregate_headers(
                    &format!("{side_name} Full"),
                    &course.title,
                    side,
                    RepertoireRole::Main,
                ),
                tree,
                "*",
            ),
        });
        merges.push(DocumentMerge { name, conflicts });
    }

    Ok(CourseDocuments {
        course_id: course.id.clone(),
        chapters,
        full,
        merges,
    })
}

fn aggregate_headers(
    title: &str,
    course_title: &str,
    side: RepertoireSide,
    role: RepertoireRole,
) -> Headers {
    let mut headers = Headers::new();
    headers.insert("Event", title);
    headers.insert("Site", "?");
    headers.insert("Date", "????.??.??");
    headers.insert("Round", "-");
    headers.insert("White", "Lines");
    headers.insert("Black", course_title);
    headers.insert("Result", "*");
    headers.insert("Chapter", title);
    headers.insert("SourceCourse", course_title);
    headers.insert("Orientation", side_text(side));
    headers.insert("RepertoireSide", side_text(side));
    headers.insert("RepertoireRole", role_text(role));
    headers
}

fn document_side(document: &PgnDocument) -> Option<RepertoireSide> {
    match document.headers().get("RepertoireSide") {
        Some("White") => Some(RepertoireSide::White),
        Some("Black") => Some(RepertoireSide::Black),
        _ => None,
    }
}

const fn side_text(side: RepertoireSide) -> &'static str {
    match side {
        RepertoireSide::White => "White",
        RepertoireSide::Black => "Black",
    }
}

const fn role_text(role: RepertoireRole) -> &'static str {
    match role {
        RepertoireRole::Main => "Main",
        RepertoireRole::Variant => "Variant",
    }
}

fn side_for_chapter(chapter: &str) -> Option<RepertoireSide> {
    match chapter.split('-').next() {
        Some(prefix) if prefix.eq_ignore_ascii_case("white") => Some(RepertoireSide::White),
        Some(prefix) if prefix.eq_ignore_ascii_case("black") => Some(RepertoireSide::Black),
        _ => None,
    }
}

fn role_for_chapter(chapter: &str) -> RepertoireRole {
    if chapter.ends_with("-variant") {
        RepertoireRole::Variant
    } else {
        RepertoireRole::Main
    }
}

fn compare_indexes(left: &str, right: &str) -> Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn title_from_slug(slug: &str) -> String {
    slug.replace('_', "-")
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let lowercase = part.to_lowercase();
            let mut characters = lowercase.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CourseDocumentError {
    MissingCourse,
    MissingRepertoireSide,
    UnsupportedCourseKind(CourseKind),
    UnknownChapter(String),
    Pgn(PgnError),
}

impl fmt::Display for CourseDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCourse => {
                formatter.write_str("course document generation requires course metadata")
            }
            Self::MissingRepertoireSide => {
                formatter.write_str("repertoire course metadata has no side")
            }
            Self::UnsupportedCourseKind(kind) => {
                write!(formatter, "cannot generate repertoire PGNs for {kind:?}")
            }
            Self::UnknownChapter(chapter) => write!(
                formatter,
                "raw source references unknown chapter {chapter:?}"
            ),
            Self::Pgn(error) => error.fmt(formatter),
        }
    }
}

impl Error for CourseDocumentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pgn(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PgnError> for CourseDocumentError {
    fn from(error: PgnError) -> Self {
        Self::Pgn(error)
    }
}

impl From<crate::tree::MergeError> for CourseDocumentError {
    fn from(error: crate::tree::MergeError) -> Self {
        Self::Pgn(PgnError::from(error))
    }
}

#[cfg(test)]
mod tests {
    use crate::{CourseMetadata, PgnWriter, SourceId};

    use super::{RawCourseLine, generate_course_documents};

    fn metadata() -> CourseMetadata {
        CourseMetadata::from_json(
            r#"{
                "schemaVersion": 1,
                "course": {"id":"sample", "title":"Sample Course", "kind":"repertoire"},
                "chapters": [
                    {"id":"white-second", "title":"Declared second"},
                    {"id":"black-first", "title":"Declared first"}
                ],
                "repertoire": {"side":"White"}
            }"#,
        )
        .unwrap()
    }

    fn raw(chapter: &str, index: &str, text: &str) -> RawCourseLine {
        RawCourseLine {
            source: SourceId::from(format!("{chapter}/{index}.raw")),
            chapter_id: chapter.to_owned(),
            index: index.to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn generates_declared_chapters_side_full_documents_and_course_order() {
        let documents = generate_course_documents(
            &metadata(),
            &[
                raw("black-first", "010", "1. e4c5"),
                raw("white-second", "010", "1. d4d5"),
                raw("white-second", "002", "1. d4Nf6"),
            ],
        )
        .unwrap();

        assert_eq!(
            documents
                .chapters
                .iter()
                .map(|document| document.name.as_str())
                .collect::<Vec<_>>(),
            ["white-second", "black-first"]
        );
        assert_eq!(
            documents
                .full
                .iter()
                .map(|document| document.name.as_str())
                .collect::<Vec<_>>(),
            ["black-full", "white-full"]
        );
        assert_eq!(documents.course_games().len(), 2);

        let white = &documents.chapters[0].document;
        assert_eq!(white.headers().get("Event"), Some("White Second"));
        assert_eq!(white.headers().get("White"), Some("Lines"));
        assert_eq!(white.headers().get("SourceCourse"), Some("Sample Course"));
        let rendered = PgnWriter::render(white).unwrap();
        assert!(rendered.ends_with("1. d4 d5 ( 1... Nf6 ) *"));

        let course_games: Vec<_> = documents.course_games().cloned().collect();
        let course = PgnWriter::render_documents(&course_games).unwrap();
        assert!(course.starts_with(b"[Event \"White Second\"]"));
        assert!(
            course
                .windows(21)
                .any(|window| window == b"[Event \"Black First\"]")
        );
    }

    #[test]
    fn reports_cross_line_repertoire_conflicts_without_dropping_opponent_branches() {
        let documents = generate_course_documents(
            &metadata(),
            &[
                raw("white-second", "001", "1. e4c5"),
                raw("white-second", "002", "1. d4d5"),
            ],
        )
        .unwrap();

        let chapter_merge = documents
            .merges
            .iter()
            .find(|merge| merge.name == "white-second")
            .unwrap();
        assert_eq!(chapter_merge.conflicts.len(), 1);
        assert_eq!(documents.chapters[0].document.tree().len(), 3);
    }
}
