//! The interaction pipeline: raw `egui` events in, annotation changes out.
//!
//! [`Session`] owns everything a measuring session needs — the figure set, the
//! undo history, the selection, the view transform, the gesture state machine
//! and the pointer tracker — and turns a frame's worth of raw events into
//! state changes.
//!
//! It deliberately knows nothing about drawing, so a whole editing session can
//! be simulated in a unit test.

use crate::geometry::Rect;
use crate::history::History;
use crate::model::{AnnotationSet, Counts, Geometry, ShapeId};
use crate::pointer::PointerInput;
use crate::tools::{CanvasModifiers, CanvasState, Effect, Tool, ZOOM_PER_WHEEL_LINE};
use crate::view::ViewTransform;
use egui::{Color32, Event, PointerButton, Pos2, Rect as EguiRect, Vec2, pos2};

/// Buttons the canvas listens to, in press order.
pub const BUTTONS: [PointerButton; 3] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Middle,
];

/// Total vertical wheel movement in the frame, in wheel lines.
///
/// [`PointerInput`] deliberately ignores wheel events, so the canvas reads them
/// straight from the event stream.
fn wheel_delta(events: &[Event]) -> f32 {
    let mut total = 0.0;
    for event in events {
        if let Event::MouseWheel { unit, delta, .. } = event {
            total += match unit {
                egui::MouseWheelUnit::Line => delta.y,
                // Points and pages are scaled to something line-like.
                egui::MouseWheelUnit::Point => delta.y / 50.0,
                egui::MouseWheelUnit::Page => delta.y * 10.0,
            };
        }
    }
    total
}

/// A transient message shown in the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// What to show.
    pub text: String,
    /// `true` when the message reports a failure.
    pub is_error: bool,
}

/// Everything that changed during one frame of interaction.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameChanges {
    /// `true` when figures or their metadata changed and the file is now
    /// out of date on disk.
    pub dirty: bool,
    /// The view moved, so the canvas needs repainting.
    pub view_changed: bool,
}

