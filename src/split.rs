//! Exporting one JSON file per figure, grouped into sub-folders.
//!
//! This is what the `folder` field is really for: instead of a single file you
//! can get a tree like
//!
//! ```text
//! out/
//!   faces/face_1.json
//!   eyes/eye_1.json
//!   point_1.json
//! ```

use crate::json_io::{self, JsonError};
use crate::model::AnnotationSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// One file to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitEntry {
    /// Path relative to the output root.
    pub relative_path: PathBuf,
    /// File contents.
    pub contents: String,
}

/// Something went wrong while writing the files.
#[derive(Debug)]
pub enum SplitError {
    /// A figure could not be serialised.
    Serialise(JsonError),
    /// A file could not be written.
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
}

impl SplitError {
    /// Renders the message in `lang`.
    #[must_use]
    pub fn message(&self, lang: crate::i18n::Lang) -> String {
        match self {
            Self::Serialise(error) => {
                let detail = error.message(lang);
                match lang {
                    crate::i18n::Lang::Ru => format!("не удалось сериализовать: {detail}"),
                    crate::i18n::Lang::En => format!("could not serialise: {detail}"),
                }
            }
            Self::Io { path, source } => {
                let path = path.display().to_string();
                format!("{path}: {source}")
            }
        }
    }
}

impl fmt::Display for SplitError {
    /// Falls back to Russian, the original language.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(crate::i18n::Lang::Ru))
    }
}

impl std::error::Error for SplitError {}

impl From<JsonError> for SplitError {
    fn from(value: JsonError) -> Self {
        Self::Serialise(value)
    }
}

/// Replaces characters that are not safe in a file name with `_`.
///
/// Keeps letters, digits, `_`, `-`, `.`, spaces and everything above U+007F so
/// that Cyrillic names survive.
#[must_use]
pub fn sanitise(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim().to_owned();
    if trimmed.is_empty() {
        "unnamed".to_owned()
    } else {
        trimmed
    }
}

/// Turns a user-entered folder into a safe relative directory path.
///
/// Absolute paths, `..` components and Windows drive letters are dropped.
#[must_use]
pub fn sanitise_folder(folder: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in folder.split(['/', '\\']) {
        let trimmed = part.trim();
        // Skip separators, traversal attempts and anything that sanitises away.
        if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
            continue;
        }
        if Path::new(trimmed).components().any(|c| {
            matches!(
                c,
                Component::RootDir | Component::Prefix(_) | Component::ParentDir
            )
        }) {
            continue;
        }
        out.push(sanitise(trimmed));
    }
    out
}

/// Builds the list of files a split export would produce.
///
/// Names are made unique by appending `_2`, `_3`, ... when two figures would
/// land in the same file.
#[must_use]
pub fn plan(set: &AnnotationSet, precision: u32) -> Vec<SplitEntry> {
    let mut used: Vec<String> = Vec::new();
    let mut entries = Vec::with_capacity(set.len());
    for annotation in set {
        let folder = sanitise_folder(annotation.folder());
        let base = sanitise(&annotation.display_name());
        let mut candidate = format!("{base}.json");
        let mut counter = 2;
        while used.contains(&candidate) {
            candidate = format!("{base}_{counter}.json");
            counter += 1;
        }
        used.push(candidate.clone());
        let contents = json_io::to_string_pretty(&clone_single(annotation), None, precision)
            .unwrap_or_default();
        entries.push(SplitEntry {
            relative_path: folder.join(&candidate),
            contents,
        });
    }
    entries
}

/// Builds an [`AnnotationSet`] containing only `annotation`, preserving its id.
fn clone_single(annotation: &crate::model::Annotation) -> AnnotationSet {
    let mut set = AnnotationSet::new();
    set.insert(annotation.clone());
    set
}

