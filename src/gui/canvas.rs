//! The central canvas: painting the image, the figures and the overlays.
//!
//! Input handling lives in [`picture_measure::session::Session`]; this module
//! only paints what the session's state describes.

use super::app::{ACCENT, App, OUTSIDE, outline_width};
use egui::{
    Align2, Color32, Context, FontId, Painter, PointerButton, Pos2, Rect as EguiRect, RichText,
    Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use picture_measure::geometry::Rect as ImageRect;
use picture_measure::model::{Annotation, FILL_ALPHA, Geometry, label_anchor};
use picture_measure::session::BUTTONS;
use picture_measure::session::Session;
use picture_measure::view::ViewTransform;

/// Radius of a point marker, in screen pixels.
const POINT_RADIUS: f32 = 4.0;
/// Minimum distance between grid lines, in screen pixels.
const GRID_MIN_STEP: f32 = 12.0;
/// Full-texture UV rectangle.
const UV_FULL: EguiRect = EguiRect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

/// Draws the canvas and feeds one frame of input to the session.
pub(crate) fn show(app: &mut App, ctx: &Context, ui: &mut Ui) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    app.canvas_rect = Some(rect);

    let events = ctx.input(|i| i.events.clone());
    let modifiers = ctx.input(|i| i.modifiers);
    let mut held: [Option<PointerButton>; 3] = [None; 3];
    ctx.input(|i| {
        for (slot, button) in held.iter_mut().zip(BUTTONS) {
            *slot = i.pointer.button_down(button).then_some(button);
        }
    });
    let backend_down: Vec<PointerButton> = held.into_iter().flatten().collect();
    app.session
        .handle_events(&events, modifiers, rect, &backend_down);

    paint(app, ui, rect);

    if app.pointer_over_canvas(rect) && !response.context_menu_opened() {
        ctx.set_cursor_icon(app.session.canvas().cursor());
    }

    context_menu(app, &response, rect);
}

/// A semi-transparent version of an **opaque** `color`.
///
/// [`Color32`] stores premultiplied channels, so the RGB components have to be
/// scaled by the new alpha rather than kept as they are.
fn translucent(color: Color32, alpha: u8) -> Color32 {
    color.gamma_multiply_u8(alpha)
}

/// Culls figures that are far outside the canvas.
fn is_visible(view: &ViewTransform, bounds: ImageRect, rect: EguiRect) -> bool {
    let screen = EguiRect::from_min_max(
        view.to_screen(bounds.min) - Vec2::splat(64.0),
        view.to_screen(bounds.max) + Vec2::splat(64.0),
    );
    screen.intersects(rect)
}

/// Paints the image, the figures and the overlays.
fn paint(app: &App, ui: &Ui, rect: EguiRect) {
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 29, 33));

    let Some(texture) = app.texture.as_ref() else {
        draw_placeholder(&painter, rect, app.t().drop_image);
        return;
    };

    let view = app.session.view();
    let image_screen =
        EguiRect::from_min_size(view.to_screen(pos2(0.0, 0.0)), app.image_size * view.scale);
    painter.image(texture.id(), image_screen, UV_FULL, Color32::WHITE);
    painter.rect_stroke(
        image_screen,
        0.0,
        Stroke::new(1.0, Color32::from_gray(70)),
        OUTSIDE,
    );

    let visible = view.visible_image_rect(rect);
    if app.show_grid {
        draw_grid(&painter, &view, visible, rect);
    }

    for annotation in app.session.set() {
        if !is_visible(&view, annotation.geometry.bounds(), rect) {
            continue;
        }
        let selected = app.session.is_selected(annotation.id);
        match annotation.geometry {
            Geometry::Point(at) => draw_point(&painter, &view, at, annotation.color, selected),
            Geometry::Rect(r) => draw_rect(&painter, &view, r, annotation.color, selected),
        }
        if app.show_labels {
            draw_label(
                &painter,
                &view,
                annotation.geometry,
                &annotation.display_name(),
                annotation.folder(),
                selected,
            );
        }
    }

    if let Some(preview) = app.session.canvas().preview {
        draw_preview(&painter, &view, preview);
    }

    if app.show_ruler
        && app.session.pointer().down(PointerButton::Primary)
        && let Some(pos) = app.session.pointer().position()
    {
        draw_crosshair(&painter, rect, pos);
    }
}

