//! The on-disk JSON format: building, serialising and parsing it.
//!
//! See `docs/JSON_FORMAT.md` for the full specification. The short version:
//!
//! ```json
//! {
//!   "format": "picture-measure",
//!   "version": 1,
//!   "image": { "path": "/tmp/cat.png", "name": "cat.png", "width": 640, "height": 480 },
//!   "settings": { "coordinate_system": "pixel", "origin": "top-left" },
//!   "folders": ["faces"],
//!   "counts": { "total": 2, "points": 1, "rects": 1 },
//!   "annotations": [
//!     { "type": "point", "name": "point_1", "id": 1, "folder": "faces", "x": 12, "y": 34 },
//!     { "type": "rect",  "name": "rect_2",  "id": 2, "x": 1, "y": 2, "width": 30, "height": 40 }
//!   ]
//! }
//! ```

use crate::geometry::{Rect, round_to};
use crate::model::{Annotation, AnnotationSet, Geometry, ShapeKind};
use crate::model::{POINT_COLOR, RECT_COLOR, SELECTION_COLOR};
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// Value of the `format` field written by this program.
pub const FORMAT_NAME: &str = "picture-measure";
/// Value of the `version` field written by this program.
pub const FORMAT_VERSION: u32 = 1;

/// A number that serialises as an integer when it has no fractional part.
///
/// Keeps the file readable (`12` instead of `12.0`) and friendly to consumers
/// that expect integers, such as most YOLO tooling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Number {
    /// Integer value.
    Int(i64),
    /// Fractional value.
    Float(f64),
}

impl Number {
    /// Numeric value as `f64`.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        match self {
            Self::Int(v) => v as f64,
            Self::Float(v) => v,
        }
    }

    /// Numeric value as `f32`.
    #[must_use]
    pub fn as_f32(self) -> f32 {
        self.as_f64() as f32
    }
}

impl From<f32> for Number {
    fn from(value: f32) -> Self {
        Self::from(f64::from(value))
    }
}

impl From<f64> for Number {
    fn from(value: f64) -> Self {
        if value.fract() == 0.0 && value.abs() < 9.0e15 {
            Self::Int(value as i64)
        } else {
            Self::Float(value)
        }
    }
}

/// Rounds `value` to `precision` decimals and converts it to a [`Number`].
#[must_use]
pub fn number(value: f32, precision: u32) -> Number {
    Number::from(round_to(f64::from(value), precision))
}

/// Parses `#rrggbb` / `#rrggbbaa` into a colour, if possible.
#[must_use]
pub fn parse_color(text: &str) -> Option<Color32> {
    let hex = text.trim().strip_prefix('#')?;
    let bytes = match hex.len() {
        6 => {
            let rgb = u32::from_str_radix(hex, 16).ok()?;
            [
                ((rgb >> 16) & 0xff) as u8,
                ((rgb >> 8) & 0xff) as u8,
                (rgb & 0xff) as u8,
                255,
            ]
        }
        8 => {
            let rgba = u64::from_str_radix(hex, 16).ok()?;
            [
                ((rgba >> 24) & 0xff) as u8,
                ((rgba >> 16) & 0xff) as u8,
                ((rgba >> 8) & 0xff) as u8,
                (rgba & 0xff) as u8,
            ]
        }
        _ => return None,
    };
    Some(Color32::from_rgba_unmultiplied(
        bytes[0], bytes[1], bytes[2], bytes[3],
    ))
}

/// Formats a colour as `#rrggbb`.
#[must_use]
pub fn format_color(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// Information about the annotated image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Path the image was loaded from (empty for drag-and-dropped buffers).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// File name including the extension.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Width of the source image in pixels.
    pub width: u32,
    /// Height of the source image in pixels.
    pub height: u32,
}

impl ImageInfo {
    /// Builds the info from a path and its pixel size.
    #[must_use]
    pub fn from_path(path: &Path, width: u32, height: u32) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            name: path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            width,
            height,
        }
    }
}

