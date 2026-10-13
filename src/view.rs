//! The pan/zoom transform that maps image pixels to screen pixels.
//!
//! ```text
//! screen = image * scale + offset
//! image  = (screen - offset) / scale
//! ```

use crate::geometry::Rect;
use egui::{Pos2, Rect as EguiRect, Vec2, vec2};

/// Smallest allowed zoom factor (1 image pixel = 0.01 screen pixels).
pub const MIN_SCALE: f32 = 0.005;
/// Largest allowed zoom factor.
pub const MAX_SCALE: f32 = 400.0;

/// Maps image coordinates to screen coordinates and back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    /// Screen-space position of the image origin `(0, 0)`.
    pub offset: Vec2,
    /// Screen pixels per image pixel.
    pub scale: f32,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self::identity()
    }
}

impl ViewTransform {
    /// 1:1 zoom, image origin at the top-left of the viewport.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: 1.0,
        }
    }

    /// Converts a screen position into image coordinates.
    #[must_use]
    pub fn to_image(&self, screen: Pos2) -> Pos2 {
        (screen - self.offset) / self.scale
    }

    /// Converts an image position into screen coordinates.
    #[must_use]
    pub fn to_screen(&self, image: Pos2) -> Pos2 {
        image * self.scale + self.offset
    }

    /// Converts a screen-space length into image-space length.
    #[must_use]
    pub fn to_image_vec(&self, screen: Vec2) -> Vec2 {
        screen / self.scale
    }

    /// Converts an image-space length into screen-space length.
    #[must_use]
    pub fn to_screen_vec(&self, image: Vec2) -> Vec2 {
        image * self.scale
    }

    /// A screen-space tolerance expressed in image pixels.
    #[must_use]
    pub fn to_image_tolerance(&self, screen: f32) -> f32 {
        screen / self.scale
    }

    /// Drags the image along with the cursor.
    pub fn pan_by(&mut self, screen_delta: Vec2) {
        self.offset += screen_delta;
    }

    /// Multiplies the zoom factor, keeping `anchor` (screen) pinned to the
    /// same image pixel.
    pub fn zoom_by(&mut self, factor: f32, anchor: Pos2) {
        let new_scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        if (new_scale - self.scale).abs() < f32::EPSILON {
            return;
        }
        let ratio = new_scale / self.scale;
        self.offset = anchor - (anchor - self.offset) * ratio;
        self.scale = new_scale;
    }

    /// Sets an absolute zoom factor, keeping the centre of `viewport` pinned.
    pub fn set_scale(&mut self, scale: f32, viewport: EguiRect) {
        let anchor = viewport.center();
        let clamped = scale.clamp(MIN_SCALE, MAX_SCALE);
        let ratio = clamped / self.scale;
        self.offset = anchor - (anchor - self.offset) * ratio;
        self.scale = clamped;
    }

    /// Centers `image_size` inside `viewport`, leaving a relative `margin`.
    pub fn fit(&mut self, viewport: EguiRect, image_size: Vec2, margin: f32) {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return;
        }
        if image_size.x <= 0.0 || image_size.y <= 0.0 {
            return;
        }
        let usable = Vec2::new(
            viewport.width() * (1.0 - margin),
            viewport.height() * (1.0 - margin),
        );
        let scale = (usable.x / image_size.x)
            .min(usable.y / image_size.y)
            .clamp(MIN_SCALE, MAX_SCALE);
        self.scale = scale;
        self.offset = viewport.center().to_vec2() - image_size * (scale * 0.5);
    }

    /// The part of the image currently visible in `viewport`, in image
    /// coordinates. May extend outside the image, which is useful for drawing
    /// rulers and for deciding whether a shape is off-screen.
    #[must_use]
    pub fn visible_image_rect(&self, viewport: EguiRect) -> Rect {
        let a = self.to_image(viewport.left_top());
        let b = self.to_image(viewport.right_bottom());
        Rect::from_corners(a, b)
    }
}

/// Screen-space bounding box of an image-space rectangle.
#[must_use]
pub fn screen_rect(transform: &ViewTransform, rect: Rect) -> EguiRect {
    let a = transform.to_screen(rect.min);
    let b = transform.to_screen(rect.max);
    EguiRect::from_min_max(a, b)
}

/// Screen-space rectangle occupied by a single point marker of `radius`.
#[must_use]
pub fn screen_dot(transform: &ViewTransform, at: Pos2, radius: f32) -> EguiRect {
    let center = transform.to_screen(at);
    EguiRect::from_center_size(center, vec2(radius * 2.0, radius * 2.0))
}

