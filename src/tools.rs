//! The canvas state machine: turns raw pointer gestures into effects.
//!
//! Everything here works in **image coordinates** and contains no drawing
//! code, which makes the "click = point, drag = rectangle, Shift = square"
//! rules unit-testable.
//!
//! ## Rules
//!
//! | Input | Tool | Result |
//! |---|---|---|
//! | click (< [`DRAG_THRESHOLD_SCREEN`]) | [`Tool::Draw`] | point |
//! | drag | [`Tool::Draw`] | rectangle |
//! | drag + <kbd>Shift</kbd> | [`Tool::Draw`] | square rectangle |
//! | click on a figure | [`Tool::Select`] | select it |
//! | <kbd>Shift</kbd> + click | [`Tool::Select`] | add to / remove from selection |
//! | drag a selected figure | [`Tool::Select`] | move the selection |
//! | click on a figure | [`Tool::Delete`] | delete it |
//! | drag | [`Tool::Pan`] | pan |
//! | drag with middle/right button, or <kbd>Alt</kbd> | any tool | pan |
//! | wheel | any tool | zoom at the cursor |

use crate::geometry::Rect;
use crate::model::ShapeId;
use egui::{Key, PointerButton, Pos2, Vec2};

/// Pointer travel (in screen pixels) below which a press+release counts as a
/// click instead of a drag.
pub const DRAG_THRESHOLD_SCREEN: f32 = 4.0;

/// Zoom multiplier applied per wheel "line".
pub const ZOOM_PER_WHEEL_LINE: f32 = 1.2;

/// The active tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tool {
    /// Click = point, drag = rectangle (<kbd>Shift</kbd> = square).
    #[default]
    Draw,
    /// Click = select, drag = move the selection.
    Select,
    /// Click = delete the figure under the cursor.
    Delete,
    /// Drag = pan.
    Pan,
}

impl Tool {
    /// All tools, in toolbar order.
    pub const ALL: [Self; 4] = [Self::Draw, Self::Select, Self::Delete, Self::Pan];

    /// Short label shown in the toolbar, in `lang`.
    #[must_use]
    pub const fn label(self, lang: crate::i18n::Lang) -> &'static str {
        let t = crate::i18n::Text::get(lang);
        match self {
            Self::Draw => t.tool_draw,
            Self::Select => t.tool_select,
            Self::Delete => t.tool_delete,
            Self::Pan => t.tool_pan,
        }
    }

    /// Keyboard shortcut that activates the tool.
    #[must_use]
    pub const fn hotkey(self) -> Key {
        match self {
            Self::Draw => Key::D,
            Self::Select => Key::V,
            Self::Delete => Key::X,
            Self::Pan => Key::H,
        }
    }

    /// One-line description used for tooltips, in `lang`.
    #[must_use]
    pub const fn description(self, lang: crate::i18n::Lang) -> &'static str {
        let t = crate::i18n::Text::get(lang);
        match self {
            Self::Draw => t.tool_draw_hint,
            Self::Select => t.tool_select_hint,
            Self::Delete => t.tool_delete_hint,
            Self::Pan => t.tool_pan_hint,
        }
    }
}

/// What a gesture is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DragState {
    /// Nothing is being dragged.
    #[default]
    Idle,
    /// A rectangle is being drawn from `start`.
    Drawing {
        /// Image-space point where the drag started.
        start: Pos2,
    },
    /// The selection is being moved.
    Moving {
        /// Image-space point where the move started.
        start: Pos2,
        /// Image-space point at the previous frame.
        last: Pos2,
    },
    /// The view is being panned; `last` is the last screen position applied.
    Panning {
        /// Screen position the view was last moved to.
        last: Pos2,
    },
}