/// Coordinate conventions, written for the benefit of downstream tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    /// Always `"pixel"`.
    #[serde(default = "default_coordinate_system")]
    pub coordinate_system: String,
    /// Always `"top-left"`.
    #[serde(default = "default_origin")]
    pub origin: String,
}

fn default_coordinate_system() -> String {
    "pixel".to_owned()
}

fn default_origin() -> String {
    "top-left".to_owned()
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            coordinate_system: default_coordinate_system(),
            origin: default_origin(),
        }
    }
}

/// Number of figures of each kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountEntry {
    /// Total number of annotations.
    pub total: usize,
    /// Number of points.
    pub points: usize,
    /// Number of rectangles.
    pub rects: usize,
}

impl From<crate::model::Counts> for CountEntry {
    fn from(c: crate::model::Counts) -> Self {
        Self {
            total: c.total,
            points: c.points,
            rects: c.rects,
        }
    }
}

/// One figure as written to disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationEntry {
    /// `"point"` or `"rect"`. Stored raw so that a typo produces a helpful
    /// error instead of a serde message.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Resolved name: the user's name, or the auto-generated `point_N` /
    /// `rect_N`. Missing in hand-written files.
    #[serde(default)]
    pub name: String,
    /// Stable figure number; also the `N` of the auto-generated name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// Sub-folder ("class"); omitted when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    /// Display colour as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// X coordinate of the point, or left edge of the rectangle.
    pub x: Number,
    /// Y coordinate of the point, or top edge of the rectangle.
    pub y: Number,
    /// Width; present only for rectangles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Number>,
    /// Height; present only for rectangles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Number>,
}

/// The whole annotation file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationFile {
    /// Always [`FORMAT_NAME`] for files written by this program.
    #[serde(default = "default_format")]
    pub format: String,
    /// File format version, see [`FORMAT_VERSION`].
    #[serde(default = "default_version")]
    pub version: u32,
    /// Image the annotations belong to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageInfo>,
    /// Coordinate conventions.
    #[serde(default)]
    pub settings: ExportSettings,
    /// Sorted list of every non-empty folder, for convenience.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    /// Figure counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<CountEntry>,
    /// The figures themselves.
    #[serde(default)]
    pub annotations: Vec<AnnotationEntry>,
}

fn default_format() -> String {
    FORMAT_NAME.to_owned()
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

/// Everything that can go wrong while reading or writing annotations.
#[derive(Debug)]
pub enum JsonError {
    /// The text is not valid JSON.
    Syntax(serde_json::Error),
    /// The JSON is valid but does not describe an annotation file.
    Structure(String),
    /// A specific figure could not be converted.
    Figure {
        /// Zero-based index inside the `annotations` array.
        index: usize,
        /// Human-readable reason.
        reason: String,
    },
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(e) => write!(f, "некорректный JSON: {e}"),
            Self::Structure(m) => write!(f, "неподдерживаемая структура файла: {m}"),
            Self::Figure { index, reason } => {
                write!(f, "фигура №{index}: {reason}")
            }
        }
    }
}

impl std::error::Error for JsonError {}

impl From<serde_json::Error> for JsonError {
    fn from(value: serde_json::Error) -> Self {
        Self::Syntax(value)
    }
}

impl AnnotationEntry {
    /// Converts a runtime annotation into a file entry.
    #[must_use]
    pub fn from_annotation(annotation: &Annotation, precision: u32) -> Self {
        let (x, y, width, height) = match annotation.geometry {
            Geometry::Point(p) => (number(p.x, precision), number(p.y, precision), None, None),
            Geometry::Rect(r) => (
                number(r.min.x, precision),
                number(r.min.y, precision),
                Some(number(r.width(), precision)),
                Some(number(r.height(), precision)),
            ),
        };
        Self {
            kind: Some(annotation.kind().as_str().to_owned()),
            name: annotation.display_name(),
            id: Some(annotation.id),
            folder: annotation.folder().to_owned(),
            color: Some(format_color(annotation.color)),
            x,
            y,
            width,
            height,
        }
    }