/// The "drop an image here" placeholder.
fn draw_placeholder(painter: &Painter, rect: EguiRect, message: &str) {
    let center = rect.center();
    painter.text(
        center,
        Align2::CENTER_CENTER,
        message,
        FontId::proportional(16.0),
        Color32::from_gray(140),
    );
    painter.rect_stroke(
        EguiRect::from_center_size(center, vec2(340.0, 190.0)),
        0.0,
        Stroke::new(1.0, Color32::from_gray(60)),
        OUTSIDE,
    );
}

/// Draws the coordinate grid.
fn draw_grid(painter: &Painter, view: &ViewTransform, visible: ImageRect, canvas: EguiRect) {
    let step = picture_measure::view::grid_step(view.scale, GRID_MIN_STEP);
    let stroke = Stroke::new(1.0, Color32::from_black_alpha(40));
    let mut x = (visible.min.x / step).ceil() * step;
    while x <= visible.max.x {
        let sx = view.to_screen(pos2(x, 0.0)).x;
        painter.line_segment([pos2(sx, canvas.top()), pos2(sx, canvas.bottom())], stroke);
        x += step;
    }
    let mut y = (visible.min.y / step).ceil() * step;
    while y <= visible.max.y {
        let sy = view.to_screen(pos2(0.0, y)).y;
        painter.line_segment([pos2(canvas.left(), sy), pos2(canvas.right(), sy)], stroke);
        y += step;
    }
}

/// Draws a point marker with a crosshair.
fn draw_point(painter: &Painter, view: &ViewTransform, at: Pos2, color: Color32, selected: bool) {
    let center = view.to_screen(at);
    let arm = POINT_RADIUS + 3.0;
    let thin = Stroke::new(1.0, color);
    painter.line_segment(
        [
            pos2(center.x - arm, center.y),
            pos2(center.x + arm, center.y),
        ],
        thin,
    );
    painter.line_segment(
        [
            pos2(center.x, center.y - arm),
            pos2(center.x, center.y + arm),
        ],
        thin,
    );
    painter.circle_filled(center, POINT_RADIUS, color);
    painter.circle_stroke(
        center,
        POINT_RADIUS,
        Stroke::new(1.5, Color32::from_black_alpha(160)),
    );
    if selected {
        painter.circle_stroke(center, POINT_RADIUS + 3.0, Stroke::new(1.5, ACCENT));
    }
}

/// Draws a rectangle with its size in the middle.
fn draw_rect(
    painter: &Painter,
    view: &ViewTransform,
    rect: ImageRect,
    color: Color32,
    selected: bool,
) {
    let a = view.to_screen(rect.min);
    let b = view.to_screen(rect.max);
    let screen = EguiRect::from_min_max(a, b);
    painter.rect_filled(screen, 0.0, translucent(color, FILL_ALPHA));
    painter.rect_stroke(
        screen,
        0.0,
        Stroke::new(outline_width(view.scale), color),
        OUTSIDE,
    );

    if selected {
        painter.rect_stroke(screen.expand(3.0), 0.0, Stroke::new(1.0, ACCENT), OUTSIDE);
        for corner in [a, b, pos2(a.x, b.y), pos2(b.x, a.y)] {
            painter.rect_filled(
                EguiRect::from_center_size(corner, Vec2::splat(5.0)),
                1.0,
                ACCENT,
            );
        }
    }

    if screen.width() > 46.0 && screen.height() > 20.0 {
        let text = format!("{:.0}×{:.0}", rect.width().round(), rect.height().round());
        painter.text(
            screen.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::monospace(12.0),
            Color32::WHITE,
        );
    }
}

/// Draws the name badge of a figure.
fn draw_label(
    painter: &Painter,
    view: &ViewTransform,
    geometry: Geometry,
    name: &str,
    folder: &str,
    selected: bool,
) {
    let screen = view.to_screen(label_anchor(geometry));
    let text = if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{name} · {folder}")
    };
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), Color32::WHITE);
    let rect = EguiRect::from_min_size(screen, galley.size() + Vec2::new(6.0, 3.0));
    let bg = if selected {
        ACCENT
    } else {
        Color32::from_black_alpha(155)
    };
    painter.rect_filled(rect, 2.0, bg);
    painter.galley(
        pos2(rect.left() + 3.0, rect.top() + 1.0),
        galley,
        Color32::WHITE,
    );
}

/// Draws the rectangle currently being dragged.
fn draw_preview(painter: &Painter, view: &ViewTransform, rect: ImageRect) {
    let screen = EguiRect::from_min_max(view.to_screen(rect.min), view.to_screen(rect.max));
    painter.rect_filled(screen, 0.0, translucent(Color32::WHITE, 30));
    painter.rect_stroke(screen, 0.0, Stroke::new(1.5, Color32::WHITE), OUTSIDE);
    let text = format!("{:.0}×{:.0}", rect.width().round(), rect.height().round());
    painter.text(
        screen.left_top() + Vec2::new(4.0, -2.0),
        Align2::LEFT_BOTTOM,
        text,
        FontId::monospace(11.0),
        Color32::WHITE,
    );
}

