use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

use crate::RepertoireSide;

const CURRENT_SCHEMA_VERSION: u64 = 1;

type Extensions = BTreeMap<String, Value>;

/// The metadata envelope shared by all supported course kinds.
///
/// A missing schema version identifies the deliberately supported legacy
/// format used by the Python converter. Version 1 manifests receive full
/// validation through [`CourseMetadata::from_json`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course: Option<CourseInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<ChapterMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classifications: Vec<CurriculumClassification>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repertoire: Option<RepertoireSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lichess: Option<LichessMetadata>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CourseMetadata {
    /// Parses metadata and applies version-specific semantic validation.
    pub fn from_json(json: &str) -> Result<Self, MetadataError> {
        let value: Value = serde_json::from_str(json).map_err(MetadataError::Json)?;
        let version = value.get("schemaVersion");
        match version {
            None => {}
            Some(Value::Number(number)) if number.as_u64() == Some(CURRENT_SCHEMA_VERSION) => {}
            Some(Value::Number(number)) => {
                return Err(MetadataError::UnsupportedSchemaVersion(number.to_string()));
            }
            Some(_) => {
                return Err(MetadataError::Invalid(
                    "schemaVersion must be a positive integer".into(),
                ));
            }
        }

        let metadata: Self = serde_json::from_value(value).map_err(MetadataError::Json)?;
        metadata.validate()?;
        Ok(metadata)
    }

    /// Returns whether this is an unversioned Python-compatible manifest.
    #[must_use]
    pub const fn is_legacy(&self) -> bool {
        self.schema_version.is_none()
    }

    /// Returns the course-wide repertoire side when one is configured.
    #[must_use]
    pub fn repertoire_side(&self) -> Option<RepertoireSide> {
        self.repertoire.as_ref().map(|settings| settings.side)
    }

    fn validate(&self) -> Result<(), MetadataError> {
        if self.is_legacy() {
            return Ok(());
        }

        let course = self.course.as_ref().ok_or_else(|| {
            MetadataError::Invalid("schema version 1 requires a course object".into())
        })?;
        require_text("course.id", &course.id)?;
        require_text("course.title", &course.title)?;

        if self.chapters.is_empty() {
            return Err(MetadataError::Invalid(
                "schema version 1 requires at least one chapter".into(),
            ));
        }

        let mut chapter_ids = std::collections::BTreeSet::new();
        for chapter in &self.chapters {
            require_text("chapters[].id", &chapter.id)?;
            require_text("chapters[].title", &chapter.title)?;
            if !chapter_ids.insert(&chapter.id) {
                return Err(MetadataError::Invalid(format!(
                    "duplicate chapter id {:?}",
                    chapter.id
                )));
            }
        }

        for classification in &self.classifications {
            require_text("classifications[].scheme", &classification.scheme)?;
            require_text("classifications[].level", &classification.level)?;
            require_text("classifications[].authority", &classification.authority)?;
        }

        if let Some(lichess) = &self.lichess {
            require_text("lichess.studyId", &lichess.study_id)?;
            for (chapter_id, mapping) in &lichess.chapters {
                if !chapter_ids.contains(chapter_id) {
                    return Err(MetadataError::Invalid(format!(
                        "Lichess mapping references unknown chapter {chapter_id:?}"
                    )));
                }
                require_text("lichess.chapters[].id", &mapping.id)?;
            }
        }

        match course.kind {
            CourseKind::Repertoire if self.repertoire.is_none() => Err(MetadataError::Invalid(
                "a repertoire course requires repertoire settings".into(),
            )),
            CourseKind::ProblemSet if self.repertoire.is_some() => Err(MetadataError::Invalid(
                "problem-set courses cannot contain repertoire settings".into(),
            )),
            _ => Ok(()),
        }
    }
}