    /// Converts a file entry back into a runtime annotation.
    ///
    /// `fallback_id` is used when the entry has no `id`.
    pub fn to_annotation(&self, fallback_id: u32, index: usize) -> Result<Annotation, JsonError> {
        let x = self.x.as_f32();
        let y = self.y.as_f32();
        let id = self.id.unwrap_or(fallback_id);

        let kind = self
            .kind
            .as_deref()
            .and_then(ShapeKind::parse)
            .or_else(|| {
                if self.width.is_some() || self.height.is_some() {
                    Some(ShapeKind::Rect)
                } else {
                    None
                }
            })
            .ok_or_else(|| JsonError::Figure {
                index,
                reason: match self.kind.as_deref() {
                    Some(other) => format!(
                        "неизвестный тип фигуры \"{other}\" (ожидается \"point\" или \"rect\")"
                    ),
                    None => "не указан тип фигуры (ожидается \"point\" или \"rect\")".to_owned(),
                },
            })?;

        let geometry = match kind {
            ShapeKind::Point => Geometry::Point(egui::pos2(x, y)),
            ShapeKind::Rect => {
                let width = self.width.ok_or_else(|| JsonError::Figure {
                    index,
                    reason: "у прямоугольника отсутствует поле \"width\"".to_owned(),
                })?;
                let height = self.height.ok_or_else(|| JsonError::Figure {
                    index,
                    reason: "у прямоугольника отсутствует поле \"height\"".to_owned(),
                })?;
                let (w, h) = (width.as_f32(), height.as_f32());
                if w < 0.0 || h < 0.0 {
                    return Err(JsonError::Figure {
                        index,
                        reason: format!("отрицательный размер прямоугольника ({w}, {h})"),
                    });
                }
                Geometry::Rect(Rect::from_xywh(x, y, w, h))
            }
        };

        let mut annotation = Annotation::new(id, geometry);
        // Only a *custom* name is kept: a file that spells out `point_1`
        // must not turn into a user-defined name that then stops following
        // the figure number.
        let auto_name = annotation.default_name();
        if !self.name.trim().is_empty() && self.name.trim() != auto_name {
            annotation.set_name(&self.name);
        }
        annotation.set_folder(&self.folder);
        if let Some(color) = self.color.as_deref().and_then(parse_color) {
            annotation.color = color;
        }
        Ok(annotation)
    }
}

/// Builds the file representation of an annotation set.
#[must_use]
pub fn build_file(set: &AnnotationSet, image: Option<ImageInfo>, precision: u32) -> AnnotationFile {
    AnnotationFile {
        format: FORMAT_NAME.to_owned(),
        version: FORMAT_VERSION,
        image,
        settings: ExportSettings::default(),
        folders: set.folders(),
        counts: Some(set.counts().into()),
        annotations: set
            .iter()
            .map(|a| AnnotationEntry::from_annotation(a, precision))
            .collect(),
    }
}

/// Serialises an annotation set to pretty-printed JSON.
pub fn to_string_pretty(
    set: &AnnotationSet,
    image: Option<ImageInfo>,
    precision: u32,
) -> Result<String, JsonError> {
    let file = build_file(set, image, precision);
    serde_json::to_string_pretty(&file).map_err(JsonError::Syntax)
}

/// Serialises an annotation set to compact, single-line JSON.
pub fn to_string_compact(
    set: &AnnotationSet,
    image: Option<ImageInfo>,
    precision: u32,
) -> Result<String, JsonError> {
    let file = build_file(set, image, precision);
    serde_json::to_string(&file).map_err(JsonError::Syntax)
}