/// A complete measuring session.
pub struct Session {
    set: AnnotationSet,
    history: History,
    selection: Vec<ShapeId>,
    view: ViewTransform,
    canvas: CanvasState,
    pointer: PointerInput,
    new_folder: String,
    /// `true` when there are changes that were never written to disk.
    dirty: bool,
    message: Option<Message>,
    select_on_drag: Option<ShapeId>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// A fresh, empty session using [`Tool::Draw`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            set: AnnotationSet::new(),
            history: History::default(),
            selection: Vec::new(),
            view: ViewTransform::identity(),
            canvas: CanvasState::new(),
            pointer: PointerInput::new(),
            new_folder: String::new(),
            dirty: false,
            message: None,
            select_on_drag: None,
        }
    }

    // ------------------------------------------------------------------
    // Read-only access
    // ------------------------------------------------------------------

    /// The figures.
    #[must_use]
    pub fn set(&self) -> &AnnotationSet {
        &self.set
    }

    /// The view transform.
    #[must_use]
    pub fn view(&self) -> ViewTransform {
        self.view
    }

    /// The gesture state machine.
    #[must_use]
    pub fn canvas(&self) -> &CanvasState {
        &self.canvas
    }

    /// The raw pointer tracker.
    #[must_use]
    pub fn pointer(&self) -> &PointerInput {
        &self.pointer
    }

    /// Selected figure ids, in selection order.
    #[must_use]
    pub fn selection(&self) -> &[ShapeId] {
        &self.selection
    }

    /// Sub-folder applied to new figures.
    #[must_use]
    pub fn new_folder(&self) -> &str {
        &self.new_folder
    }

    /// Sets the sub-folder applied to new figures.
    pub fn set_new_folder(&mut self, folder: &str) {
        folder.trim().clone_into(&mut self.new_folder);
    }

    /// `true` when there are unsaved changes.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the session as saved.
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Number of figures of each kind.
    #[must_use]
    pub fn counts(&self) -> Counts {
        self.set.counts()
    }

    /// `true` when the undo stack has something to restore.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// `true` when the redo stack has something to restore.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// `true` when `id` is selected.
    #[must_use]
    pub fn is_selected(&self, id: ShapeId) -> bool {
        self.selection.contains(&id)
    }

    /// The message to display, if any.
    #[must_use]
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// Takes the current message out of the session.
    pub fn take_message(&mut self) -> Option<Message> {
        self.message.take()
    }

    /// Shows an informational message.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            is_error: false,
        });
    }

    /// Shows an error message.
    pub fn notify_error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            is_error: true,
        });
    }

    /// Screen-space radius, in pixels, within which a figure is grabbable.
    #[must_use]
    pub const fn hit_radius() -> f32 {
        6.0
    }

    /// Image-space tolerance corresponding to [`Session::hit_radius`].
    #[must_use]
    pub fn hit_tolerance(&self) -> f32 {
        self.view.to_image_tolerance(Self::hit_radius())
    }

    /// Tooltip text for a figure, e.g. `rect_2 — прямоугольник (10.0, 20.0, 30×40)`.
    #[must_use]
    pub fn describe(annotation: &crate::model::Annotation) -> String {
        match annotation.geometry {
            Geometry::Point(p) => format!(
                "{} — точка ({:.1}, {:.1})",
                annotation.display_name(),
                p.x,
                p.y
            ),
            Geometry::Rect(r) => format!(
                "{} — прямоугольник ({:.1}, {:.1}, {:.1}×{:.1})",
                annotation.display_name(),
                r.min.x,
                r.min.y,
                r.width(),
                r.height()
            ),
        }
    }

    // ------------------------------------------------------------------
    // Loading
    // ------------------------------------------------------------------

    /// Replaces the figures with `set` and forgets the undo history.
    pub fn load(&mut self, set: AnnotationSet) {
        self.set = set;
        self.selection.clear();
        self.select_on_drag = None;
        self.history.clear();
        self.dirty = false;
    }

    /// Aborts any gesture in progress, e.g. on <kbd>Esc</kbd>.
    pub fn cancel_gesture(&mut self) {
        self.canvas.cancel();
    }

    /// `true` while a gesture is in progress.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.canvas.is_dragging()
    }

    // ------------------------------------------------------------------
    // Editing
    // ------------------------------------------------------------------

    /// Makes `id` the only selected figure.
    pub fn select_only(&mut self, id: ShapeId) {
        self.selection.clear();
        self.selection.push(id);
    }

    /// Adds or removes `id` from the selection.
    pub fn toggle_selection(&mut self, id: ShapeId) {
        if let Some(index) = self.selection.iter().position(|s| *s == id) {
            self.selection.remove(index);
        } else {
            self.selection.push(id);
        }
    }

    /// Selects everything.
    pub fn select_all(&mut self) {
        self.selection = self.set.ids().collect();
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Deletes every selected figure.
    pub fn delete_selected(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.history.record(&self.set);
        self.set.remove_many(&self.selection);
        self.selection.clear();
        self.dirty = true;
    }

    /// Deletes one figure.
    pub fn delete_one(&mut self, id: ShapeId) {
        self.history.record(&self.set);
        if self.set.remove(id).is_some() {
            self.selection.retain(|s| *s != id);
            self.dirty = true;
        }
    }

    /// Deletes everything.
    pub fn clear_all(&mut self) {
        if self.set.is_empty() {
            return;
        }
        self.history.record(&self.set);
        self.set.reset();
        self.selection.clear();
        self.dirty = true;
    }

    /// Copies a figure, keeping its folder, colour and position.
    pub fn duplicate(&mut self, id: ShapeId) {
        let Some(source) = self.set.get(id).cloned() else {
            return;
        };
        self.history.record(&self.set);
        let folder = source.folder().to_owned();
        let color = source.color;
        let new_id = self.set.push(source.geometry, Some(&folder));
        if let Some(annotation) = self.set.get_mut(new_id) {
            annotation.color = color;
        }
        self.select_only(new_id);
        self.dirty = true;
        let name = self
            .set
            .get(new_id)
            .map_or_else(String::new, crate::model::Annotation::display_name);
        self.notify(format!("Создана копия: {name}"));
    }

    /// Renames a figure; an empty name restores the auto-generated one.
    pub fn set_name(&mut self, id: ShapeId, name: &str) {
        let Some(annotation) = self.set.get_mut(id) else {
            return;
        };
        let wanted = if name.trim().is_empty() {
            annotation.default_name()
        } else {
            name.trim().to_owned()
        };
        if annotation.display_name() != wanted {
            self.history.record(&self.set);
            if let Some(annotation) = self.set.get_mut(id) {
                annotation.set_name(name);
            }
            self.dirty = true;
        }
    }

    /// Moves a figure into `folder`; an empty folder removes it.
    pub fn set_folder(&mut self, id: ShapeId, folder: &str) {
        let Some(annotation) = self.set.get_mut(id) else {
            return;
        };
        if annotation.folder() != folder.trim() {
            self.history.record(&self.set);
            if let Some(annotation) = self.set.get_mut(id) {
                annotation.set_folder(folder);
            }
            self.dirty = true;
        }
    }

    /// Moves every selected figure into `folder`.
    pub fn set_selection_folder(&mut self, folder: &str) {
        if self.selection.is_empty() {
            return;
        }
        self.history.record(&self.set);
        let ids = self.selection.clone();
        for id in ids {
            if let Some(annotation) = self.set.get_mut(id) {
                annotation.set_folder(folder);
            }
        }
        self.dirty = true;
    }

    /// Recolours one figure.
    pub fn set_color(&mut self, id: ShapeId, color: Color32) {
        let Some(annotation) = self.set.get_mut(id) else {
            return;
        };
        if annotation.color != color {
            self.history.record(&self.set);
            if let Some(annotation) = self.set.get_mut(id) {
                annotation.color = color;
            }
            self.dirty = true;
        }
    }

    /// Recolours the whole selection.
    pub fn set_selection_color(&mut self, color: Color32) {
        if self.selection.is_empty() {
            return;
        }
        self.history.record(&self.set);
        let ids = self.selection.clone();
        for id in ids {
            if let Some(annotation) = self.set.get_mut(id) {
                annotation.color = color;
            }
        }
        self.dirty = true;
    }

    /// Makes the auto-generated names consecutive again.
    pub fn renumber(&mut self) {
        if self.set.is_empty() {
            return;
        }
        self.history.record(&self.set);
        self.set.renumber();
        self.selection.clear();
        self.dirty = true;
        self.notify("Фигуры перенумерованы");
    }

    /// Undoes the last change.
    pub fn undo(&mut self) -> bool {
        let restored = self.history.undo(&mut self.set);
        if restored {
            self.selection.clear();
            self.dirty = true;
        }
        restored
    }

    /// Redoes the last undone change.
    pub fn redo(&mut self) -> bool {
        let restored = self.history.redo(&mut self.set);
        if restored {
            self.selection.clear();
            self.dirty = true;
        }
        restored
    }

    // ------------------------------------------------------------------
    // View
    // ------------------------------------------------------------------

    /// Fits `image_size` into `canvas`.
    pub fn fit_view(&mut self, canvas: EguiRect, image_size: Vec2) {
        if canvas.width() > 1.0 && canvas.height() > 1.0 && image_size.x > 0.0 && image_size.y > 0.0
        {
            self.view.fit(canvas, image_size, 0.04);
        }
    }

    /// Resets the zoom to 1:1 around the centre of `canvas`.
    pub fn reset_zoom(&mut self, canvas: EguiRect) {
        self.view.set_scale(1.0, canvas);
    }

    /// Zoom percentage for the status bar.
    #[must_use]
    pub fn zoom_percent(&self) -> f32 {
        self.view.scale * 100.0
    }

    /// Drags the image along with the cursor, in screen pixels.
    pub fn pan_by(&mut self, screen_delta: Vec2) {
        self.view.pan_by(screen_delta);
    }

    /// The image point currently under `screen_pos`.
    #[must_use]
    pub fn image_under(&self, screen_pos: Pos2) -> Pos2 {
        self.view.to_image(screen_pos)
    }

    /// The screen position of an image point.
    #[must_use]
    pub fn screen_of(&self, image_pos: Pos2) -> Pos2 {
        self.view.to_screen(image_pos)
    }

    // ------------------------------------------------------------------
    // Input
    // ------------------------------------------------------------------

    /// Feeds one frame's worth of raw events into the session.
    ///
    /// `canvas` is the rectangle the canvas occupied; events outside it are
    /// ignored so that the side panel keeps working normally.
    pub fn handle_events(
        &mut self,
        events: &[Event],
        modifiers: egui::Modifiers,
        canvas: EguiRect,
        backend_down: &[PointerButton],
    ) -> FrameChanges {
        let mut changes = FrameChanges {
            dirty: self.dirty,
            view_changed: false,
        };

        self.pointer.begin_frame();
        self.pointer.update(events);
        let mods = CanvasModifiers::from_egui(modifiers);

        let over_canvas = self.pointer.position().is_some_and(|p| canvas.contains(p));

        if over_canvas {
            changes.view_changed |= self.handle_canvas(canvas, mods, wheel_delta(events));
        }

        // A gesture must never outlive the button that started it. `backend_down`
        // is authoritative: backends drop events when the window loses focus,
        // and a lost release would otherwise freeze the preview on the canvas.
        let tracked = BUTTONS.iter().any(|b| self.pointer.down(*b));
        let backend_agrees = BUTTONS.iter().any(|b| backend_down.contains(b));
        if self.canvas.is_dragging() && !(tracked && backend_agrees) {
            self.canvas.cancel();
        }

        changes.dirty = self.dirty;
        changes
    }

    /// The part of [`Session::handle_events`] that only runs over the canvas.
    ///
    /// Returns `true` when the view moved.
    fn handle_canvas(&mut self, canvas: EguiRect, mods: CanvasModifiers, scroll: f32) -> bool {
        let mut view_changed = false;
        let position = self.pointer.position();

        // --- buttons going down ------------------------------------------------
        for button in BUTTONS {
            if !self.pointer.pressed(button) {
                continue;
            }
            let Some(screen) = position else { continue };
            let image = self.view.to_image(screen);
            let effect = self.canvas.on_press(button, image, screen, mods);
            view_changed |= self.apply(&[effect], mods);
        }

        // --- dragging ----------------------------------------------------------
        let delta = self.pointer.frame_delta();
        if delta != Vec2::ZERO
            && let Some(screen) = position
        {
            let image = self.view.to_image(screen);
            let effects = self.canvas.on_drag(image, delta, mods);
            view_changed |= self.apply(&effects, mods);
        }

        // --- buttons coming up -------------------------------------------------
        for button in BUTTONS {
            if !self.pointer.released(button) {
                continue;
            }
            let release = self
                .pointer
                .release_position(button)
                .or(position)
                .unwrap_or_else(|| pos2(0.0, 0.0));
            let travel = self
                .pointer
                .release_travelled(button)
                .unwrap_or_else(|| self.pointer.travelled(button));
            let image = self.view.to_image(release);
            // A fast flick releases at a new position without an intervening
            // move event: flush the remaining movement first.
            let flush = self.flush_move(image);
            view_changed |= self.apply(&flush, mods);
            let effects = self.canvas.on_release(button, image, release, travel, mods);
            view_changed |= self.apply(&effects, mods);
        }
        self.select_on_drag = None;

        // --- mouse wheel --------------------------------------------------------
        if scroll != 0.0
            && let Some(anchor) = position
            && canvas.contains(anchor)
        {
            let effect = self.canvas.on_scroll(scroll, anchor);
            view_changed |= self.apply(&[effect], mods);
        }

        view_changed
    }

    /// Moves the selection to `at` if a move gesture is in progress.
    ///
    /// Returns the effects to apply; empty when there is nothing to do.
    fn flush_move(&mut self, at: Pos2) -> Vec<Effect> {
        if !matches!(self.canvas.drag, crate::tools::DragState::Moving { .. }) {
            return Vec::new();
        }
        self.canvas
            .on_drag(at, Vec2::ZERO, CanvasModifiers::default())
    }

    /// Applies a batch of [`Effect`]s; returns `true` when the view moved.
    pub fn apply(&mut self, effects: &[Effect], mods: CanvasModifiers) -> bool {
        let mut view_changed = false;
        for effect in effects {
            match *effect {
                Effect::None | Effect::ClearSelection => {}
                Effect::Pressed(at) => self.resolve_click(at, mods),
                Effect::AddPoint(at) => {
                    let folder = self.new_folder.clone();
                    self.history.record(&self.set);
                    let id = self.set.add_point(at, Some(&folder));
                    self.select_only(id);
                    self.dirty = true;
                }
                Effect::AddRect(rect) => {
                    let folder = self.new_folder.clone();
                    self.history.record(&self.set);
                    let id = self.set.add_rect(rect, Some(&folder));
                    self.select_only(id);
                    self.dirty = true;
                }
                Effect::Delete(id) => self.delete_one(id),
                Effect::Select(id) => {
                    self.select_only(id);
                    self.select_on_drag = Some(id);
                    self.begin_move();
                }
                Effect::AddToSelection(id) => {
                    if !self.is_selected(id) {
                        self.selection.push(id);
                    }
                    self.select_on_drag = Some(id);
                    self.begin_move();
                }
                Effect::RemoveFromSelection(id) => self.toggle_selection(id),
                Effect::MoveSelection(delta) => {
                    let ids = self.selection.clone();
                    self.set.translate_many(&ids, delta);
                    self.dirty = true;
                }
                Effect::Pan(delta) => {
                    self.view.pan_by(delta);
                    view_changed = true;
                }
                Effect::Zoom { factor, anchor } => {
                    self.view.zoom_by(factor, anchor);
                    view_changed = true;
                }
            }
        }
        view_changed
    }

    /// Enters the move gesture when the pressed figure is in the selection.
    fn begin_move(&mut self) {
        let Some(id) = self.select_on_drag else {
            return;
        };
        if !self.is_selected(id) {
            return;
        }
        if let Some(pos) = self.pointer.position() {
            self.canvas.begin_move(self.view.to_image(pos));
        }
    }

    /// Handles a click with the selection or delete tool.
    fn resolve_click(&mut self, at: Pos2, mods: CanvasModifiers) {
        let hit = self.set.hit_test(at, self.hit_tolerance());
        match self.canvas.tool {
            Tool::Delete => match hit {
                Some(id) => self.delete_one(id),
                None => self.clear_selection(),
            },
            Tool::Select => match hit {
                Some(id) => {
                    if mods.shift {
                        self.toggle_selection(id);
                        if self.is_selected(id) {
                            self.select_on_drag = Some(id);
                            self.begin_move();
                        }
                    } else {
                        self.select_only(id);
                        self.select_on_drag = Some(id);
                        self.begin_move();
                    }
                }
                None => self.clear_selection(),
            },
            Tool::Draw | Tool::Pan => {}
        }
    }

    /// The figure under `screen_pos`, if any.
    #[must_use]
    pub fn hit_at(&self, screen_pos: Pos2) -> Option<ShapeId> {
        let image = self.view.to_image(screen_pos);
        self.set.hit_test(image, self.hit_tolerance())
    }

    /// Requests a zoom change, used by the toolbar.
    pub fn zoom_by(&mut self, factor: f32, anchor: Pos2) {
        self.view.zoom_by(factor, anchor);
    }

    /// Switches tool.
    pub fn set_tool(&mut self, tool: Tool) {
        self.canvas.set_tool(tool);
    }

    /// The active tool.
    #[must_use]
    pub fn tool(&self) -> Tool {
        self.canvas.tool
    }

    /// Places a figure programmatically, used by the tests and by importers.
    pub fn add_point(&mut self, at: Pos2) -> ShapeId {
        self.history.record(&self.set);
        let id = self.set.add_point(at, None);
        self.dirty = true;
        id
    }

    /// Places a rectangle programmatically.
    pub fn add_rect(&mut self, rect: Rect) -> ShapeId {
        self.history.record(&self.set);
        let id = self.set.add_rect(rect, None);
        self.dirty = true;
        id
    }

    /// Zoom factor for one wheel line, re-exported for the status bar hints.
    #[must_use]
    pub const fn wheel_zoom() -> f32 {
        ZOOM_PER_WHEEL_LINE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Geometry;
    use egui::vec2;

    const CANVAS: EguiRect = EguiRect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0));

    fn press(pos: Pos2) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn release(pos: Pos2, mods: egui::Modifiers) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: mods,
        }
    }

    fn move_to(pos: Pos2) -> Event {
        Event::PointerMoved(pos)
    }

    fn shift() -> egui::Modifiers {
        egui::Modifiers {
            shift: true,
            ..Default::default()
        }
    }

    /// Feeds one frame, deriving the backend button state from the events.
    ///
    /// A real window backend reports which buttons are physically held; here it
    /// is reconstructed from the `PointerButton` events of the current frame
    /// plus whatever was already held.
    fn frame(session: &mut Session, events: &[Event], mods: egui::Modifiers) -> FrameChanges {
        let mut backend: Vec<PointerButton> = session.pointer().held_now().collect();
        for event in events {
            if let Event::PointerButton {
                button, pressed, ..
            } = event
            {
                if *pressed {
                    if !backend.contains(button) {
                        backend.push(*button);
                    }
                } else {
                    backend.retain(|b| b != button);
                }
            }
        }
        session.handle_events(events, mods, CANVAS, &backend)
    }

    fn click(session: &mut Session, at: Pos2) {
        let none = egui::Modifiers::default();
        frame(session, &[press(at)], none);
        frame(session, &[release(at, none)], none);
    }

    fn drag(session: &mut Session, from: Pos2, to: Pos2, mods: egui::Modifiers) {
        frame(session, &[press(from)], mods);
        frame(session, &[move_to(to)], mods);
        frame(session, &[release(to, mods)], mods);
    }

    #[test]
    fn a_click_creates_a_point() {
        let mut session = Session::new();
        click(&mut session, pos2(100.0, 50.0));
        assert_eq!(session.set().len(), 1);
        let annotation = &session.set().items()[0];
        assert_eq!(annotation.geometry, Geometry::Point(pos2(100.0, 50.0)));
        assert_eq!(annotation.display_name(), "point_1");
        assert!(session.is_dirty());
    }

    #[test]
    fn a_drag_creates_a_rectangle() {
        let mut session = Session::new();
        drag(
            &mut session,
            pos2(10.0, 10.0),
            pos2(40.0, 30.0),
            egui::Modifiers::default(),
        );
        assert_eq!(session.set().len(), 1);
        let rect = session.set().items()[0].geometry.as_rect().unwrap();
        assert_eq!(rect.to_xywh(), [10.0, 10.0, 30.0, 20.0]);
        assert_eq!(session.set().items()[0].display_name(), "rect_1");
    }

    #[test]
    fn shift_creates_a_square() {
        let mut session = Session::new();
        drag(&mut session, pos2(10.0, 10.0), pos2(60.0, 25.0), shift());
        let rect = session.set().items()[0].geometry.as_rect().unwrap();
        assert_eq!(rect.width(), 50.0, "сторона следует за движением по X");
        assert_eq!(rect.height(), 50.0);
        assert!(rect.is_square(1e-4));
    }

    #[test]
    fn new_figures_get_the_current_folder() {
        let mut session = Session::new();
        session.set_new_folder("eyes");
        click(&mut session, pos2(1.0, 1.0));
        assert_eq!(session.set().items()[0].folder(), "eyes");
        session.set_new_folder("");
        click(&mut session, pos2(2.0, 2.0));
        assert_eq!(session.set().items()[1].folder(), "");
    }

    #[test]
    fn names_count_up_per_figure() {
        let mut session = Session::new();
        click(&mut session, pos2(1.0, 1.0));
        drag(
            &mut session,
            pos2(0.0, 0.0),
            pos2(20.0, 20.0),
            egui::Modifiers::default(),
        );
        click(&mut session, pos2(2.0, 2.0));
        let names: Vec<String> = session
            .set()
            .iter()
            .map(crate::model::Annotation::display_name)
            .collect();
        assert_eq!(names, ["point_1", "rect_2", "point_3"]);
    }

    #[test]
    fn panning_moves_the_view_but_not_the_figures() {
        let mut session = Session::new();
        click(&mut session, pos2(100.0, 100.0));
        let before = session.view();
        // Start the pan with the middle button.
        frame(
            &mut session,
            &[Event::PointerButton {
                pos: pos2(400.0, 300.0),
                button: PointerButton::Middle,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
            egui::Modifiers::default(),
        );
        assert!(session.canvas().is_panning());
        let changes = frame(
            &mut session,
            &[move_to(pos2(430.0, 320.0))],
            egui::Modifiers::default(),
        );
        assert!(changes.view_changed, "движение мыши сдвигает картинку");
        assert_ne!(session.view(), before);
        assert_eq!(session.set().len(), 1, "панорама не добавляет фигур");
        assert_eq!(
            session.set().items()[0].geometry,
            Geometry::Point(pos2(100.0, 100.0)),
            "координаты фигур не зависят от панорамы"
        );
    }

    #[test]
    fn selecting_and_deleting_with_the_tools() {
        let mut session = Session::new();
        click(&mut session, pos2(50.0, 50.0));

        session.set_tool(Tool::Select);
        click(&mut session, pos2(50.0, 50.0));
        assert_eq!(session.selection(), &[1]);

        session.set_tool(Tool::Delete);
        click(&mut session, pos2(50.0, 50.0));
        assert!(session.set().is_empty());
    }

    #[test]
    fn shift_click_extends_the_selection() {
        let mut session = Session::new();
        click(&mut session, pos2(10.0, 10.0));
        click(&mut session, pos2(80.0, 80.0));
        session.set_tool(Tool::Select);
        click(&mut session, pos2(10.0, 10.0));
        frame(&mut session, &[press(pos2(80.0, 80.0))], shift());
        frame(&mut session, &[release(pos2(80.0, 80.0), shift())], shift());
        assert_eq!(session.selection().len(), 2);
    }

    #[test]
    fn dragging_a_selected_figure_moves_it() {
        let mut session = Session::new();
        session.add_point(pos2(10.0, 10.0));
        session.set_tool(Tool::Select);
        frame(
            &mut session,
            &[press(pos2(10.0, 10.0))],
            egui::Modifiers::default(),
        );
        frame(
            &mut session,
            &[move_to(pos2(25.0, 18.0))],
            egui::Modifiers::default(),
        );
        frame(
            &mut session,
            &[release(pos2(25.0, 18.0), egui::Modifiers::default())],
            egui::Modifiers::default(),
        );
        assert_eq!(
            session.set().items()[0].geometry,
            Geometry::Point(pos2(25.0, 18.0))
        );
    }

    #[test]
    fn events_outside_the_canvas_are_ignored() {
        let mut session = Session::new();
        let changes = frame(
            &mut session,
            &[press(pos2(900.0, 700.0))],
            egui::Modifiers::default(),
        );
        assert!(!changes.view_changed);
        frame(
            &mut session,
            &[release(pos2(900.0, 700.0), egui::Modifiers::default())],
            egui::Modifiers::default(),
        );
        assert!(session.set().is_empty());
    }

    #[test]
    fn a_lost_release_cancels_the_gesture() {
        let mut session = Session::new();
        frame(
            &mut session,
            &[press(pos2(10.0, 10.0))],
            egui::Modifiers::default(),
        );
        assert!(session.canvas().is_dragging());
        frame(
            &mut session,
            &[move_to(pos2(30.0, 30.0))],
            egui::Modifiers::default(),
        );
        assert!(session.canvas().preview.is_some());

        // The window loses focus: the backend stops reporting the button and
        // the release event never arrives.
        session.handle_events(&[], egui::Modifiers::default(), CANVAS, &[]);
        assert!(!session.canvas().is_dragging(), "жест не должен зависать");
        assert!(session.canvas().preview.is_none());
        assert!(session.set().is_empty());
    }

    #[test]
    fn a_gesture_survives_while_the_backend_agrees() {
        let mut session = Session::new();
        frame(
            &mut session,
            &[press(pos2(10.0, 10.0))],
            egui::Modifiers::default(),
        );
        // The backend still says the button is held, so the drag continues.
        session.handle_events(
            &[move_to(pos2(30.0, 30.0))],
            egui::Modifiers::default(),
            CANVAS,
            &[PointerButton::Primary],
        );
        assert!(session.canvas().is_dragging());
        assert_eq!(
            session.canvas().preview.unwrap().to_xywh(),
            [10.0, 10.0, 20.0, 20.0]
        );
    }

    #[test]
    fn zoom_and_reset() {
        let mut session = Session::new();
        let anchor = pos2(400.0, 300.0);
        session.zoom_by(2.0, anchor);
        assert!((session.zoom_percent() - 200.0).abs() < 1e-3);
        session.reset_zoom(CANVAS);
        assert!((session.zoom_percent() - 100.0).abs() < 1e-3);
        session.fit_view(CANVAS, vec2(100.0, 50.0));
        assert!(session.zoom_percent() > 100.0);
    }

    #[test]
    fn fit_view_ignores_degenerate_input() {
        let mut session = Session::new();
        session.fit_view(
            EguiRect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 0.0)),
            vec2(10.0, 10.0),
        );
        session.fit_view(CANVAS, Vec2::ZERO);
        assert!((session.view().scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn undo_and_redo_through_the_session() {
        let mut session = Session::new();
        click(&mut session, pos2(1.0, 1.0));
        click(&mut session, pos2(2.0, 2.0));
        assert_eq!(session.set().len(), 2);
        assert!(session.can_undo());
        assert!(session.undo());
        assert_eq!(session.set().len(), 1);
        assert!(session.can_redo());
        assert!(session.redo());
        assert_eq!(session.set().len(), 2);
        assert!(!session.redo());
    }

    #[test]
    fn load_replaces_everything() {
        let mut session = Session::new();
        click(&mut session, pos2(1.0, 1.0));
        let mut other = AnnotationSet::new();
        other.add_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), None);
        session.load(other);
        assert_eq!(session.set().len(), 1);
        assert!(!session.is_dirty());
        assert!(!session.can_undo());
    }

    #[test]
    fn folder_and_colour_apply_to_the_selection() {
        let mut session = Session::new();
        session.add_point(pos2(1.0, 1.0));
        session.add_point(pos2(2.0, 2.0));
        session.select_all();
        session.set_selection_folder("eyes");
        assert!(session.set().iter().all(|a| a.folder() == "eyes"));
        let red = Color32::from_rgb(255, 0, 0);
        session.set_selection_color(red);
        assert!(session.set().iter().all(|a| a.color == red));
    }

    #[test]
    fn names_folders_and_colours_can_be_edited() {
        let mut session = Session::new();
        let id = session.add_point(pos2(1.0, 1.0));
        session.set_name(id, "нос");
        assert_eq!(session.set().get(id).unwrap().display_name(), "нос");
        session.set_name(id, "  ");
        assert_eq!(session.set().get(id).unwrap().display_name(), "point_1");
        session.set_folder(id, " лица ");
        assert_eq!(session.set().get(id).unwrap().folder(), "лица");
        session.set_folder(id, " ");
        assert_eq!(session.set().get(id).unwrap().folder(), "");
        session.set_color(id, Color32::GREEN);
        assert_eq!(session.set().get(id).unwrap().color, Color32::GREEN);
    }

    #[test]
    fn renumber_and_clear() {
        let mut session = Session::new();
        session.add_point(pos2(1.0, 1.0));
        session.add_point(pos2(2.0, 2.0));
        session.delete_one(1);
        session.add_point(pos2(3.0, 3.0));
        session.renumber();
        let ids: Vec<ShapeId> = session.set().ids().collect();
        assert_eq!(ids, vec![1, 2]);
        session.clear_all();
        assert!(session.set().is_empty());
    }

    #[test]
    fn duplicate_keeps_folder_and_colour() {
        let mut session = Session::new();
        let id = session.add_point(pos2(3.0, 4.0));
        session.set_folder(id, "eyes");
        session.set_color(id, Color32::from_rgb(1, 2, 3));
        session.duplicate(id);
        assert_eq!(session.set().len(), 2);
        let copy = session.set().items()[1].clone();
        assert_eq!(copy.folder(), "eyes");
        assert_eq!(copy.color, Color32::from_rgb(1, 2, 3));
        assert_eq!(copy.geometry, Geometry::Point(pos2(3.0, 4.0)));
        assert_eq!(session.selection(), &[copy.id]);
    }

    #[test]
    fn messages_can_be_set_and_taken() {
        let mut session = Session::new();
        assert!(session.message().is_none());
        session.notify("готово");
        assert_eq!(
            session.take_message(),
            Some(Message {
                text: "готово".to_owned(),
                is_error: false
            })
        );
        assert!(session.message().is_none());
        session.notify_error("ошибка");
        assert!(session.message().unwrap().is_error);
    }

    #[test]
    fn counts_and_hit_testing() {
        let mut session = Session::new();
        session.add_point(pos2(10.0, 10.0));
        session.add_rect(Rect::from_xywh(0.0, 0.0, 40.0, 40.0));
        let counts = session.counts();
        assert_eq!(counts.total, 2);
        assert_eq!(counts.points, 1);
        assert_eq!(counts.rects, 1);
        assert_eq!(session.hit_at(pos2(10.0, 10.0)), Some(1));
        assert_eq!(session.hit_at(pos2(500.0, 500.0)), None);
    }

    #[test]
    fn describe_is_human_readable() {
        let mut session = Session::new();
        let p = session.add_point(pos2(1.5, 2.5));
        assert!(Session::describe(session.set().get(p).unwrap()).contains("точка"));
        let r = session.add_rect(Rect::from_xywh(1.0, 2.0, 3.0, 4.0));
        let text = Session::describe(session.set().get(r).unwrap());
        assert!(text.contains("прямоугольник"), "{text}");
        assert!(text.contains("3.0×4.0"), "{text}");
    }

    #[test]
    fn dirty_flag_tracks_edits() {
        let mut session = Session::new();
        assert!(!session.is_dirty());
        session.add_point(pos2(1.0, 1.0));
        assert!(session.is_dirty());
        session.mark_saved();
        assert!(!session.is_dirty());
        let id = session.set().ids().next().unwrap();
        let auto = session.set().get(id).unwrap().display_name();
        session.set_name(id, &auto);
        assert!(
            !session.is_dirty(),
            "переименование в то же имя не меняет файл"
        );
        session.set_name(id, "новое");
        assert!(session.is_dirty());
    }

    #[test]
    fn wheel_zoom_constant_is_sane() {
        assert!(Session::wheel_zoom() > 1.0);
    }
}
