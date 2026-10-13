//! Small geometry helpers built on top of `egui`'s [`Pos2`]/[`Vec2`].

use egui::{Pos2, Vec2, pos2, vec2};

/// Minimal side (in pixels) under which a dragged rectangle is considered
/// "not dragged yet". Expressed in *image* pixels.
pub const MIN_DRAG_SIZE: f32 = 1.0;

/// An axis-aligned rectangle stored as its minimum/maximum corners.
///
/// Using corners instead of `x/y/w/h` makes it impossible to represent an
/// inverted rectangle by accident.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Upper-left corner (smallest coordinates).
    pub min: Pos2,
    /// Lower-right corner (largest coordinates).
    pub max: Pos2,
}

impl Rect {
    /// Builds a rectangle from raw `x`, `y`, `width`, `height` values.
    #[must_use]
    pub fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            min: pos2(x, y),
            max: pos2(x + width, y + height),
        }
    }

    /// Builds the smallest rectangle containing both corners.
    #[must_use]
    pub fn from_corners(a: Pos2, b: Pos2) -> Self {
        Self {
            min: pos2(a.x.min(b.x), a.y.min(b.y)),
            max: pos2(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Rectangle spanning `a` to `b`; when `square` is `true` the result has
    /// equal sides.
    ///
    /// The side length is `max(|dx|, |dy|)` and the rectangle grows from `a`
    /// towards `b`, so dragging down-right grows right/down, dragging
    /// up-left grows left/up, exactly like the mouse moved.
    #[must_use]
    pub fn from_corners_maybe_square(a: Pos2, b: Pos2, square: bool) -> Self {
        if !square {
            return Self::from_corners(a, b);
        }
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let side = dx.abs().max(dy.abs());
        let x = if dx < 0.0 { a.x - side } else { a.x };
        let y = if dy < 0.0 { a.y - side } else { a.y };
        Self {
            min: pos2(x, y),
            max: pos2(x + side, y + side),
        }
    }

    /// Builds a rectangle with the given `origin` (top-left) and `size`.
    #[must_use]
    pub fn from_min_size(origin: Pos2, size: Vec2) -> Self {
        Self {
            min: origin,
            max: pos2(origin.x + size.x, origin.y + size.y),
        }
    }

    /// Builds a rectangle with the given centre and `size`.
    #[must_use]
    pub fn from_center_size(center: Pos2, size: Vec2) -> Self {
        let half = size * 0.5;
        Self {
            min: center - half,
            max: center + half,
        }
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> f32 {
        (self.max.x - self.min.x).max(0.0)
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> f32 {
        (self.max.y - self.min.y).max(0.0)
    }

    /// Size as a vector.
    #[must_use]
    pub fn size(&self) -> Vec2 {
        vec2(self.width(), self.height())
    }

    /// Geometric centre.
    #[must_use]
    pub fn center(&self) -> Pos2 {
        self.min + (self.max - self.min) * 0.5
    }

    /// Area in square pixels.
    #[must_use]
    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    /// `true` if the rectangle has no meaningful area (a click, not a drag).
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        self.width() < MIN_DRAG_SIZE || self.height() < MIN_DRAG_SIZE
    }

    /// `true` if the rectangle is a perfect square (within `eps`).
    #[must_use]
    pub fn is_square(&self, eps: f32) -> bool {
        (self.width() - self.height()).abs() <= eps
    }

    /// `true` if `p` lies inside the rectangle.
    #[must_use]
    pub fn contains(&self, p: Pos2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// Returns a copy grown (or shrunk, for negative values) on every side.
    #[must_use]
    pub fn expanded(&self, margin: f32) -> Self {
        Self {
            min: self.min - Vec2::splat(margin),
            max: self.max + Vec2::splat(margin),
        }
    }

    /// Returns a copy moved by `delta`.
    #[must_use]
    pub fn translated(&self, delta: Vec2) -> Self {
        Self {
            min: self.min + delta,
            max: self.max + delta,
        }
    }

    /// Distance from `p` to the nearest point inside the rectangle.
    ///
    /// Zero when `p` is inside. Used for "grabbing" thin rectangles.
    #[must_use]
    pub fn distance_to(&self, p: Pos2) -> f32 {
        let dx = (self.min.x - p.x).max(0.0).max(p.x - self.max.x);
        let dy = (self.min.y - p.y).max(0.0).max(p.y - self.max.y);
        dx.hypot(dy)
    }

    /// Smallest rectangle containing both `self` and `other`.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: Pos2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Pos2::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// Converts to `[x, y, width, height]`.
    #[must_use]
    pub fn to_xywh(self) -> [f32; 4] {
        [self.min.x, self.min.y, self.width(), self.height()]
    }
}

impl From<Rect> for [f32; 4] {
    fn from(value: Rect) -> Self {
        value.to_xywh()
    }
}

/// Rounds `value` to `precision` decimal places.
///
/// Used by the JSON exporter so `12` stays `12` instead of becoming `12.0`.
/// Works in `f64` so that e.g. `12.3456` with precision 2 gives exactly
/// `12.35`.
#[must_use]
pub fn round_to(value: f64, precision: u32) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let factor = 10f64.powi(precision as i32);
    (value * factor).round() / factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_xywh() {
        let r = Rect::from_xywh(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.min, pos2(10.0, 20.0));
        assert_eq!(r.max, pos2(40.0, 60.0));
        assert_eq!(r.width(), 30.0);
        assert_eq!(r.height(), 40.0);
        assert_eq!(r.center(), pos2(25.0, 40.0));
    }

    #[test]
    fn from_corners_normalises_direction() {
        let a = pos2(100.0, 100.0);
        let b = pos2(40.0, 20.0);
        let r = Rect::from_corners(a, b);
        assert_eq!(r.min, pos2(40.0, 20.0));
        assert_eq!(r.max, pos2(100.0, 100.0));
        assert_eq!(Rect::from_corners(b, a), r);
        assert_eq!(r.to_xywh(), [40.0, 20.0, 60.0, 80.0]);
    }

    #[test]
    fn square_grows_towards_the_cursor() {
        let a = pos2(10.0, 10.0);
        // Wider than tall: side follows the x movement.
        let r = Rect::from_corners_maybe_square(a, pos2(40.0, 20.0), true);
        assert_eq!(r.min, pos2(10.0, 10.0));
        assert_eq!(r.max, pos2(40.0, 40.0));
        assert_eq!(r.width(), 30.0);
        assert_eq!(r.height(), 30.0);

        // Taller than wide: side follows the y movement.
        let r = Rect::from_corners_maybe_square(a, pos2(20.0, 50.0), true);
        assert_eq!(r.min, pos2(10.0, 10.0));
        assert_eq!(r.max, pos2(50.0, 50.0));
        assert_eq!(r.width(), 40.0);
        assert_eq!(r.height(), 40.0);
    }

    #[test]
    fn square_keeps_growth_direction_when_dragging_up_left() {
        let a = pos2(100.0, 100.0);
        let r = Rect::from_corners_maybe_square(a, pos2(70.0, 60.0), true);
        assert_eq!(r.min, pos2(60.0, 60.0));
        assert_eq!(r.max, pos2(100.0, 100.0));
        assert_eq!(r.width(), 40.0);
        assert_eq!(r.height(), 40.0);
        assert!(r.is_square(0.001));
    }

    #[test]
    fn not_square_when_flag_is_off() {
        let r = Rect::from_corners_maybe_square(pos2(0.0, 0.0), pos2(40.0, 20.0), false);
        assert_eq!(r.to_xywh(), [0.0, 0.0, 40.0, 20.0]);
    }

    #[test]
    fn contains_and_distance() {
        let r = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(pos2(5.0, 5.0)));
        assert!(r.contains(pos2(0.0, 10.0)));
        assert!(!r.contains(pos2(-0.1, 5.0)));
        assert_eq!(r.distance_to(pos2(5.0, 5.0)), 0.0);
        assert!((r.distance_to(pos2(13.0, 5.0)) - 3.0).abs() < 1e-5);
        assert!((r.distance_to(pos2(-3.0, -4.0)) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn degenerate_detection() {
        assert!(Rect::from_xywh(5.0, 5.0, 0.0, 10.0).is_degenerate());
        assert!(Rect::from_xywh(5.0, 5.0, 0.4, 0.4).is_degenerate());
        assert!(!Rect::from_xywh(5.0, 5.0, 2.0, 2.0).is_degenerate());
    }

    #[test]
    fn translate_expand_union() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            a.translated(vec2(5.0, -5.0)).to_xywh(),
            [5.0, -5.0, 10.0, 10.0]
        );
        assert_eq!(a.expanded(2.0).to_xywh(), [-2.0, -2.0, 14.0, 14.0]);
        let b = Rect::from_xywh(20.0, 20.0, 5.0, 5.0);
        assert_eq!(a.union(&b).to_xywh(), [0.0, 0.0, 25.0, 25.0]);
    }

    #[test]
    fn from_center_size() {
        let r = Rect::from_center_size(pos2(10.0, 10.0), vec2(4.0, 8.0));
        assert_eq!(r.to_xywh(), [8.0, 6.0, 4.0, 8.0]);
        assert_eq!(r.center(), pos2(10.0, 10.0));
    }

    #[test]
    fn rounding() {
        assert_eq!(round_to(12.3456, 0), 12.0);
        assert_eq!(round_to(12.3456, 2), 12.35);
        assert_eq!(round_to(12.3456, 3), 12.346);
        assert_eq!(round_to(-1.005, 1), -1.0);
        assert_eq!(round_to(f64::NAN, 2), 0.0);
        assert_eq!(round_to(f64::INFINITY, 2), 0.0);
        assert_eq!(round_to(100.0, 0), 100.0);
    }
}
