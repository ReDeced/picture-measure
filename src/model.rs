//! Annotations: points, rectangles, their names, folders and the container
//! that owns them.

use crate::geometry::Rect;
use egui::{Color32, Pos2, Vec2, pos2, vec2};

/// Stable, never-reused identifier of an annotation.
///
/// Also used as the `N` in the default names `point_N` / `rect_N`.
pub type ShapeId = u32;

/// Prefix of the auto-generated name of a point.
pub const POINT_PREFIX: &str = crate::POINT_PREFIX;
/// Prefix of the auto-generated name of a rectangle.
pub const RECT_PREFIX: &str = crate::RECT_PREFIX;

/// Default colour used for points.
pub const POINT_COLOR: Color32 = Color32::from_rgb(255, 92, 92);
/// Default colour used for rectangles.
pub const RECT_COLOR: Color32 = Color32::from_rgb(64, 196, 255);
/// Colour of the selection outline.
pub const SELECTION_COLOR: Color32 = Color32::from_rgb(255, 214, 51);
/// Colour of the shape currently being drawn.
pub const PREVIEW_COLOR: Color32 = Color32::from_rgb(255, 255, 255);
/// Semi-transparent fill drawn inside rectangles.
pub const FILL_ALPHA: u8 = 38;

/// Which kind of figure an annotation describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    /// A single point.
    Point,
    /// An axis-aligned rectangle.
    Rect,
}

impl ShapeKind {
    /// Prefix of the auto-generated name (`point` or `rect`).
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Point => POINT_PREFIX,
            Self::Rect => RECT_PREFIX,
        }
    }

    /// Value written to the `type` field of the JSON file.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.prefix()
    }

    /// Auto-generated name for a hypothetical figure with id `id`.
    #[must_use]
    pub fn default_name_of(self, id: ShapeId) -> String {
        format!("{}_{id}", self.prefix())
    }

    /// Default colour of this kind.
    #[must_use]
    pub const fn default_color(self) -> Color32 {
        match self {
            Self::Point => POINT_COLOR,
            Self::Rect => RECT_COLOR,
        }
    }

    /// Parses the JSON `type` field.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "point" | "dot" | "keypoint" => Some(Self::Point),
            "rect" | "rectangle" | "box" | "bbox" => Some(Self::Rect),
            _ => None,
        }
    }
}

/// The geometric payload of an annotation, in **image pixel coordinates**.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geometry {
    /// A single point.
    Point(Pos2),
    /// An axis-aligned rectangle.
    Rect(Rect),
}

impl Geometry {
    /// Kind of this geometry.
    #[must_use]
    pub const fn kind(&self) -> ShapeKind {
        match self {
            Self::Point(_) => ShapeKind::Point,
            Self::Rect(_) => ShapeKind::Rect,
        }
    }

    /// The single point, if this is a point.
    #[must_use]
    pub const fn as_point(&self) -> Option<Pos2> {
        match self {
            Self::Point(p) => Some(*p),
            Self::Rect(_) => None,
        }
    }

    /// The rectangle, if this is a rectangle.
    #[must_use]
    pub const fn as_rect(&self) -> Option<Rect> {
        match self {
            Self::Point(_) => None,
            Self::Rect(r) => Some(*r),
        }
    }

    /// Returns a copy moved by `delta`.
    #[must_use]
    pub fn translated(&self, delta: Vec2) -> Self {
        match *self {
            Self::Point(p) => Self::Point(p + delta),
            Self::Rect(r) => Self::Rect(r.translated(delta)),
        }
    }

    /// Bounding box of the geometry; points get a zero-sized box at their
    /// position.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        match *self {
            Self::Point(p) => Rect { min: p, max: p },
            Self::Rect(r) => r,
        }
    }
}

/// One measured figure on the image.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Stable identifier; also the `N` of the default name.
    pub id: ShapeId,
    /// What was measured.
    pub geometry: Geometry,
    /// Custom name; `None` means "use [`Annotation::default_name`]".
    pub name: Option<String>,
    /// Optional sub-folder ("class"/"group") this figure belongs to.
    pub folder: Option<String>,
    /// Display colour.
    pub color: Color32,
}