/// Parses an annotation file.
///
/// Accepts the full document produced by this program as well as a bare array
/// of figures, which is convenient for hand-written files.
pub fn from_str(text: &str) -> Result<AnnotationSet, JsonError> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let entries: Vec<AnnotationEntry> = match value {
        serde_json::Value::Array(items) => serde_json::from_value(serde_json::Value::Array(items))?,
        serde_json::Value::Object(mut map) => {
            let annotations = map.remove("annotations");
            match annotations {
                Some(serde_json::Value::Array(items)) => {
                    serde_json::from_value(serde_json::Value::Array(items))?
                }
                Some(other) => {
                    return Err(JsonError::Structure(format!(
                        "поле \"annotations\" должно быть массивом, получено {other}"
                    )));
                }
                None => {
                    return Err(JsonError::Structure(
                        "в объекте нет поля \"annotations\"".to_owned(),
                    ));
                }
            }
        }
        other => {
            return Err(JsonError::Structure(format!(
                "ожидался объект или массив, получено {other}"
            )));
        }
    };
    from_entries(&entries)
}

/// Converts already-parsed entries into an annotation set.
pub fn from_entries(entries: &[AnnotationEntry]) -> Result<AnnotationSet, JsonError> {
    let mut set = AnnotationSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let fallback_id = index as u32 + 1;
        set.insert(entry.to_annotation(fallback_id, index)?);
    }
    Ok(set)
}

/// Extracts the image metadata stored in a file, if any.
pub fn peek_image_info(text: &str) -> Option<ImageInfo> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::from_value(value.get("image")?.clone()).ok()
}

/// Colours offered in the colour picker.
#[must_use]
pub fn palette() -> Vec<Color32> {
    vec![
        POINT_COLOR,
        RECT_COLOR,
        SELECTION_COLOR,
        Color32::from_rgb(120, 230, 120),
        Color32::from_rgb(255, 160, 80),
        Color32::from_rgb(180, 130, 255),
        Color32::from_rgb(255, 255, 255),
        Color32::from_rgb(160, 160, 160),
    ]
}

/// Unique, sorted list of folder names in `set`.
#[must_use]
pub fn folder_names(set: &AnnotationSet) -> Vec<String> {
    set.folders()
}

/// Suggested JSON path for an image that has not been saved yet.
#[must_use]
pub fn default_json_path(image_path: Option<&Path>) -> String {
    match image_path {
        Some(path) => {
            let mut out = path.to_path_buf();
            out.set_extension("json");
            out.to_string_lossy().into_owned()
        }
        None => "annotations.json".to_owned(),
    }
}