/// A single, side-effect-free instruction for the application state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Do nothing.
    None,
    /// The primary button went down; the application must resolve what is
    /// under `at` (select, add to selection, delete, ...).
    Pressed(Pos2),
    /// Add a point at the given image coordinates.
    AddPoint(Pos2),
    /// Add a rectangle.
    AddRect(Rect),
    /// Delete an annotation.
    Delete(ShapeId),
    /// Make an annotation the only selected one.
    Select(ShapeId),
    /// Add an annotation to the selection.
    AddToSelection(ShapeId),
    /// Remove an annotation from the selection.
    RemoveFromSelection(ShapeId),
    /// Clear the selection.
    ClearSelection,
    /// Move the selection by an image-space delta.
    MoveSelection(Vec2),
    /// Pan the view by a screen-space delta.
    Pan(Vec2),
    /// Multiply the zoom factor around a screen-space anchor.
    Zoom {
        /// Zoom factor, e.g. `1.2` for +20%.
        factor: f32,
        /// Screen-space point that must stay put.
        anchor: Pos2,
    },
}

/// Modifier keys the canvas cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CanvasModifiers {
    /// <kbd>Shift</kbd>.
    pub shift: bool,
    /// <kbd>Ctrl</kbd> / <kbd>Cmd</kbd>.
    pub ctrl: bool,
    /// <kbd>Alt</kbd>.
    pub alt: bool,
}

impl CanvasModifiers {
    /// Converts egui's platform-aware modifier set.
    #[must_use]
    pub fn from_egui(m: egui::Modifiers) -> Self {
        Self {
            shift: m.shift,
            ctrl: m.ctrl || m.command,
            alt: m.alt,
        }
    }
}

/// Gesture state of the canvas.
#[derive(Debug, Clone, Default)]
pub struct CanvasState {
    /// Active tool.
    pub tool: Tool,
    /// Current gesture.
    pub drag: DragState,
    /// Rectangle currently being drawn, if any.
    pub preview: Option<Rect>,
    drag_origin_screen: Option<Pos2>,
}

impl CanvasState {
    /// Creates an idle state using [`Tool::Draw`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Switches tool and cancels any gesture in progress.
    pub fn set_tool(&mut self, tool: Tool) {
        if self.tool != tool {
            self.tool = tool;
            self.cancel();
        }
    }

    /// Aborts the current gesture without producing an effect.
    pub fn cancel(&mut self) {
        self.drag = DragState::Idle;
        self.preview = None;
        self.drag_origin_screen = None;
    }

    /// `true` while a gesture is in progress.
    #[must_use]
    pub const fn is_dragging(&self) -> bool {
        !matches!(self.drag, DragState::Idle)
    }

    /// `true` while the view is being panned.
    #[must_use]
    pub const fn is_panning(&self) -> bool {
        matches!(self.drag, DragState::Panning { .. })
    }

    /// Screen-space origin of the current gesture.
    #[must_use]
    pub fn drag_origin_screen(&self) -> Option<Pos2> {
        self.drag_origin_screen
    }

    /// Enters [`DragState::Moving`] at `at`.
    pub fn begin_move(&mut self, at: Pos2) {
        self.drag = DragState::Moving {
            start: at,
            last: at,
        };
        self.drag_origin_screen = Some(at);
    }

    /// Distance the pointer travelled since the move gesture started.
    #[must_use]
    pub fn move_delta(&self, image_pos: Pos2) -> Vec2 {
        match self.drag {
            DragState::Moving { start, .. } => image_pos - start,
            _ => Vec2::ZERO,
        }
    }

    /// Handles a button going down.
    ///
    /// `image_pos` is the pointer position in image coordinates,
    /// `screen_pos` the same point in screen coordinates.
    pub fn on_press(
        &mut self,
        button: PointerButton,
        image_pos: Pos2,
        screen_pos: Pos2,
        mods: CanvasModifiers,
    ) -> Effect {
        self.preview = None;
        self.drag_origin_screen = Some(screen_pos);

        // Middle/right buttons always pan; so does Alt + left button.
        if button != PointerButton::Primary
            || (mods.alt && self.tool != Tool::Pan)
            || self.tool == Tool::Pan
        {
            self.drag = DragState::Panning { last: screen_pos };
            return Effect::None;
        }

        match self.tool {
            Tool::Draw => {
                self.drag = DragState::Drawing { start: image_pos };
                Effect::None
            }
            Tool::Select | Tool::Delete => {
                // What is under the cursor is only known by the application.
                self.drag = DragState::Idle;
                Effect::Pressed(image_pos)
            }
            Tool::Pan => unreachable!("handled above"),
        }
    }