impl Annotation {
    /// Creates an annotation with default name, folder and colour.
    #[must_use]
    pub fn new(id: ShapeId, geometry: Geometry) -> Self {
        Self {
            id,
            color: geometry.kind().default_color(),
            geometry,
            name: None,
            folder: None,
        }
    }

    /// Kind of this annotation.
    #[must_use]
    pub const fn kind(&self) -> ShapeKind {
        self.geometry.kind()
    }

    /// Auto-generated name, e.g. `point_1` or `rect_2`.
    #[must_use]
    pub fn default_name(&self) -> String {
        crate::default_name(self.kind(), self.id)
    }

    /// The name shown in the UI and written to JSON.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.default_name())
    }

    /// `true` when the name was set by the user.
    #[must_use]
    pub fn has_custom_name(&self) -> bool {
        self.name.as_ref().is_some_and(|s| !s.trim().is_empty())
    }

    /// Sub-folder, or `""` when unset.
    #[must_use]
    pub fn folder(&self) -> &str {
        self.folder.as_deref().unwrap_or("")
    }

    /// Sets the sub-folder; empty or whitespace-only input clears it.
    pub fn set_folder(&mut self, folder: &str) {
        self.folder = normalise_folder(folder);
    }

    /// Sets the name; empty or whitespace-only input restores the default.
    pub fn set_name(&mut self, name: &str) {
        self.name = if name.trim().is_empty() {
            None
        } else {
            Some(name.trim().to_owned())
        };
    }

    /// `true` if the point `p` (in image coordinates) is on this annotation,
    /// with `tolerance` in image pixels.
    #[must_use]
    pub fn hit(&self, p: Pos2, tolerance: f32) -> bool {
        match self.geometry {
            Geometry::Point(q) => q.distance(p) <= tolerance,
            Geometry::Rect(r) => r.distance_to(p) <= tolerance,
        }
    }

    /// Moves the annotation by `delta`.
    pub fn translate(&mut self, delta: Vec2) {
        self.geometry = self.geometry.translated(delta);
    }
}

/// Trims a folder name and turns empty values into `None`.
#[must_use]
pub fn normalise_folder(folder: &str) -> Option<String> {
    let trimmed = folder.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.replace('\\', "/"))
    }
}

/// How many figures of each kind the set contains.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Total number of annotations.
    pub total: usize,
    /// Number of points.
    pub points: usize,
    /// Number of rectangles.
    pub rects: usize,
}

/// An ordered collection of annotations with stable, never-reused ids.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotationSet {
    next_id: ShapeId,
    items: Vec<Annotation>,
}