/// Steps used by the grid overlay: the smallest `1/2/5 × 10^n` multiple of
/// `min_screen_step / scale`.
#[must_use]
pub fn grid_step(scale: f32, min_screen_step: f32) -> f32 {
    let target = min_screen_step / scale;
    let exponent = target.log10().floor();
    let base = 10f32.powf(exponent);
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * base)
        .find(|s| *s >= target)
        .unwrap_or(10.0 * base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> EguiRect {
        EguiRect::from_min_size(pos2(x, y), vec2(w, h))
    }

    const EPS: f32 = 1e-4;

    #[test]
    fn identity_maps_one_to_one() {
        let v = ViewTransform::identity();
        assert_eq!(v.to_image(pos2(10.0, 20.0)), pos2(10.0, 20.0));
        assert_eq!(v.to_screen(pos2(10.0, 20.0)), pos2(10.0, 20.0));
        assert_eq!(v.to_image_vec(Vec2::new(4.0, 0.0)), Vec2::new(4.0, 0.0));
        assert_eq!(v.to_screen_vec(Vec2::new(4.0, 0.0)), Vec2::new(4.0, 0.0));
    }

    #[test]
    fn round_trip() {
        let v = ViewTransform {
            offset: Vec2::new(13.0, -4.0),
            scale: 2.5,
        };
        let p = pos2(123.0, 456.0);
        let back = v.to_image(v.to_screen(p));
        assert!((back - p).length() < EPS, "{back:?} != {p:?}");
    }

    #[test]
    fn zoom_keeps_anchor_pinned() {
        let mut v = ViewTransform {
            offset: Vec2::new(100.0, 50.0),
            scale: 1.0,
        };
        let anchor = pos2(400.0, 300.0);
        let before = v.to_image(anchor);
        v.zoom_by(3.0, anchor);
        let after = v.to_image(anchor);
        assert!((after - before).length() < EPS);
        assert!((v.scale - 3.0).abs() < EPS);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = ViewTransform::identity();
        v.zoom_by(1e9, pos2(0.0, 0.0));
        assert_eq!(v.scale, MAX_SCALE);
        v.zoom_by(1e-12, pos2(0.0, 0.0));
        assert_eq!(v.scale, MIN_SCALE);
    }

    #[test]
    fn pan_moves_image_with_cursor() {
        let mut v = ViewTransform::identity();
        let image = v.to_image(pos2(10.0, 10.0));
        v.pan_by(Vec2::new(5.0, 7.0));
        assert_eq!(v.to_image(pos2(10.0, 10.0)), image - Vec2::new(5.0, 7.0));
    }

    #[test]
    fn fit_centers_the_image() {
        let mut v = ViewTransform::identity();
        let viewport = rect(0.0, 0.0, 800.0, 600.0);
        v.fit(viewport, vec2(400.0, 200.0), 0.0);
        assert!((v.scale - 2.0).abs() < EPS);
        assert!(v.to_screen(pos2(200.0, 100.0)).distance(viewport.center()) < EPS);
    }

    #[test]
    fn fit_respects_margin_and_degenerate_inputs() {
        let mut v = ViewTransform::identity();
        let viewport = rect(0.0, 0.0, 1000.0, 1000.0);
        v.fit(viewport, vec2(100.0, 100.0), 0.5);
        assert!((v.scale - 5.0).abs() < EPS);

        let before = v;
        v.fit(rect(0.0, 0.0, 0.0, 0.0), vec2(100.0, 100.0), 0.0);
        v.fit(viewport, vec2(0.0, 100.0), 0.0);
        assert_eq!(v, before);
    }

    #[test]
    fn set_scale_keeps_the_viewport_center_pinned() {
        let mut v = ViewTransform::identity();
        let viewport = rect(0.0, 0.0, 500.0, 500.0);
        let before = v.to_image(viewport.center());
        v.set_scale(3.0, viewport);
        assert!((v.scale - 3.0).abs() < EPS);
        assert!(v.to_image(viewport.center()).distance(before) < EPS);
        v.set_scale(1e9, viewport);
        assert_eq!(v.scale, MAX_SCALE);
    }

    #[test]
    fn visible_image_rect() {
        let v = ViewTransform {
            offset: Vec2::ZERO,
            scale: 2.0,
        };
        let visible = v.visible_image_rect(rect(0.0, 0.0, 200.0, 100.0));
        assert_eq!(visible.to_xywh(), [0.0, 0.0, 100.0, 50.0]);
    }

    #[test]
    fn helpers() {
        let v = ViewTransform {
            offset: Vec2::ZERO,
            scale: 2.0,
        };
        assert_eq!(
            screen_rect(&v, Rect::from_xywh(1.0, 1.0, 2.0, 2.0)),
            rect(2.0, 2.0, 4.0, 4.0)
        );
        assert_eq!(
            screen_dot(&v, pos2(1.0, 1.0), 3.0),
            rect(-1.0, -1.0, 6.0, 6.0)
        );
        assert!((v.to_image_tolerance(4.0) - 2.0).abs() < EPS);
    }

    #[test]
    fn grid_step_is_nice() {
        assert_eq!(grid_step(1.0, 10.0), 10.0);
        assert_eq!(grid_step(1.0, 30.0), 50.0);
        assert_eq!(grid_step(4.0, 20.0), 5.0);
        assert_eq!(grid_step(0.1, 10.0), 100.0);
    }
}