    /// Handles pointer movement while a button is held.
    pub fn on_drag(
        &mut self,
        image_pos: Pos2,
        screen_delta: Vec2,
        mods: CanvasModifiers,
    ) -> Vec<Effect> {
        match self.drag {
            DragState::Idle => Vec::new(),
            DragState::Panning { last } => {
                let last = last + screen_delta;
                self.drag = DragState::Panning { last };
                if screen_delta == Vec2::ZERO {
                    Vec::new()
                } else {
                    vec![Effect::Pan(screen_delta)]
                }
            }
            DragState::Moving { start, last } => {
                let delta = image_pos - last;
                self.drag = DragState::Moving {
                    start,
                    last: image_pos,
                };
                if delta == Vec2::ZERO {
                    Vec::new()
                } else {
                    vec![Effect::MoveSelection(delta)]
                }
            }
            DragState::Drawing { start } => {
                self.preview = Some(Rect::from_corners_maybe_square(
                    start, image_pos, mods.shift,
                ));
                Vec::new()
            }
        }
    }

    /// Handles a button coming up.
    pub fn on_release(
        &mut self,
        button: PointerButton,
        image_pos: Pos2,
        screen_pos: Pos2,
        screen_travel: f32,
        mods: CanvasModifiers,
    ) -> Vec<Effect> {
        let drag = self.drag;
        self.drag_origin_screen = Some(screen_pos);
        let release_screen = screen_pos;
        self.drag = DragState::Idle;
        self.preview = None;
        self.drag_origin_screen = None;

        match (button, drag) {
            (PointerButton::Primary, DragState::Drawing { start }) => {
                let rect = Rect::from_corners_maybe_square(start, image_pos, mods.shift);
                if screen_travel <= DRAG_THRESHOLD_SCREEN || rect.is_degenerate() {
                    vec![Effect::AddPoint(start)]
                } else {
                    vec![Effect::AddRect(rect)]
                }
            }
            // A fast pan releases at a position no move event announced.
            (_, DragState::Panning { last }) => {
                let delta = release_screen - last;
                if delta == Vec2::ZERO {
                    Vec::new()
                } else {
                    vec![Effect::Pan(delta)]
                }
            }
            _ => Vec::new(),
        }
    }

    /// Handles a mouse wheel / trackpad scroll.
    pub fn on_scroll(&mut self, delta: f32, anchor: Pos2) -> Effect {
        if delta == 0.0 {
            return Effect::None;
        }
        Effect::Zoom {
            factor: ZOOM_PER_WHEEL_LINE.powf(delta),
            anchor,
        }
    }