impl AnnotationSet {
    /// Creates an empty set whose first id will be `1`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 1,
            items: Vec::new(),
        }
    }

    /// Id that will be assigned to the next annotation.
    #[must_use]
    pub const fn peek_next_id(&self) -> ShapeId {
        self.next_id
    }

    /// Number of annotations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` when there are no annotations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// All annotations, in creation order.
    #[must_use]
    pub fn items(&self) -> &[Annotation] {
        &self.items
    }

    /// Iterator over the annotations.
    pub fn iter(&self) -> std::slice::Iter<'_, Annotation> {
        self.items.iter()
    }

    /// Mutable iterator over the annotations.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Annotation> {
        self.items.iter_mut()
    }

    /// Looks an annotation up by id.
    #[must_use]
    pub fn get(&self, id: ShapeId) -> Option<&Annotation> {
        self.items.iter().find(|a| a.id == id)
    }

    /// Looks an annotation up by id, mutably.
    pub fn get_mut(&mut self, id: ShapeId) -> Option<&mut Annotation> {
        self.items.iter_mut().find(|a| a.id == id)
    }

    /// All ids, in creation order.
    pub fn ids(&self) -> impl Iterator<Item = ShapeId> + '_ {
        self.items.iter().map(|a| a.id)
    }

    /// Adds a point and returns its id.
    pub fn add_point(&mut self, at: Pos2, folder: Option<&str>) -> ShapeId {
        self.push(Geometry::Point(at), folder)
    }

    /// Adds a rectangle and returns its id.
    pub fn add_rect(&mut self, rect: Rect, folder: Option<&str>) -> ShapeId {
        self.push(Geometry::Rect(rect), folder)
    }

    /// Adds an annotation of any geometry and returns its id.
    pub fn push(&mut self, geometry: Geometry, folder: Option<&str>) -> ShapeId {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut ann = Annotation::new(id, geometry);
        ann.set_folder(folder.unwrap_or_default());
        self.items.push(ann);
        id
    }

    /// Inserts an annotation keeping its id, and advances the counter so the
    /// id is never handed out twice (used when importing a JSON file).
    pub fn insert(&mut self, annotation: Annotation) -> ShapeId {
        let id = annotation.id;
        self.next_id = self.next_id.max(id.saturating_add(1));
        self.items.push(annotation);
        id
    }

    /// Removes an annotation by id, returning it.
    pub fn remove(&mut self, id: ShapeId) -> Option<Annotation> {
        let index = self.items.iter().position(|a| a.id == id)?;
        Some(self.items.remove(index))
    }

    /// Removes several annotations, returning the removed ones.
    pub fn remove_many(&mut self, ids: &[ShapeId]) -> Vec<Annotation> {
        ids.iter().filter_map(|id| self.remove(*id)).collect()
    }

    /// Drops every annotation. Ids keep growing so names stay unique.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Removes every annotation and restarts numbering at `1`.
    pub fn reset(&mut self) {
        self.items.clear();
        self.next_id = 1;
    }

    /// Finds the topmost annotation under `p`.
    ///
    /// Points win over rectangles (they are smaller and more specific), and
    /// among equals the most recently added one wins. Rectangles are scored by
    /// how close the cursor is to them so thin borders stay grabbable.
    #[must_use]
    pub fn hit_test(&self, p: Pos2, tolerance: f32) -> Option<ShapeId> {
        // Priority 1 beats priority 0; on a tie the most recently added wins
        // because we only replace the current best on a strictly better hit.
        let mut best: Option<(u8, usize)> = None;
        for (index, ann) in self.items.iter().enumerate() {
            let priority = match ann.geometry {
                Geometry::Point(q) if q.distance(p) <= tolerance => 1u8,
                Geometry::Rect(r) if r.distance_to(p) <= tolerance => 0u8,
                _ => continue,
            };
            if best.is_none_or(|(current, _)| priority >= current) {
                best = Some((priority, index));
            }
        }
        best.map(|(_, index)| self.items[index].id)
    }

    /// Finds the topmost annotation whose rectangle contains `p`.
    #[must_use]
    pub fn hit_test_interior(&self, p: Pos2) -> Option<ShapeId> {
        self.items
            .iter()
            .rev()
            .find(|a| matches!(a.geometry, Geometry::Rect(r) if r.contains(p)))
            .map(|a| a.id)
    }

    /// Finds the topmost point within `tolerance` of `p`.
    #[must_use]
    pub fn hit_test_point(&self, p: Pos2, tolerance: f32) -> Option<ShapeId> {
        self.items
            .iter()
            .rev()
            .find(|a| matches!(a.geometry, Geometry::Point(q) if q.distance(p) <= tolerance))
            .map(|a| a.id)
    }

    /// Counts points and rectangles.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for ann in &self.items {
            counts.total += 1;
            match ann.kind() {
                ShapeKind::Point => counts.points += 1,
                ShapeKind::Rect => counts.rects += 1,
            }
        }
        counts
    }

    /// Sorted, de-duplicated list of all non-empty folders.
    #[must_use]
    pub fn folders(&self) -> Vec<String> {
        let mut folders: Vec<String> = self.items.iter().filter_map(|a| a.folder.clone()).collect();
        folders.sort_unstable();
        folders.dedup();
        folders
    }

    /// Bounding box of everything, or `None` when empty.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        let mut iter = self.items.iter().map(|a| a.geometry.bounds());
        let first = iter.next()?;
        Some(iter.fold(first, |acc, r| acc.union(&r)))
    }

    /// Moves every listed annotation by `delta`, skipping unknown ids.
    pub fn translate_many(&mut self, ids: &[ShapeId], delta: Vec2) {
        for id in ids {
            if let Some(ann) = self.get_mut(*id) {
                ann.translate(delta);
            }
        }
    }

    /// Renumbers annotations so that the auto-generated names use consecutive
    /// numbers again (the "renumber" button in the UI).
    ///
    /// Custom names are left untouched, and an auto-generated name is never
    /// handed to a figure whose custom name would then duplicate it.
    pub fn renumber(&mut self) {
        let taken: Vec<String> = self
            .items
            .iter()
            .filter(|a| a.has_custom_name())
            .map(Annotation::display_name)
            .collect();

        let mut next_id: ShapeId = 1;
        for ann in &mut self.items {
            while taken.contains(&ann.kind().default_name_of(next_id)) {
                next_id += 1;
            }
            ann.id = next_id;
            next_id = next_id.saturating_add(1);
        }
        self.next_id = next_id;
    }
}