/// Writes one JSON file per figure under `root`, creating sub-folders.
///
/// Returns how many files were written.
///
/// # Errors
///
/// Returns [`SplitError`] when a file cannot be serialised or written.
pub fn write_split_json(
    root: &Path,
    set: &AnnotationSet,
    precision: u32,
) -> Result<usize, SplitError> {
    let entries = plan(set, precision);
    for entry in &entries {
        let path = root.join(&entry.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| SplitError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(&path, &entry.contents).map_err(|source| SplitError::Io { path, source })?;
    }
    Ok(entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use egui::pos2;

    fn sample() -> AnnotationSet {
        let mut set = AnnotationSet::new();
        set.add_point(pos2(1.0, 2.0), None);
        set.add_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), Some("faces"));
        set.add_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), Some("eyes/left"));
        set
    }

    #[test]
    fn sanitise_keeps_cyrillic_and_replaces_separators() {
        assert_eq!(sanitise("левый глаз"), "левый глаз");
        assert_eq!(sanitise("a/b\\c"), "a_b_c");
        assert_eq!(sanitise("  ..  "), "unnamed");
        assert_eq!(sanitise("a\u{0}b"), "a_b");
        assert_eq!(sanitise("норма:name"), "норма_name");
    }

    #[test]
    fn sanitise_folder_drops_traversal() {
        assert_eq!(sanitise_folder("faces"), PathBuf::from("faces"));
        assert_eq!(sanitise_folder("a/b"), PathBuf::from("a").join("b"));
        assert_eq!(sanitise_folder(".."), PathBuf::new());
        assert_eq!(sanitise_folder("/etc"), PathBuf::from("etc"));
        assert_eq!(sanitise_folder(""), PathBuf::new());
        assert!(sanitise_folder("../x").is_relative());
    }

    #[test]
    fn plan_creates_one_file_per_figure() {
        let entries = plan(&sample(), 2);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].relative_path, PathBuf::from("point_1.json"));
        assert_eq!(
            entries[1].relative_path,
            PathBuf::from("faces").join("rect_2.json")
        );
        assert_eq!(
            entries[2].relative_path,
            PathBuf::from("eyes").join("left").join("rect_3.json")
        );
    }

    #[test]
    fn each_file_holds_exactly_one_figure_and_keeps_its_id() {
        let entries = plan(&sample(), 2);
        let parsed = json_io::from_str(&entries[1].contents).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.get(2).unwrap().display_name(), "rect_2");
    }

    #[test]
    fn duplicate_names_are_disambiguated() {
        let mut set = AnnotationSet::new();
        set.add_point(pos2(0.0, 0.0), Some("a"));
        set.add_point(pos2(1.0, 1.0), Some("a"));
        set.get_mut(1).unwrap().set_name("same");
        set.get_mut(2).unwrap().set_name("same");
        let entries = plan(&set, 2);
        assert_eq!(
            entries[0].relative_path,
            PathBuf::from("a").join("same.json")
        );
        assert_eq!(
            entries[1].relative_path,
            PathBuf::from("a").join("same_2.json")
        );
    }

    #[test]
    fn write_creates_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        let count = write_split_json(dir.path(), &sample(), 2).unwrap();
        assert_eq!(count, 3);
        assert!(dir.path().join("point_1.json").is_file());
        assert!(dir.path().join("faces/rect_2.json").is_file());
        assert!(dir.path().join("eyes/left/rect_3.json").is_file());
        let text = std::fs::read_to_string(dir.path().join("faces/rect_2.json")).unwrap();
        assert_eq!(json_io::from_str(&text).unwrap().len(), 1);
    }

    #[test]
    fn errors_are_readable_in_both_languages() {
        use crate::i18n::Lang;
        let error = SplitError::Serialise(crate::error::Error::Figure {
            index: 0,
            problem: crate::error::Problem::MissingWidth,
        });
        assert!(error.message(Lang::Ru).contains("width"), "{error}");
        assert!(error.message(Lang::En).contains("width"), "{error}");
        assert_ne!(error.message(Lang::Ru), error.message(Lang::En));
        assert_eq!(error.to_string(), error.message(Lang::Ru));
    }

    #[test]
    fn write_reports_errors() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("faces");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let error = write_split_json(dir.path(), &sample(), 2).unwrap_err();
        assert!(matches!(error, SplitError::Io { .. }), "{error:?}");
        assert!(
            error.to_string().contains('t'),
            "сообщение должно называть путь: {error}"
        );
    }

    #[test]
    fn empty_set_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            write_split_json(dir.path(), &AnnotationSet::new(), 2).unwrap(),
            0
        );
        assert!(plan(&AnnotationSet::new(), 2).is_empty());
    }
}