fn require_text(field: &str, value: &str) -> Result<(), MetadataError> {
    if value.trim().is_empty() {
        Err(MetadataError::Invalid(format!("{field} cannot be empty")))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseInfo {
    pub id: String,
    pub title: String,
    pub kind: CourseKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceAttribution>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CourseKind {
    Repertoire,
    ProblemSet,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAttribution {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterMetadata {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurriculumClassification {
    pub scheme: String,
    pub level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lesson: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    pub authority: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireSettings {
    #[serde(deserialize_with = "deserialize_repertoire_side")]
    pub side: RepertoireSide,
    #[serde(flatten)]
    pub extensions: Extensions,
}

fn deserialize_repertoire_side<'de, D>(deserializer: D) -> Result<RepertoireSide, D::Error>
where
    D: Deserializer<'de>,
{
    let side = String::deserialize(deserializer)?;
    if side.eq_ignore_ascii_case("white") {
        Ok(RepertoireSide::White)
    } else if side.eq_ignore_ascii_case("black") {
        Ok(RepertoireSide::Black)
    } else {
        Err(de::Error::unknown_variant(&side, &["White", "Black"]))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LichessMetadata {
    pub study_id: String,
    #[serde(default)]
    pub chapters: BTreeMap<String, LichessChapter>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LichessChapter {
    pub id: String,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Debug)]
pub enum MetadataError {
    Json(serde_json::Error),
    UnsupportedSchemaVersion(String),
    Invalid(String),
}

impl fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid course metadata JSON: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "unsupported course metadata schema version {version}"
                )
            }
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl Error for MetadataError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::UnsupportedSchemaVersion(_) | Self::Invalid(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CourseKind, CourseMetadata, MetadataError};
    use crate::RepertoireSide;

    const VERSIONED_REPERTOIRE: &str = r#"{
        "schemaVersion": 1,
        "course": {
            "id": "synthetic-black-repertoire",
            "title": "Synthetic Black Repertoire",
            "kind": "repertoire",
            "source": {
                "url": "https://example.invalid/synthetic",
                "attribution": "Synthetic fixture",
                "license": "CC0-1.0"
            }
        },
        "chapters": [{"id": "defence", "title": "Example Defence"}],
        "repertoire": {"side": "Black"},
        "lichess": {
            "studyId": "study-001",
            "chapters": {"defence": {"id": "chapter-001"}}
        }
    }"#;

    const SYNTHETIC_PROBLEM_SET: &str = r#"{
        "schemaVersion": 1,
        "course": {
            "id": "synthetic-problems",
            "title": "Synthetic Problems",
            "kind": "problem-set",
            "source": {
                "url": "https://example.invalid/synthetic",
                "attribution": "Synthetic fixture",
                "license": "CC0-1.0"
            }
        },
        "chapters": [{"id": "mate-in-one", "title": "Mate in One"}],
        "problemSet": {
            "problems": [{
                "id": "synthetic-mate",
                "sourceId": "synthetic",
                "chapterId": "mate-in-one",
                "start": {"fen": "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1"},
                "objective": {"kind": "checkmate", "moves": 1}
            }]
        }
    }"#;

    #[test]
    fn parses_and_validates_versioned_repertoire_metadata() {
        let metadata =
            CourseMetadata::from_json(VERSIONED_REPERTOIRE).expect("synthetic fixture is valid");

        assert!(!metadata.is_legacy());
        assert_eq!(metadata.repertoire_side(), Some(RepertoireSide::Black));
        assert_eq!(metadata.chapters[0].id, "defence");
        assert_eq!(
            metadata
                .lichess
                .as_ref()
                .expect("lichess mappings")
                .chapters["defence"]
                .id,
            "chapter-001"
        );
    }

    #[test]
    fn accepts_existing_unversioned_python_metadata() {
        let metadata = CourseMetadata::from_json(r#"{"repertoire":{"side":"bLaCk"}}"#)
            .expect("legacy metadata remains supported");

        assert!(metadata.is_legacy());
        assert_eq!(metadata.repertoire_side(), Some(RepertoireSide::Black));
    }

    #[test]
    fn rejects_unsupported_versions_with_a_specific_error() {
        let error = CourseMetadata::from_json(r#"{"schemaVersion":2}"#)
            .expect_err("version 2 is not supported");

        assert!(
            matches!(error, MetadataError::UnsupportedSchemaVersion(version) if version == "2")
        );
    }

    #[test]
    fn validates_required_fields_and_unique_chapters() {
        let error = CourseMetadata::from_json(
            r#"{
                "schemaVersion": 1,
                "course": {"id":"course", "title":"Course", "kind":"repertoire"},
                "chapters": [
                    {"id":"same", "title":"One"},
                    {"id":"same", "title":"Two"}
                ],
                "repertoire": {"side":"White"}
            }"#,
        )
        .expect_err("duplicate chapter identifiers are ambiguous");

        assert_eq!(error.to_string(), "duplicate chapter id \"same\"");
    }

    #[test]
    fn rejects_lichess_mappings_for_undeclared_chapters() {
        let error = CourseMetadata::from_json(
            r#"{
                "schemaVersion": 1,
                "course": {"id":"course", "title":"Course", "kind":"repertoire"},
                "chapters": [{"id":"declared", "title":"Declared"}],
                "repertoire": {"side":"White"},
                "lichess": {
                    "studyId":"study",
                    "chapters":{"missing":{"id":"chapter"}}
                }
            }"#,
        )
        .expect_err("publishing mappings must use stable declared chapter ids");

        assert_eq!(
            error.to_string(),
            "Lichess mapping references unknown chapter \"missing\""
        );
    }

    #[test]
    fn future_problem_set_keeps_position_and_integer_objective_extensions() {
        let metadata = CourseMetadata::from_json(SYNTHETIC_PROBLEM_SET)
            .expect("synthetic fixture fits the common envelope");

        assert_eq!(
            metadata.course.as_ref().expect("course").kind,
            CourseKind::ProblemSet
        );
        let problems = &metadata.extensions["problemSet"]["problems"];
        assert_eq!(
            problems[0]["start"]["fen"],
            json!("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1")
        );
        assert_eq!(problems[0]["objective"]["moves"], json!(1));

        let round_trip = serde_json::to_value(&metadata).expect("metadata serializes");
        assert_eq!(round_trip["problemSet"]["problems"], *problems);
    }
}