/// File extensions the image loader accepts.
#[must_use]
pub fn is_image_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "bmp"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AnnotationSet;
    use egui::pos2;

    fn sample() -> AnnotationSet {
        let mut set = AnnotationSet::new();
        set.add_point(pos2(12.4, 34.6), None);
        set.add_rect(Rect::from_xywh(1.0, 2.0, 30.0, 40.0), Some("faces"));
        set.add_rect(Rect::from_xywh(1.0, 2.0, 30.0, 40.0), Some("eyes"));
        set
    }

    #[test]
    fn numbers_serialise_compactly() {
        assert!(matches!(Number::from(12.0f32), Number::Int(12)));
        assert!(matches!(Number::from(12.5f32), Number::Float(_)));
        assert_eq!(Number::from(-3.0f32).as_f64(), -3.0);
        assert_eq!(Number::Float(1.25).as_f32(), 1.25);
        assert_eq!(number(12.3456, 2).as_f64(), 12.35);
        assert_eq!(number(12.3456, 0).as_f64(), 12.0);
    }

    #[test]
    fn colours_round_trip() {
        let c = Color32::from_rgb(18, 52, 86);
        assert_eq!(format_color(c), "#123456");
        assert_eq!(parse_color("#123456"), Some(c));
        assert_eq!(parse_color("#123456ff"), Some(c));
        assert_eq!(parse_color("red"), None);
        assert_eq!(parse_color("#fff"), None);
        assert_eq!(parse_color("#gggggg"), None);
    }

    #[test]
    fn export_shape() {
        let set = sample();
        let json = to_string_pretty(&set, None, 2).unwrap();
        assert!(json.contains("\"format\": \"picture-measure\""));
        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("\"type\": \"point\""));
        assert!(json.contains("\"name\": \"point_1\""));
        assert!(json.contains("\"name\": \"rect_2\""));
        assert!(json.contains("\"folder\": \"faces\""));
        assert!(json.contains("\"total\": 3"));
        assert!(json.contains("\"point_1\""), "{json}");

        let file: AnnotationFile = serde_json::from_str(&json).unwrap();
        assert_eq!(file.format, FORMAT_NAME);
        assert_eq!(file.version, FORMAT_VERSION);
        assert_eq!(file.folders, vec!["eyes".to_owned(), "faces".to_owned()]);
        assert_eq!(file.counts.unwrap().points, 1);
        assert_eq!(file.counts.unwrap().rects, 2);
        assert_eq!(file.annotations.len(), 3);
        assert!(file.annotations[0].width.is_none());
        assert_eq!(file.annotations[1].width.unwrap().as_f64(), 30.0);
        assert_eq!(file.settings.coordinate_system, "pixel");
        assert_eq!(file.settings.origin, "top-left");
    }

    #[test]
    fn coordinates_are_integers_when_whole() {
        let mut set = AnnotationSet::new();
        set.add_point(pos2(10.0, 20.0), None);
        let json = to_string_compact(&set, None, 2).unwrap();
        assert!(json.contains("\"x\":10,\"y\":20"), "{json}");
    }

    #[test]
    fn round_trip_preserves_everything() {
        let mut set = sample();
        set.get_mut(3).unwrap().set_name("left_eye");
        let json = to_string_pretty(&set, None, 3).unwrap();
        let restored = from_str(&json).unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored.items(), set.items());
        assert_eq!(restored.peek_next_id(), 4);
        assert_eq!(restored.get(3).unwrap().display_name(), "left_eye");
        assert_eq!(restored.get(2).unwrap().folder(), "faces");
    }

    #[test]
    fn auto_names_stay_auto_after_a_round_trip() {
        let set = sample();
        let restored = from_str(&to_string_pretty(&set, None, 2).unwrap()).unwrap();
        assert!(!restored.get(1).unwrap().has_custom_name());
        assert!(!restored.get(2).unwrap().has_custom_name());
        assert_eq!(restored.get(1).unwrap().display_name(), "point_1");
        // Renumbering after the round trip must not collide with anything.
        let mut restored = restored;
        restored.renumber();
        assert_eq!(restored.get(1).unwrap().display_name(), "point_1");
    }

    #[test]
    fn precision_is_respected() {
        let mut set = AnnotationSet::new();
        set.add_point(pos2(10.123456, 20.987654), None);
        let json = to_string_pretty(&set, None, 2).unwrap();
        assert!(json.contains("10.12"), "{json}");
        assert!(json.contains("20.99"), "{json}");
        let restored = from_str(&json).unwrap();
        assert!((restored.get(1).unwrap().geometry.as_point().unwrap().x - 10.12).abs() < 1e-5);
    }

    #[test]
    fn image_info_is_written_and_read() {
        let info = ImageInfo::from_path(Path::new("/tmp/pics/cat.png"), 640, 480);
        let set = sample();
        let json = to_string_pretty(&set, Some(info.clone()), 2).unwrap();
        assert!(json.contains("\"name\": \"cat.png\""));
        assert_eq!(peek_image_info(&json), Some(info));
        assert_eq!(peek_image_info("not json"), None);
        assert_eq!(peek_image_info("{}"), None);
    }

    #[test]
    fn accepts_a_bare_array() {
        let json = r#"[
            {"type": "point", "name": "a", "x": 1, "y": 2},
            {"type": "rect", "name": "b", "x": 3, "y": 4, "width": 5, "height": 6, "folder": "z"}
        ]"#;
        let set = from_str(json).unwrap();
        assert_eq!(set.len(), 2);
        assert_eq!(set.get(1).unwrap().display_name(), "a");
        assert_eq!(set.get(2).unwrap().folder(), "z");
        assert_eq!(set.peek_next_id(), 3);
    }

    #[test]
    fn missing_name_falls_back_to_the_default() {
        let set = from_str(r#"[{"type":"point","x":1,"y":2}]"#).unwrap();
        assert_eq!(set.get(1).unwrap().display_name(), "point_1");
    }

    #[test]
    fn missing_type_is_inferred_from_the_size() {
        let set = from_str(r#"[{"x":1,"y":2,"width":3,"height":4}]"#).unwrap();
        assert_eq!(set.get(1).unwrap().kind(), ShapeKind::Rect);
    }

    #[test]
    fn ids_from_the_file_are_kept_and_drive_the_next_id() {
        let set = from_str(r#"{"annotations":[{"id":7,"type":"point","name":"x","x":0,"y":0}]}"#)
            .unwrap();
        assert_eq!(set.peek_next_id(), 8);
        assert_eq!(set.get(7).unwrap().display_name(), "x");
    }

    #[test]
    fn rectangles_must_have_a_size() {
        let err = from_str(r#"[{"type":"rect","name":"x","x":0,"y":0}]"#).unwrap_err();
        assert!(matches!(err, JsonError::Figure { index: 0, .. }), "{err:?}");
        assert!(err.to_string().contains("width"), "{err}");

        let err = from_str(r#"[{"type":"rect","name":"x","x":0,"y":0,"width":3}]"#).unwrap_err();
        assert!(err.to_string().contains("height"), "{err}");
    }

    #[test]
    fn unknown_type_is_reported() {
        let err = from_str(r#"[{"type":"polygon","x":0,"y":0}]"#).unwrap_err();
        assert!(err.to_string().contains("неизвестный тип"), "{err}");
        let err = from_str(r#"[{"x":0,"y":0}]"#).unwrap_err();
        assert!(err.to_string().contains("не указан тип"), "{err}");
    }

    #[test]
    fn negative_size_is_reported() {
        let err = from_str(r#"[{"type":"rect","x":0,"y":0,"width":-3,"height":4}]"#).unwrap_err();
        assert!(err.to_string().contains("отрицательный"), "{err}");
    }

    #[test]
    fn broken_documents_are_reported() {
        assert!(matches!(from_str("{"), Err(JsonError::Syntax(_))));
        assert!(matches!(from_str("42"), Err(JsonError::Structure(_))));
        assert!(matches!(from_str("{}"), Err(JsonError::Structure(_))));
        assert!(matches!(
            from_str(r#"{"annotations": 5}"#),
            Err(JsonError::Structure(_))
        ));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let set =
            from_str(r#"[{"type":"point","name":"n","x":1,"y":2,"confidence":0.9}]"#).unwrap();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn float_coordinates_are_accepted() {
        let set = from_str(r#"[{"type":"point","x":1.5,"y":2.5}]"#).unwrap();
        assert_eq!(
            set.get(1).unwrap().geometry.as_point(),
            Some(pos2(1.5, 2.5))
        );
    }

    #[test]
    fn colours_survive_the_round_trip() {
        let mut set = sample();
        set.get_mut(1).unwrap().color = Color32::from_rgb(1, 2, 3);
        let json = to_string_pretty(&set, None, 2).unwrap();
        assert!(json.contains("\"#010203\""), "{json}");
        assert_eq!(from_str(&json).unwrap(), set);
    }

    #[test]
    fn helpers() {
        assert_eq!(palette().len(), 8);
        assert_eq!(folder_names(&sample()), vec!["eyes", "faces"]);
        assert_eq!(
            default_json_path(Some(Path::new("/tmp/pics/cat.png"))),
            "/tmp/pics/cat.json"
        );
        assert_eq!(default_json_path(None), "annotations.json");
        assert!(is_image_path(Path::new("a/b.PNG")));
        assert!(is_image_path(Path::new("a.jpeg")));
        assert!(!is_image_path(Path::new("a.json")));
        assert!(!is_image_path(Path::new("a")));
    }

    #[test]
    fn empty_set_exports_valid_json() {
        let json = to_string_pretty(&AnnotationSet::new(), None, 2).unwrap();
        assert!(json.contains("\"annotations\": []"), "{json}");
        assert_eq!(from_str(&json).unwrap().len(), 0);
    }
}