impl<'a> IntoIterator for &'a AnnotationSet {
    type Item = &'a Annotation;
    type IntoIter = std::slice::Iter<'a, Annotation>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a> IntoIterator for &'a mut AnnotationSet {
    type Item = &'a mut Annotation;
    type IntoIter = std::slice::IterMut<'a, Annotation>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}

/// Helper used by the UI to draw the label anchor of a geometry.
#[must_use]
pub fn label_anchor(geometry: Geometry) -> Pos2 {
    match geometry {
        Geometry::Point(p) => p + Vec2::new(10.0, -10.0),
        Geometry::Rect(r) => Pos2::new(r.min.x, r.min.y - 4.0),
    }
}

/// A rectangle grown to be visible even when the user only made a click.
#[must_use]
pub fn point_hit_rect(p: Pos2, tolerance: f32) -> Rect {
    Rect {
        min: p - Vec2::splat(tolerance),
        max: p + Vec2::splat(tolerance),
    }
}

/// Small vector helper used by the tests and the canvas code.
#[must_use]
pub fn direction(from: Pos2, to: Pos2) -> Vec2 {
    to - from
}

/// Convenience constructor mirroring [`Rect::from_xywh`].
#[must_use]
pub fn rect_xywh(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_xywh(x, y, width, height)
}

/// Convenience constructor mirroring [`Rect::from_corners`].
#[must_use]
pub fn rect_corners(a: Pos2, b: Pos2) -> Rect {
    Rect::from_corners(a, b)
}

/// A point at the origin, mostly useful in tests.
#[must_use]
pub fn origin() -> Pos2 {
    pos2(0.0, 0.0)
}

/// Unit vector, mostly useful in tests.
#[must_use]
pub fn unit() -> Vec2 {
    vec2(1.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names_use_id() {
        let point = Annotation::new(7, Geometry::Point(pos2(1.0, 2.0)));
        assert_eq!(point.default_name(), "point_7");
        assert_eq!(point.display_name(), "point_7");
        assert!(!point.has_custom_name());

        let rect = Annotation::new(3, Geometry::Rect(Rect::from_xywh(0.0, 0.0, 5.0, 5.0)));
        assert_eq!(rect.default_name(), "rect_3");
    }

    #[test]
    fn custom_name_overrides_default() {
        let mut ann = Annotation::new(1, Geometry::Point(origin()));
        ann.set_name("  eye_left ");
        assert_eq!(ann.display_name(), "eye_left");
        assert!(ann.has_custom_name());

        ann.set_name("   ");
        assert_eq!(ann.display_name(), "point_1");
        assert!(!ann.has_custom_name());
    }

    #[test]
    fn folder_is_trimmed_and_can_be_cleared() {
        let mut ann = Annotation::new(1, Geometry::Point(origin()));
        assert_eq!(ann.folder(), "");
        ann.set_folder("  faces\\a ");
        assert_eq!(ann.folder(), "faces/a");
        ann.set_folder("   ");
        assert_eq!(ann.folder(), "");
        assert!(ann.folder.is_none());
    }

    #[test]
    fn ids_are_never_reused() {
        let mut set = AnnotationSet::new();
        let a = set.add_point(origin(), None);
        let b = set.add_point(origin(), None);
        assert_eq!((a, b), (1, 2));
        set.remove(1);
        let c = set.add_point(origin(), None);
        assert_eq!(c, 3, "deleted ids must not be handed out again");
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn counts_and_folders() {
        let mut set = AnnotationSet::new();
        set.add_point(origin(), Some("eyes"));
        set.add_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Some("eyes"));
        set.add_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), Some("face"));
        set.add_point(origin(), None);
        let counts = set.counts();
        assert_eq!(counts.total, 4);
        assert_eq!(counts.points, 2);
        assert_eq!(counts.rects, 2);
        assert_eq!(set.folders(), vec!["eyes".to_owned(), "face".to_owned()]);
    }

    #[test]
    fn hit_test_prefers_points_then_newest() {
        let mut set = AnnotationSet::new();
        let r1 = set.add_rect(Rect::from_xywh(0.0, 0.0, 100.0, 100.0), None);
        let p1 = set.add_point(pos2(50.0, 50.0), None);
        let p2 = set.add_point(pos2(50.0, 50.0), None);

        assert_eq!(set.hit_test(pos2(50.0, 50.0), 5.0), Some(p2));
        assert_eq!(set.hit_test(pos2(20.0, 20.0), 5.0), Some(r1));
        assert_eq!(set.hit_test(pos2(500.0, 500.0), 5.0), None);
        assert_eq!(set.hit_test_point(pos2(50.0, 50.0), 5.0), Some(p2));
        assert_eq!(set.hit_test_interior(pos2(20.0, 20.0)), Some(r1));
        assert_eq!(set.hit_test_interior(pos2(200.0, 20.0)), None);
        assert!(set.get(p1).is_some());
    }

    #[test]
    fn hit_test_uses_tolerance_for_thin_rectangles() {
        let mut set = AnnotationSet::new();
        let id = set.add_rect(Rect::from_xywh(0.0, 0.0, 100.0, 4.0), None);
        assert_eq!(set.hit_test(pos2(50.0, 8.0), 5.0), Some(id));
        assert_eq!(set.hit_test(pos2(50.0, 40.0), 5.0), None);
    }

    #[test]
    fn translate_many_moves_selected_only() {
        let mut set = AnnotationSet::new();
        let a = set.add_point(pos2(0.0, 0.0), None);
        let b = set.add_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), None);
        set.translate_many(&[b], Vec2::new(10.0, 10.0));
        assert_eq!(
            set.get(a).unwrap().geometry,
            Geometry::Point(pos2(0.0, 0.0))
        );
        assert_eq!(
            set.get(b).unwrap().geometry.as_rect().unwrap().to_xywh(),
            [10.0, 10.0, 2.0, 2.0]
        );
        set.translate_many(&[999], Vec2::new(1.0, 1.0)); // unknown id is ignored
    }

    #[test]
    fn renumber_makes_ids_consecutive_and_keeps_custom_names() {
        let mut set = AnnotationSet::new();
        set.add_point(origin(), None);
        set.add_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), None);
        let c = set.add_point(origin(), None);
        set.get_mut(c).unwrap().set_name("nose");
        set.remove_many(&[1, 2]);

        set.add_point(origin(), None);
        set.add_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), None);
        set.renumber();

        assert_eq!(set.ids().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(set.get(1).unwrap().display_name(), "nose");
        assert_eq!(set.get(2).unwrap().display_name(), "point_2");
        assert_eq!(set.get(3).unwrap().display_name(), "rect_3");
        assert_eq!(set.peek_next_id(), 4);
    }

    #[test]
    fn renumber_avoids_colliding_with_custom_names() {
        let mut set = AnnotationSet::new();
        set.add_point(origin(), None);
        set.get_mut(1).unwrap().set_name("point_2");
        set.add_point(origin(), None);
        set.renumber();

        let names: Vec<String> = set.iter().map(Annotation::display_name).collect();
        assert_eq!(names, vec!["point_2".to_owned(), "point_3".to_owned()]);
        assert_eq!(set.ids().collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn bounds_covers_everything() {
        let mut set = AnnotationSet::new();
        assert!(set.bounds().is_none());
        set.add_point(pos2(5.0, 5.0), None);
        set.add_rect(Rect::from_xywh(0.0, 10.0, 4.0, 4.0), None);
        assert_eq!(set.bounds().unwrap().to_xywh(), [0.0, 5.0, 5.0, 9.0]);
    }

    #[test]
    fn insert_keeps_the_file_id_and_advances_the_counter() {
        let mut set = AnnotationSet::new();
        set.push(Geometry::Point(pos2(0.0, 0.0)), None);
        let mut imported = Annotation::new(7, Geometry::Point(pos2(1.0, 1.0)));
        imported.set_name("from_file");
        assert_eq!(set.insert(imported), 7);
        assert_eq!(set.peek_next_id(), 8);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clear_and_reset() {
        let mut set = AnnotationSet::new();
        set.add_point(origin(), None);
        set.add_point(origin(), None);
        set.clear();
        assert!(set.is_empty());
        assert_eq!(set.add_point(origin(), None), 3);
        set.reset();
        assert_eq!(set.add_point(origin(), None), 1);
    }

    #[test]
    fn geometry_helpers() {
        let g = Geometry::Rect(Rect::from_xywh(1.0, 2.0, 3.0, 4.0));
        assert_eq!(g.kind(), ShapeKind::Rect);
        assert_eq!(g.as_rect().unwrap().to_xywh(), [1.0, 2.0, 3.0, 4.0]);
        assert!(g.as_point().is_none());
        assert_eq!(g.bounds().to_xywh(), [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(
            Geometry::Point(pos2(0.0, 0.0))
                .translated(Vec2::new(2.0, 3.0))
                .as_point(),
            Some(pos2(2.0, 3.0))
        );
        assert_eq!(
            g.translated(unit()).as_rect().unwrap().to_xywh(),
            [2.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(direction(origin(), pos2(3.0, 4.0)), Vec2::new(3.0, 4.0));
        assert_eq!(
            rect_xywh(0.0, 0.0, 1.0, 1.0),
            Rect::from_min_size(origin(), Vec2::splat(1.0))
        );
        assert_eq!(
            rect_corners(origin(), pos2(1.0, 1.0)).size(),
            Vec2::splat(1.0)
        );
        assert_eq!(label_anchor(Geometry::Point(origin())), pos2(10.0, -10.0));
        assert_eq!(
            label_anchor(Geometry::Rect(Rect::from_xywh(1.0, 2.0, 3.0, 4.0))),
            pos2(1.0, -2.0)
        );
        assert_eq!(
            point_hit_rect(origin(), 2.0).to_xywh(),
            [-2.0, -2.0, 4.0, 4.0]
        );
    }

    #[test]
    fn kind_parsing() {
        assert_eq!(ShapeKind::parse("Point"), Some(ShapeKind::Point));
        assert_eq!(ShapeKind::parse(" bbox "), Some(ShapeKind::Rect));
        assert_eq!(ShapeKind::parse("polygon"), None);
        assert_eq!(ShapeKind::Point.as_str(), "point");
        assert_eq!(ShapeKind::Rect.default_name_of(12), "rect_12");
    }
}
