//! Reconstruction of multi-button pointer input from raw [`egui::Event`]s.
//!
//! `egui::Pointer` only tracks the primary button in detail, but this app needs
//! to pan with the middle/right mouse button while drawing with the left one.
//! [`PointerInput`] therefore replays the event stream once per frame and keeps
//! the position, the press origin and the accumulated movement of every button
//! that is currently held down.

use egui::{Event, PointerButton, Pos2, Vec2};

/// Where a button went down and where it is now.
#[derive(Debug, Clone, Copy)]
struct HeldButton {
    origin: Pos2,
    last: Pos2,
}

/// Information about a release that happened during this frame.
#[derive(Debug, Clone, Copy)]
struct ReleaseInfo {
    position: Pos2,
    travelled: f32,
}

/// Per-frame snapshot of what the mouse buttons are doing.
///
/// Call [`PointerInput::begin_frame`] at the start of a frame, then feed the
/// frame's events to [`PointerInput::update`].
#[derive(Debug, Clone, Default)]
pub struct PointerInput {
    position: Option<Pos2>,
    pressed: Vec<PointerButton>,
    released: Vec<(PointerButton, ReleaseInfo)>,
    held: Vec<(PointerButton, HeldButton)>,
    frame_delta: Vec2,
}

impl PointerInput {
    /// Creates an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the per-frame accumulators. Call once per frame, before
    /// [`PointerInput::update`].
    pub fn begin_frame(&mut self) {
        self.pressed.clear();
        self.released.clear();
        self.frame_delta = Vec2::ZERO;
    }

    /// Forgets everything, e.g. when the pointer leaves the window.
    pub fn reset(&mut self) {
        self.position = None;
        self.pressed.clear();
        self.released.clear();
        self.held.clear();
        self.frame_delta = Vec2::ZERO;
    }

    /// Feeds a slice of events, in the order egui produced them.
    pub fn update(&mut self, events: &[Event]) {
        for event in events {
            self.push_event(event);
        }
    }

    /// Feeds a single event.
    pub fn push_event(&mut self, event: &Event) {
        match *event {
            Event::PointerButton {
                pos,
                button,
                pressed,
                ..
            } => {
                self.position = Some(pos);
                if pressed {
                    if !self.is_down(button) {
                        self.held.push((
                            button,
                            HeldButton {
                                origin: pos,
                                last: pos,
                            },
                        ));
                        self.pressed.push(button);
                    }
                } else if let Some(index) = self.held.iter().position(|(b, _)| *b == button) {
                    let (_, held) = self.held.remove(index);
                    self.released.push((
                        button,
                        ReleaseInfo {
                            position: pos,
                            travelled: (pos - held.origin).length(),
                        },
                    ));
                }
            }
            Event::PointerMoved(pos) => {
                if let Some(previous) = self.position {
                    let delta = pos - previous;
                    if delta != Vec2::ZERO {
                        if !self.held.is_empty() {
                            self.frame_delta += delta;
                        }
                        for (_, held) in &mut self.held {
                            held.last = pos;
                        }
                    }
                }
                self.position = Some(pos);
            }
            Event::PointerGone => self.reset(),
            _ => {}
        }
    }

    /// Total pointer movement accumulated during this frame, or `ZERO` when no
    /// button is held.
    #[must_use]
    pub fn frame_delta(&self) -> Vec2 {
        self.frame_delta
    }

    /// Where the pointer is, if the window has seen it.
    #[must_use]
    pub fn position(&self) -> Option<Pos2> {
        self.position
    }

    /// `true` if `button` went down during this frame.
    #[must_use]
    pub fn pressed(&self, button: PointerButton) -> bool {
        self.pressed.contains(&button)
    }

    /// `true` if `button` came up during this frame.
    #[must_use]
    pub fn released(&self, button: PointerButton) -> bool {
        self.released.iter().any(|(b, _)| *b == button)
    }

    /// `true` if `button` is currently held down.
    #[must_use]
    pub fn down(&self, button: PointerButton) -> bool {
        self.is_down(button)
    }

    /// Position where `button` went down.
    #[must_use]
    pub fn origin(&self, button: PointerButton) -> Option<Pos2> {
        self.held
            .iter()
            .find(|(b, _)| *b == button)
            .map(|(_, h)| h.origin)
    }

    /// Where `button` was when it came up (only valid during the release
    /// frame).
    #[must_use]
    pub fn release_position(&self, button: PointerButton) -> Option<Pos2> {
        self.released
            .iter()
            .find(|(b, _)| *b == button)
            .map(|(_, r)| r.position)
    }

    /// Distance travelled by `button` since it went down.
    ///
    /// Works both while the button is held ([`PointerInput::travelled`]) and
    /// during the frame it was released ([`PointerInput::release_travelled`]).
    #[must_use]
    pub fn travelled(&self, button: PointerButton) -> f32 {
        self.held
            .iter()
            .find(|(b, _)| *b == button)
            .map_or(0.0, |(_, h)| (h.last - h.origin).length())
    }

    /// Distance travelled by `button` at the moment it was released.
    #[must_use]
    pub fn release_travelled(&self, button: PointerButton) -> Option<f32> {
        self.released
            .iter()
            .find(|(b, _)| *b == button)
            .map(|(_, r)| r.travelled)
    }