/// Draws the two lines that follow the cursor.
fn draw_crosshair(painter: &Painter, rect: EguiRect, at: Pos2) {
    if !rect.contains(at) {
        return;
    }
    let stroke = Stroke::new(1.0, Color32::from_white_alpha(50));
    painter.line_segment([pos2(rect.left(), at.y), pos2(rect.right(), at.y)], stroke);
    painter.line_segment([pos2(at.x, rect.top()), pos2(at.x, rect.bottom())], stroke);
}

/// Right-click menu on a figure.
fn context_menu(app: &mut App, response: &egui::Response, canvas: EguiRect) {
    let hit = app
        .session
        .pointer()
        .position()
        .filter(|p| canvas.contains(*p))
        .and_then(|p| app.session.hit_at(p));
    app.context_target = hit;

    response.context_menu(|ui| {
        ui.set_min_width(210.0);
        let t = app.t();
        let Some(id) = app.context_target else {
            ui.label(t.no_figure_here);
            return;
        };
        let annotation = app.session.set().get(id);
        ui.label(
            RichText::new(annotation.map_or_else(String::new, Annotation::display_name)).strong(),
        );
        if let Some(annotation) = annotation {
            ui.label(RichText::new(Session::describe(annotation, app.lang)).small());
        }
        ui.separator();
        if ui.button(t.menu_select).clicked() {
            app.session.select_only(id);
            ui.close();
        }
        if ui.button(t.menu_duplicate).clicked() {
            app.session.duplicate(id);
            let template = app.t().duplicated;
            let name = app
                .session
                .set()
                .get(id)
                .map_or_else(String::new, Annotation::display_name);
            app.say(template, &[("name", &name)]);
            ui.close();
        }
        if ui.button(t.menu_reset_name).clicked() {
            app.session.set_name(id, "");
            ui.close();
        }
        ui.separator();
        if ui.button(t.menu_delete).clicked() {
            app.session.delete_one(id);
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> EguiRect {
        EguiRect::from_min_size(pos2(x, y), vec2(w, h))
    }

    #[test]
    fn uv_is_the_whole_texture() {
        assert_eq!(UV_FULL, rect(0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn culling_keeps_what_is_on_screen() {
        let view = ViewTransform::identity();
        let canvas = rect(0.0, 0.0, 800.0, 600.0);
        assert!(is_visible(
            &view,
            ImageRect::from_xywh(10.0, 10.0, 5.0, 5.0),
            canvas
        ));
        // A 64px margin keeps things just off the edge grabbable.
        assert!(is_visible(
            &view,
            ImageRect::from_xywh(790.0, 590.0, 1.0, 1.0),
            canvas
        ));
        assert!(!is_visible(
            &view,
            ImageRect::from_xywh(5000.0, 5000.0, 1.0, 1.0),
            canvas
        ));
    }

    #[test]
    fn culling_works_when_zoomed_in() {
        let view = ViewTransform {
            offset: Vec2::new(-10_000.0, -10_000.0),
            scale: 10.0,
        };
        let canvas = rect(0.0, 0.0, 800.0, 600.0);
        assert!(is_visible(
            &view,
            ImageRect::from_xywh(1000.0, 1000.0, 10.0, 10.0),
            canvas
        ));
        assert!(!is_visible(
            &view,
            ImageRect::from_xywh(10.0, 10.0, 10.0, 10.0),
            canvas
        ));
    }

    #[test]
    fn translucent_keeps_the_hue() {
        let opaque = Color32::from_rgb(200, 100, 50);
        let faded = translucent(opaque, 0);
        assert_eq!((faded.r(), faded.g(), faded.b()), (0, 0, 0));
        assert_eq!(faded.a(), 0);

        let half = translucent(opaque, 128);
        assert_eq!(half.a(), 128);
        assert!(
            half.r() > opaque.r() / 3 && half.r() < opaque.r(),
            "каналы масштабируются вместе с прозрачностью: {half:?}"
        );
        assert!(
            half.r() > half.g() && half.g() > half.b(),
            "порядок каналов сохранён"
        );

        // Полная непрозрачность ничего не меняет.
        let same = translucent(opaque, 255);
        assert_eq!(
            (same.r(), same.g(), same.b(), same.a()),
            (200, 100, 50, 255)
        );
    }
}