    /// Cursor to show for the current state.
    #[must_use]
    pub fn cursor(&self) -> egui::CursorIcon {
        match self.drag {
            DragState::Panning { .. } => egui::CursorIcon::Grabbing,
            DragState::Moving { .. } => egui::CursorIcon::Move,
            DragState::Drawing { .. } => egui::CursorIcon::Crosshair,
            DragState::Idle => match self.tool {
                Tool::Pan => egui::CursorIcon::Grab,
                Tool::Draw => egui::CursorIcon::Crosshair,
                Tool::Select | Tool::Delete => egui::CursorIcon::PointingHand,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    const P: PointerButton = PointerButton::Primary;
    const NONE: CanvasModifiers = CanvasModifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };
    const SHIFT: CanvasModifiers = CanvasModifiers {
        shift: true,
        ctrl: false,
        alt: false,
    };
    const ALT: CanvasModifiers = CanvasModifiers {
        shift: false,
        ctrl: false,
        alt: true,
    };

    fn press(s: &mut CanvasState, at: Pos2, mods: CanvasModifiers) -> Effect {
        s.on_press(P, at, at, mods)
    }

    #[test]
    fn click_creates_a_point() {
        let mut s = CanvasState::new();
        assert_eq!(press(&mut s, pos2(10.0, 20.0), NONE), Effect::None);
        assert_eq!(
            s.drag,
            DragState::Drawing {
                start: pos2(10.0, 20.0)
            }
        );
        let effects = s.on_release(P, pos2(10.0, 20.0), pos2(10.0, 20.0), 0.0, NONE);
        assert_eq!(effects, vec![Effect::AddPoint(pos2(10.0, 20.0))]);
        assert_eq!(s.drag, DragState::Idle);
        assert!(s.preview.is_none());
    }

    #[test]
    fn drag_creates_a_rectangle() {
        let mut s = CanvasState::new();
        press(&mut s, pos2(10.0, 10.0), NONE);
        s.on_drag(pos2(40.0, 30.0), Vec2::new(30.0, 20.0), NONE);
        assert_eq!(s.preview.unwrap().to_xywh(), [10.0, 10.0, 30.0, 20.0]);
        let effects = s.on_release(P, pos2(40.0, 30.0), pos2(40.0, 30.0), 36.0, NONE);
        assert_eq!(
            effects,
            vec![Effect::AddRect(Rect::from_xywh(10.0, 10.0, 30.0, 20.0))]
        );
    }

    #[test]
    fn shift_makes_a_square() {
        let mut s = CanvasState::new();
        press(&mut s, pos2(10.0, 10.0), SHIFT);
        s.on_drag(pos2(40.0, 20.0), Vec2::new(30.0, 10.0), SHIFT);
        let preview = s.preview.unwrap();
        assert_eq!(preview.width(), 30.0);
        assert_eq!(preview.height(), 30.0);
        let effects = s.on_release(P, pos2(40.0, 20.0), pos2(40.0, 20.0), 31.0, SHIFT);
        assert_eq!(
            effects,
            vec![Effect::AddRect(Rect::from_xywh(10.0, 10.0, 30.0, 30.0))]
        );
    }

    #[test]
    fn shift_released_before_mouse_up_still_squares() {
        let mut s = CanvasState::new();
        press(&mut s, pos2(0.0, 0.0), SHIFT);
        s.on_drag(pos2(20.0, 5.0), Vec2::new(20.0, 5.0), SHIFT);
        let effects = s.on_release(P, pos2(20.0, 5.0), pos2(20.0, 5.0), 20.6, SHIFT);
        assert_eq!(
            effects,
            vec![Effect::AddRect(Rect::from_xywh(0.0, 0.0, 20.0, 20.0))]
        );
    }

    #[test]
    fn small_travel_is_a_click_even_when_moved() {
        let mut s = CanvasState::new();
        press(&mut s, pos2(10.0, 10.0), NONE);
        s.on_drag(pos2(11.0, 10.5), Vec2::new(1.0, 0.5), NONE);
        let effects = s.on_release(P, pos2(11.0, 10.5), pos2(11.0, 10.5), 1.1, NONE);
        assert_eq!(effects, vec![Effect::AddPoint(pos2(10.0, 10.0))]);
    }

    #[test]
    fn dragging_up_left_normalises() {
        let mut s = CanvasState::new();
        press(&mut s, pos2(100.0, 100.0), NONE);
        s.on_drag(pos2(70.0, 60.0), Vec2::new(-30.0, -40.0), NONE);
        let effects = s.on_release(P, pos2(70.0, 60.0), pos2(70.0, 60.0), 50.0, NONE);
        assert_eq!(
            effects,
            vec![Effect::AddRect(Rect::from_xywh(70.0, 60.0, 30.0, 40.0))]
        );
    }

    #[test]
    fn degenerate_release_is_a_point() {
        let mut s = CanvasState::new();
        press(&mut s, pos2(0.0, 0.0), NONE);
        assert!(
            s.on_release(P, pos2(0.0, 0.0), pos2(0.0, 0.0), 0.0, NONE)
                .contains(&Effect::AddPoint(pos2(0.0, 0.0)))
        );
    }

    #[test]
    fn secondary_button_pans_regardless_of_tool() {
        for tool in Tool::ALL {
            let mut s = CanvasState::new();
            s.set_tool(tool);
            let effect = s.on_press(
                PointerButton::Secondary,
                pos2(5.0, 5.0),
                pos2(5.0, 5.0),
                NONE,
            );
            assert_eq!(effect, Effect::None);
            assert!(s.is_panning(), "{tool:?} should pan");
            assert_eq!(
                s.on_drag(pos2(7.0, 5.0), Vec2::new(2.0, 0.0), NONE),
                vec![Effect::Pan(Vec2::new(2.0, 0.0))]
            );
            assert!(
                s.on_release(
                    PointerButton::Secondary,
                    pos2(7.0, 5.0),
                    pos2(7.0, 5.0),
                    2.0,
                    NONE
                )
                .is_empty()
            );
            assert!(!s.is_dragging());
        }
    }

    #[test]
    fn alt_pans_with_the_primary_button() {
        for tool in [Tool::Draw, Tool::Select, Tool::Delete] {
            let mut s = CanvasState::new();
            s.set_tool(tool);
            s.on_press(P, pos2(1.0, 1.0), pos2(1.0, 1.0), ALT);
            assert!(s.is_panning(), "{tool:?} should pan with Alt");
        }
    }

    #[test]
    fn pan_tool_pans_and_never_draws() {
        let mut s = CanvasState::new();
        s.set_tool(Tool::Pan);
        press(&mut s, pos2(1.0, 1.0), NONE);
        assert!(s.is_panning());
        assert_eq!(
            s.on_drag(pos2(20.0, 1.0), Vec2::new(19.0, 0.0), NONE),
            vec![Effect::Pan(Vec2::new(19.0, 0.0))]
        );
        assert!(
            s.on_release(P, pos2(20.0, 1.0), pos2(20.0, 1.0), 19.0, NONE)
                .is_empty()
        );
    }

    #[test]
    fn select_and_delete_tools_defer_to_the_application() {
        let mut s = CanvasState::new();
        s.set_tool(Tool::Select);
        assert_eq!(
            press(&mut s, pos2(3.0, 3.0), NONE),
            Effect::Pressed(pos2(3.0, 3.0))
        );
        assert!(!s.is_dragging());
        s.set_tool(Tool::Delete);
        assert_eq!(
            press(&mut s, pos2(4.0, 4.0), SHIFT),
            Effect::Pressed(pos2(4.0, 4.0))
        );
        assert!(
            s.on_release(P, pos2(4.0, 4.0), pos2(4.0, 4.0), 0.0, SHIFT)
                .is_empty()
        );
    }

    #[test]
    fn changing_tool_cancels_the_gesture() {
        let mut s = CanvasState::new();
        press(&mut s, pos2(1.0, 1.0), NONE);
        assert!(s.is_dragging());
        s.set_tool(Tool::Select);
        assert!(!s.is_dragging());
        assert!(
            s.on_release(P, pos2(50.0, 50.0), pos2(50.0, 50.0), 100.0, NONE)
                .is_empty()
        );
    }

    #[test]
    fn move_gesture_emits_incremental_deltas() {
        let mut s = CanvasState::new();
        s.set_tool(Tool::Select);
        s.begin_move(pos2(10.0, 10.0));
        assert!(s.is_dragging());
        assert_eq!(s.drag_origin_screen(), Some(pos2(10.0, 10.0)));
        assert_eq!(
            s.on_drag(pos2(15.0, 12.0), Vec2::new(5.0, 2.0), NONE),
            vec![Effect::MoveSelection(Vec2::new(5.0, 2.0))]
        );
        assert_eq!(
            s.on_drag(pos2(16.0, 12.0), Vec2::new(1.0, 0.0), NONE),
            vec![Effect::MoveSelection(Vec2::new(1.0, 0.0))]
        );
        assert_eq!(s.move_delta(pos2(16.0, 12.0)), Vec2::new(6.0, 2.0));
        assert!(
            s.on_drag(pos2(16.0, 12.0), Vec2::ZERO, NONE).is_empty(),
            "no movement, no effect"
        );
        s.on_release(P, pos2(16.0, 12.0), pos2(16.0, 12.0), 6.3, NONE);
        assert!(!s.is_dragging());
        assert_eq!(s.move_delta(pos2(16.0, 12.0)), Vec2::ZERO);
    }

    #[test]
    fn scroll_zooms_at_the_anchor() {
        let mut s = CanvasState::new();
        match s.on_scroll(1.0, pos2(5.0, 5.0)) {
            Effect::Zoom { factor, anchor } => {
                assert!((factor - ZOOM_PER_WHEEL_LINE).abs() < 1e-5);
                assert_eq!(anchor, pos2(5.0, 5.0));
            }
            other => panic!("expected zoom, got {other:?}"),
        }
        match s.on_scroll(-10.0, pos2(5.0, 5.0)) {
            Effect::Zoom { factor, .. } => assert!(factor < 0.5),
            other => panic!("expected zoom, got {other:?}"),
        }
        assert_eq!(s.on_scroll(0.0, pos2(0.0, 0.0)), Effect::None);
    }

    #[test]
    fn tool_metadata() {
        assert_eq!(Tool::ALL.len(), 4);
        assert_eq!(Tool::default(), Tool::Draw);
        assert_eq!(Tool::Draw.hotkey(), Key::D);
        assert_eq!(Tool::Select.hotkey(), Key::V);
        assert_eq!(Tool::Delete.hotkey(), Key::X);
        assert_eq!(Tool::Pan.hotkey(), Key::H);
        for tool in Tool::ALL {
            for lang in crate::i18n::Lang::ALL {
                let label = tool.label(lang);
                assert!(
                    !label.trim().is_empty(),
                    "{tool:?} / {lang}: пустая подпись"
                );
                assert!(
                    !label.chars().any(char::is_control),
                    "{tool:?} / {lang}: в подписи есть управляющие символы"
                );
            }
        }
    }

    #[test]
    fn cursor_icons() {
        let mut s = CanvasState::new();
        assert_eq!(s.cursor(), egui::CursorIcon::Crosshair);
        press(&mut s, pos2(0.0, 0.0), NONE);
        assert_eq!(s.cursor(), egui::CursorIcon::Crosshair);
        s.begin_move(pos2(0.0, 0.0));
        assert_eq!(s.cursor(), egui::CursorIcon::Move);
        s.cancel();
        s.on_press(
            PointerButton::Secondary,
            pos2(0.0, 0.0),
            pos2(0.0, 0.0),
            NONE,
        );
        assert_eq!(s.cursor(), egui::CursorIcon::Grabbing);
        s.cancel();
        s.set_tool(Tool::Select);
        assert_eq!(s.cursor(), egui::CursorIcon::PointingHand);
        s.set_tool(Tool::Delete);
        assert_eq!(s.cursor(), egui::CursorIcon::PointingHand);
        s.set_tool(Tool::Pan);
        assert_eq!(s.cursor(), egui::CursorIcon::Grab);
    }

    #[test]
    fn modifier_conversion() {
        let m = egui::Modifiers {
            shift: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            CanvasModifiers::from_egui(m),
            CanvasModifiers {
                shift: true,
                ctrl: false,
                alt: true
            }
        );
        let cmd = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        assert!(CanvasModifiers::from_egui(cmd).ctrl);
    }
}