    /// Movement of `button` during this frame (`ZERO` if it is not held).
    #[must_use]
    pub fn delta(&self, button: PointerButton) -> Vec2 {
        if self.is_down(button) {
            self.frame_delta
        } else {
            Vec2::ZERO
        }
    }

    /// Every button that is currently held down.
    pub fn held_now(&self) -> impl Iterator<Item = PointerButton> + '_ {
        self.held.iter().map(|(b, _)| *b)
    }

    fn is_down(&self, button: PointerButton) -> bool {
        self.held.iter().any(|(b, _)| *b == button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, pos2};

    fn press(button: PointerButton, pos: Pos2) -> Event {
        Event::PointerButton {
            pos,
            button,
            pressed: true,
            modifiers: Modifiers::default(),
        }
    }

    fn release(button: PointerButton, pos: Pos2) -> Event {
        Event::PointerButton {
            pos,
            button,
            pressed: false,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn tracks_primary_click() {
        let mut p = PointerInput::new();
        p.begin_frame();
        p.update(&[press(PointerButton::Primary, pos2(10.0, 10.0))]);
        assert!(p.pressed(PointerButton::Primary));
        assert!(p.down(PointerButton::Primary));
        assert_eq!(p.origin(PointerButton::Primary), Some(pos2(10.0, 10.0)));
        assert_eq!(p.position(), Some(pos2(10.0, 10.0)));

        p.begin_frame();
        p.update(&[release(PointerButton::Primary, pos2(12.0, 10.0))]);
        assert!(p.released(PointerButton::Primary));
        assert_eq!(
            p.release_position(PointerButton::Primary),
            Some(pos2(12.0, 10.0))
        );
        assert!((p.release_travelled(PointerButton::Primary).unwrap() - 2.0).abs() < 1e-5);
        assert!(!p.down(PointerButton::Primary));
        assert!(!p.pressed(PointerButton::Primary));
    }

    #[test]
    fn tracks_two_buttons_at_once() {
        let mut p = PointerInput::new();
        p.begin_frame();
        p.update(&[
            press(PointerButton::Primary, pos2(0.0, 0.0)),
            press(PointerButton::Secondary, pos2(0.0, 0.0)),
        ]);
        assert!(p.down(PointerButton::Primary));
        assert!(p.down(PointerButton::Secondary));

        p.begin_frame();
        p.update(&[Event::PointerMoved(pos2(7.0, 0.0))]);
        assert_eq!(p.delta(PointerButton::Primary), Vec2::new(7.0, 0.0));
        assert_eq!(p.delta(PointerButton::Secondary), Vec2::new(7.0, 0.0));
        assert_eq!(p.frame_delta(), Vec2::new(7.0, 0.0));
        assert!((p.travelled(PointerButton::Primary) - 7.0).abs() < 1e-5);
        assert!((p.travelled(PointerButton::Secondary) - 7.0).abs() < 1e-5);
    }

    #[test]
    fn hovering_does_not_produce_a_delta() {
        let mut p = PointerInput::new();
        p.begin_frame();
        p.update(&[Event::PointerMoved(pos2(5.0, 5.0))]);
        assert_eq!(p.frame_delta(), Vec2::ZERO);
        assert_eq!(p.delta(PointerButton::Primary), Vec2::ZERO);
        assert!(!p.down(PointerButton::Primary));
    }

    #[test]
    fn deltas_accumulate_within_a_frame_and_reset_next_frame() {
        let mut p = PointerInput::new();
        p.begin_frame();
        p.update(&[press(PointerButton::Primary, pos2(0.0, 0.0))]);
        p.update(&[
            Event::PointerMoved(pos2(1.0, 0.0)),
            Event::PointerMoved(pos2(2.0, 0.0)),
            Event::PointerMoved(pos2(3.0, 0.0)),
        ]);
        assert_eq!(p.frame_delta(), Vec2::new(3.0, 0.0));

        p.begin_frame();
        p.update(&[Event::PointerMoved(pos2(5.0, 0.0))]);
        assert_eq!(p.frame_delta(), Vec2::new(2.0, 0.0));
    }

    #[test]
    fn repeated_press_events_are_reported_once() {
        let mut p = PointerInput::new();
        p.begin_frame();
        p.update(&[
            press(PointerButton::Primary, pos2(0.0, 0.0)),
            press(PointerButton::Primary, pos2(0.0, 0.0)),
        ]);
        assert!(p.pressed(PointerButton::Primary));
        assert!(p.down(PointerButton::Primary));
    }

    #[test]
    fn pointer_gone_resets() {
        let mut p = PointerInput::new();
        p.begin_frame();
        p.update(&[
            press(PointerButton::Primary, pos2(0.0, 0.0)),
            Event::PointerGone,
        ]);
        assert!(!p.down(PointerButton::Primary));
        assert_eq!(p.position(), None);
        assert_eq!(p.frame_delta(), Vec2::ZERO);
    }

    #[test]
    fn unknown_events_are_ignored() {
        let mut p = PointerInput::new();
        p.begin_frame();
        p.update(&[
            Event::Copy,
            Event::Text("a".into()),
            Event::ModifiersChanged(Modifiers::default()),
            Event::Zoom(1.0),
        ]);
        assert_eq!(p.position(), None);
        assert!(!p.down(PointerButton::Primary));
    }
}
