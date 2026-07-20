//! PGN document construction and deterministic rendering.

mod writer;

use std::{error::Error, fmt};

use crate::{
    domain::{RepertoireRole, RepertoireSide},
    tree::{MergeError, MoveTree, RawTreeBuilder, TreeError},
};

pub use writer::PgnWriter;

/// A PGN header, retained in insertion order by [`Headers`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

/// Ordered PGN headers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Headers(Vec<Header>);

impl Headers {
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some(header) = self.0.iter_mut().find(|header| header.name == name) {
            header.value = value;
        } else {
            self.0.push(Header { name, value });
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|header| header.name == name)
            .map(|header| header.value.as_str())
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Header> {
        self.0.iter()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl IntoIterator for Headers {
    type Item = Header;
    type IntoIter = std::vec::IntoIter<Header>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Metadata needed to turn one raw repertoire into a standalone PGN document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SingleRawMetadata {
    pub course_title: String,
    pub event: String,
    pub chapter_slug: String,
    pub index: String,
    pub repertoire_side: RepertoireSide,
    pub repertoire_role: RepertoireRole,
}

/// A PGN boundary representation backed by an owned move tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PgnDocument {
    headers: Headers,
    tree: MoveTree,
    result: String,
}

impl PgnDocument {
    pub(crate) fn new(headers: Headers, tree: MoveTree, result: impl Into<String>) -> Self {
        Self {
            headers,
            tree,
            result: result.into(),
        }
    }

    /// Creates a PGN document from an already validated move tree.
    pub fn from_tree(
        headers: Headers,
        tree: MoveTree,
        result: impl Into<String>,
    ) -> Result<Self, PgnError> {
        tree.validate()?;
        Ok(Self::new(headers, tree, result))
    }

    #[must_use]
    pub const fn headers(&self) -> &Headers {
        &self.headers
    }

    #[must_use]
    pub const fn tree(&self) -> &MoveTree {
        &self.tree
    }

    #[must_use]
    pub fn result(&self) -> &str {
        &self.result
    }
}

/// Converts one raw text value without reading or writing the filesystem.
pub fn convert_single_raw(
    text: &str,
    metadata: &SingleRawMetadata,
) -> Result<PgnDocument, PgnError> {
    let built = RawTreeBuilder::new(metadata.repertoire_side).build(text)?;
    let mut headers = Headers::new();
    headers.insert("Event", &metadata.event);
    headers.insert("Site", "?");
    headers.insert("Date", "????.??.??");
    headers.insert("Round", &metadata.index);
    headers.insert("White", &metadata.index);
    headers.insert("Black", &metadata.course_title);
    headers.insert("Result", "*");
    headers.insert("Chapter", title_from_slug(&metadata.chapter_slug));
    headers.insert(
        "Orientation",
        match metadata.repertoire_side {
            RepertoireSide::White => "White",
            RepertoireSide::Black => "Black",
        },
    );
    headers.insert(
        "RepertoireSide",
        match metadata.repertoire_side {
            RepertoireSide::White => "White",
            RepertoireSide::Black => "Black",
        },
    );
    headers.insert(
        "RepertoireRole",
        match metadata.repertoire_role {
            RepertoireRole::Main => "Main",
            RepertoireRole::Quickstarter => "Quickstarter",
            RepertoireRole::Alternative => "Alternative",
            RepertoireRole::Variant => "Variant",
        },
    );

    Ok(PgnDocument {
        headers,
        tree: built.tree,
        result: "*".to_owned(),
    })
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
pub enum PgnError {
    Tree(TreeError),
    Merge(MergeError),
}

impl fmt::Display for PgnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tree(error) => error.fmt(formatter),
            Self::Merge(error) => error.fmt(formatter),
        }
    }
}

impl Error for PgnError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Tree(error) => Some(error),
            Self::Merge(error) => Some(error),
        }
    }
}

impl From<TreeError> for PgnError {
    fn from(error: TreeError) -> Self {
        Self::Tree(error)
    }
}

impl From<MergeError> for PgnError {
    fn from(error: MergeError) -> Self {
        Self::Merge(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{RepertoireRole, RepertoireSide};

    use super::{Headers, SingleRawMetadata, title_from_slug};

    #[test]
    fn headers_keep_insertion_order_when_updated() {
        let mut headers = Headers::new();
        headers.insert("Event", "First");
        headers.insert("Site", "Local");
        headers.insert("Event", "Updated");

        let headers: Vec<_> = headers
            .iter()
            .map(|header| (header.name.as_str(), header.value.as_str()))
            .collect();
        assert_eq!(headers, [("Event", "Updated"), ("Site", "Local")]);
    }

    #[test]
    fn slug_titles_match_the_python_boundary_behavior() {
        assert_eq!(title_from_slug("white-queen_pawn"), "White Queen Pawn");
        assert_eq!(title_from_slug("alpha--beta"), "Alpha Beta");
        assert_eq!(title_from_slug("WHITE-oPEN"), "White Open");
    }

    #[test]
    fn metadata_is_owned_for_filesystem_independent_conversion() {
        let metadata = SingleRawMetadata {
            course_title: "Course".to_owned(),
            event: "Chapter".to_owned(),
            chapter_slug: "white-open".to_owned(),
            index: "001".to_owned(),
            repertoire_side: RepertoireSide::White,
            repertoire_role: RepertoireRole::Main,
        };
        assert_eq!(metadata.index, "001");
    }
}
