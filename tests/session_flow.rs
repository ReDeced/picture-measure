//! End-to-end tests: build a session with real pointer input, export it to
//! JSON and read it back.
//!
//! These exercise the whole pipeline the way the GUI does, but headless.

// Пояснение к `assert_is_empty` — в src/lib.rs.
#![allow(unknown_lints)]
#![allow(clippy::assert_is_empty)]

use egui::{Event, PointerButton, Pos2, Rect as EguiRect, Vec2, pos2, vec2};
use picture_measure::json_io;
use picture_measure::model::{AnnotationSet, Geometry};
use picture_measure::session::Session;
use picture_measure::tools::Tool;
use picture_measure::view::ViewTransform;

const CANVAS: EguiRect = EguiRect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));

/// A scripted user: feeds events frame by frame, tracking button state the way
/// a window backend would.
struct User {
    session: Session,
    backend: Vec<PointerButton>,
}

impl User {
    fn new() -> Self {
        Self {
            session: Session::new(),
            backend: Vec::new(),
        }
    }

    /// Feeds one frame of events.
    fn frame(&mut self, events: &[Event], mods: egui::Modifiers) {
        for event in events {
            if let Event::PointerButton {
                button, pressed, ..
            } = event
            {
                if *pressed {
                    if !self.backend.contains(button) {
                        self.backend.push(*button);
                    }
                } else {
                    self.backend.retain(|b| b != button);
                }
            }
        }
        self.session
            .handle_events(events, mods, CANVAS, &self.backend);
    }

    /// Moves the cursor without any button held.
    fn hover(&mut self, to: Pos2) {
        self.frame(&[Event::PointerMoved(to)], egui::Modifiers::default());
    }

    /// Presses, moves through `via`, and releases at `to`.
    fn drag(&mut self, from: Pos2, via: &[Pos2], to: Pos2, mods: egui::Modifiers) {
        self.frame(
            &[Event::PointerButton {
                pos: from,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: mods,
            }],
            mods,
        );
        for step in via {
            self.frame(&[Event::PointerMoved(*step)], mods);
        }
        self.frame(
            &[Event::PointerButton {
                pos: to,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: mods,
            }],
            mods,
        );
    }

    /// A click: press and release at the same spot.
    fn click(&mut self, at: Pos2) {
        self.drag(at, &[], at, egui::Modifiers::default());
    }

    /// A click with modifier keys held.
    fn click_with(&mut self, at: Pos2, mods: egui::Modifiers) {
        self.drag(at, &[], at, mods);
    }

    /// Presses the secondary button and drags, i.e. pans.
    fn pan(&mut self, from: Pos2, to: Pos2) {
        self.frame(
            &[Event::PointerButton {
                pos: from,
                button: PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
            egui::Modifiers::default(),
        );
        self.frame(&[Event::PointerMoved(to)], egui::Modifiers::default());
        self.frame(
            &[Event::PointerButton {
                pos: to,
                button: PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
            egui::Modifiers::default(),
        );
    }

    /// Scrolls the wheel `lines` notches at `at`.
    fn scroll(&mut self, at: Pos2, lines: f32) {
        self.hover(at);
        self.frame(
            &[Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, lines * 50.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            }],
            egui::Modifiers::default(),
        );
    }

    fn shift() -> egui::Modifiers {
        egui::Modifiers {
            shift: true,
            ..Default::default()
        }
    }
}

fn shift() -> egui::Modifiers {
    User::shift()
}

/// Asserts two sets describe the same figures, allowing for the rounding the
/// JSON writer applies to coordinates.
#[track_caller]
fn assert_same_figures(left: &AnnotationSet, right: &AnnotationSet) {
    assert_eq!(left.len(), right.len(), "разное число фигур");
    for (a, b) in left.iter().zip(right) {
        assert_eq!(a.id, b.id, "id");
        assert_eq!(a.display_name(), b.display_name(), "имя");
        assert_eq!(a.folder(), b.folder(), "папка");
        match (a.geometry, b.geometry) {
            (Geometry::Point(p), Geometry::Point(q)) => assert!(
                p.distance(q) < 0.01,
                "точки {p:?} и {q:?} разошлись больше чем на 0.01"
            ),
            (Geometry::Rect(r), Geometry::Rect(s)) => {
                for (x, y) in std::iter::zip(r.to_xywh(), s.to_xywh()) {
                    assert!(
                        (x - y).abs() < 0.01,
                        "прямоугольники {r:?} и {s:?} разошлись"
                    );
                }
            }
            _ => panic!("разные типы фигур: {:?} и {:?}", a.geometry, b.geometry),
        }
    }
}

// ----------------------------------------------------------------------
// The three core gestures from the specification
// ----------------------------------------------------------------------

#[test]
fn single_click_marks_a_point() {
    let mut user = User::new();
    user.click(pos2(120.0, 80.0));

    let set = user.session.set();
    assert_eq!(set.len(), 1);
    assert_eq!(set.items()[0].geometry, Geometry::Point(pos2(120.0, 80.0)));
    assert_eq!(set.items()[0].kind().as_str(), "point");
}

#[test]
fn press_and_drag_marks_a_rectangle() {
    let mut user = User::new();
    user.drag(
        pos2(50.0, 60.0),
        &[pos2(70.0, 75.0)],
        pos2(90.0, 80.0),
        egui::Modifiers::default(),
    );

    let rect = user.session.set().items()[0].geometry.as_rect().unwrap();
    assert_eq!(rect.to_xywh(), [50.0, 60.0, 40.0, 20.0]);
    assert_eq!(user.session.set().items()[0].kind().as_str(), "rect");
}

#[test]
fn shift_makes_the_rectangle_a_square() {
    let mut user = User::new();
    user.drag(
        pos2(100.0, 100.0),
        &[pos2(160.0, 120.0)],
        pos2(200.0, 130.0),
        shift(),
    );

    let rect = user.session.set().items()[0].geometry.as_rect().unwrap();
    assert_eq!(rect.width(), 100.0, "сторона равна движению по X");
    assert_eq!(rect.height(), 100.0, "…и по Y тоже");
    assert!(rect.is_square(1e-4), "{rect:?}");
}

#[test]
fn shift_drag_works_in_every_direction() {
    for (from, to) in [
        (pos2(300.0, 300.0), pos2(250.0, 340.0)),
        (pos2(300.0, 300.0), pos2(280.0, 220.0)),
        (pos2(300.0, 300.0), pos2(220.0, 260.0)),
        (pos2(300.0, 300.0), pos2(360.0, 380.0)),
    ] {
        let mut user = User::new();
        user.drag(from, &[], to, shift());
        let rect = user.session.set().items()[0].geometry.as_rect().unwrap();
        assert!(rect.is_square(1e-4), "{from} → {to}: {rect:?}");
        assert!(rect.contains(from), "квадрат растёт из точки нажатия");
    }
}

#[test]
fn a_tiny_drag_is_still_a_point() {
    let mut user = User::new();
    // Below the 4px threshold: a point, not a 1px rectangle.
    user.drag(
        pos2(200.0, 200.0),
        &[pos2(201.0, 201.0)],
        pos2(202.0, 200.5),
        egui::Modifiers::default(),
    );
    assert_eq!(user.session.set().len(), 1);
    assert_eq!(
        user.session.set().items()[0].geometry,
        Geometry::Point(pos2(200.0, 200.0))
    );
}

// ----------------------------------------------------------------------
// Naming and folders
// ----------------------------------------------------------------------

#[test]
fn default_names_are_point_n_and_rect_n() {
    let mut user = User::new();
    user.click(pos2(10.0, 10.0));
    user.drag(
        pos2(0.0, 0.0),
        &[],
        pos2(30.0, 30.0),
        egui::Modifiers::default(),
    );
    user.click(pos2(20.0, 20.0));
    user.drag(pos2(0.0, 0.0), &[], pos2(15.0, 15.0), shift());

    let names: Vec<String> = user
        .session
        .set()
        .iter()
        .map(picture_measure::model::Annotation::display_name)
        .collect();
    assert_eq!(
        names,
        ["point_1", "rect_2", "point_3", "rect_4"],
        "N — сквозной номер фигуры"
    );
}

#[test]
fn deleted_numbers_are_not_reused() {
    let mut user = User::new();
    user.click(pos2(10.0, 10.0));
    user.click(pos2(20.0, 20.0));
    user.session.delete_one(1);
    user.click(pos2(30.0, 30.0));
    let names: Vec<String> = user
        .session
        .set()
        .iter()
        .map(picture_measure::model::Annotation::display_name)
        .collect();
    assert_eq!(names, ["point_2", "point_3"]);
}

#[test]
fn renumbering_makes_names_consecutive() {
    let mut user = User::new();
    user.click(pos2(10.0, 10.0));
    user.click(pos2(20.0, 20.0));
    user.click(pos2(30.0, 30.0));
    user.session.delete_one(2);
    user.session.renumber();
    let names: Vec<String> = user
        .session
        .set()
        .iter()
        .map(picture_measure::model::Annotation::display_name)
        .collect();
    assert_eq!(names, ["point_1", "point_2"]);
}

#[test]
fn custom_names_survive_renumbering() {
    let mut user = User::new();
    let id = user.session.add_point(pos2(1.0, 1.0));
    user.session.set_name(id, "нос");
    user.session.add_point(pos2(2.0, 2.0));
    user.session.renumber();
    let names: Vec<String> = user
        .session
        .set()
        .iter()
        .map(picture_measure::model::Annotation::display_name)
        .collect();
    assert_eq!(names, ["нос", "point_2"]);
}

#[test]
fn folders_apply_to_new_figures_and_can_be_changed_later() {
    let mut user = User::new();
    user.session.set_new_folder("eyes");
    user.click(pos2(10.0, 10.0));
    assert_eq!(user.session.set().items()[0].folder(), "eyes");

    user.session.set_new_folder("");
    user.click(pos2(20.0, 20.0));
    assert_eq!(user.session.set().items()[1].folder(), "");

    // Move the first figure to a folder after the fact.
    user.session.set_folder(1, "nose");
    assert_eq!(user.session.set().items()[0].folder(), "nose");
    assert_eq!(user.session.set().folders(), vec!["nose".to_owned()]);
}

#[test]
fn nested_folders_are_supported() {
    let mut user = User::new();
    user.session.set_new_folder("faces/eyes/left");
    user.click(pos2(10.0, 10.0));
    let folder = user.session.set().items()[0].folder();
    assert_eq!(folder, "faces/eyes/left");
    // Backslashes are normalised so the export works on every platform.
    user.session.set_folder(1, r"faces\eyes\right");
    assert_eq!(user.session.set().items()[0].folder(), "faces/eyes/right");
}

#[test]
fn a_folder_can_be_applied_to_a_whole_selection() {
    let mut user = User::new();
    let a = user.session.add_point(pos2(1.0, 1.0));
    let b = user.session.add_point(pos2(2.0, 2.0));
    user.session.add_point(pos2(3.0, 3.0));
    user.session.select_only(a);
    user.session.toggle_selection(b);
    assert_eq!(user.session.selection().len(), 2);
    user.session.set_selection_folder("paired");
    assert_eq!(user.session.set().get(a).unwrap().folder(), "paired");
    assert_eq!(user.session.set().get(b).unwrap().folder(), "paired");
    assert_eq!(user.session.set().get(3).unwrap().folder(), "");
}

// ----------------------------------------------------------------------
// Editing gestures
// ----------------------------------------------------------------------

#[test]
fn selecting_with_the_select_tool() {
    let mut user = User::new();
    // Two figures, drawn with the draw tool.
    user.click(pos2(50.0, 50.0));
    user.click(pos2(80.0, 80.0));
    assert_eq!(user.session.set().len(), 2);

    user.session.set_tool(Tool::Select);

    // Clicking empty space clears the selection.
    user.click(pos2(500.0, 500.0));
    assert!(user.session.selection().is_empty());

    // Clicking a figure selects exactly it.
    user.click(pos2(50.0, 50.0));
    assert_eq!(user.session.selection(), &[1]);

    // Clicking another one replaces the selection.
    user.click(pos2(80.0, 80.0));
    assert_eq!(user.session.selection(), &[2]);

    // Shift-clicking adds to the selection.
    user.click_with(pos2(50.0, 50.0), shift());
    assert_eq!(user.session.selection().len(), 2);

    // Shift-clicking it again removes it.
    user.click_with(pos2(50.0, 50.0), shift());
    assert_eq!(user.session.selection(), &[2]);
}

#[test]
fn moving_a_selected_figure() {
    let mut user = User::new();
    user.session.add_point(pos2(100.0, 100.0));
    user.session.set_tool(Tool::Select);

    user.drag(
        pos2(100.0, 100.0),
        &[pos2(110.0, 105.0)],
        pos2(120.0, 110.0),
        egui::Modifiers::default(),
    );
    assert_eq!(
        user.session.set().items()[0].geometry,
        Geometry::Point(pos2(120.0, 110.0))
    );
}

#[test]
fn moving_with_the_select_tool_does_not_draw_a_rectangle() {
    let mut user = User::new();
    user.session.add_point(pos2(100.0, 100.0));
    user.session.set_tool(Tool::Select);
    user.drag(
        pos2(100.0, 100.0),
        &[],
        pos2(200.0, 200.0),
        egui::Modifiers::default(),
    );
    assert_eq!(user.session.set().len(), 1, "инструмент «выбор» не рисует");
    assert_eq!(
        user.session.set().items()[0].geometry,
        Geometry::Point(pos2(200.0, 200.0))
    );
}

#[test]
fn deleting_with_the_delete_tool() {
    let mut user = User::new();
    user.click(pos2(10.0, 10.0));
    user.drag(
        pos2(0.0, 0.0),
        &[],
        pos2(40.0, 40.0),
        egui::Modifiers::default(),
    );
    user.session.set_tool(Tool::Delete);
    user.click(pos2(10.0, 10.0));
    assert_eq!(user.session.set().len(), 1);
    user.click(pos2(20.0, 20.0));
    assert!(user.session.set().is_empty());
}

#[test]
fn select_all_and_clear() {
    let mut user = User::new();
    user.session.add_point(pos2(1.0, 1.0));
    user.session.add_point(pos2(2.0, 2.0));
    user.session.select_all();
    assert_eq!(user.session.selection().len(), 2);
    user.session.clear_selection();
    assert!(user.session.selection().is_empty());
}

// ----------------------------------------------------------------------
// View
// ----------------------------------------------------------------------

#[test]
fn panning_with_the_right_button() {
    let mut user = User::new();
    user.click(pos2(100.0, 100.0));
    let before = user.session.view();
    user.pan(pos2(400.0, 300.0), pos2(450.0, 320.0));
    let after = user.session.view();
    assert_ne!(before, after);
    assert_eq!(after.offset, before.offset + vec2(50.0, 20.0));
    // The figure stays at the same image coordinates.
    assert_eq!(
        user.session.set().items()[0].geometry,
        Geometry::Point(pos2(100.0, 100.0))
    );
}

#[test]
fn panning_with_the_middle_button() {
    let mut user = User::new();
    let before = user.session.view();
    user.frame(
        &[Event::PointerButton {
            pos: pos2(300.0, 300.0),
            button: PointerButton::Middle,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        }],
        egui::Modifiers::default(),
    );
    user.frame(
        &[Event::PointerMoved(pos2(310.0, 300.0))],
        egui::Modifiers::default(),
    );
    assert_eq!(user.session.view().offset, before.offset + vec2(10.0, 0.0));
}

#[test]
fn alt_pans_instead_of_drawing() {
    let mut user = User::new();
    let mods = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let before = user.session.view();
    user.drag(
        pos2(300.0, 300.0),
        &[pos2(320.0, 300.0)],
        pos2(340.0, 300.0),
        mods,
    );
    assert!(user.session.set().is_empty(), "Alt + ЛКМ не рисует");
    assert_eq!(
        user.session.view().offset,
        before.offset + vec2(40.0, 0.0),
        "картинка уехала вместе с курсором"
    );
}

#[test]
fn the_pan_tool_never_draws() {
    let mut user = User::new();
    user.session.set_tool(Tool::Pan);
    user.drag(
        pos2(300.0, 300.0),
        &[pos2(340.0, 320.0)],
        pos2(380.0, 340.0),
        egui::Modifiers::default(),
    );
    assert!(user.session.set().is_empty());
}

#[test]
fn the_wheel_zooms_at_the_cursor() {
    let mut user = User::new();
    let anchor = pos2(500.0, 400.0);
    let image_under_cursor = user.session.image_under(anchor);

    user.scroll(anchor, 3.0);
    assert!(user.session.view().scale > 1.0, "колесо вверх увеличивает");
    assert!(
        user.session
            .image_under(anchor)
            .distance(image_under_cursor)
            < 1e-3,
        "точка под курсором не должна уезжать"
    );

    user.scroll(anchor, -6.0);
    assert!(user.session.view().scale < 1.0, "колесо вниз уменьшает");
}

#[test]
fn fitting_the_view_centres_the_image() {
    let mut user = User::new();
    user.session.fit_view(CANVAS, vec2(400.0, 200.0));
    let view = user.session.view();
    let screen = view.to_screen(pos2(200.0, 100.0));
    assert!(
        screen.distance(CANVAS.center()) < 0.5,
        "{screen:?} != {:?}",
        CANVAS.center()
    );
    // 1000x0.96 / 400 = 2.4; the height is not the limiting side.
    assert!((view.scale - 2.4).abs() < 1e-3, "{}", view.scale);
}

// ----------------------------------------------------------------------
// Coordinates stay in image space under zoom
// ----------------------------------------------------------------------

#[test]
fn figures_placed_while_zoomed_use_image_coordinates() {
    let mut user = User::new();
    user.session.zoom_by(4.0, pos2(0.0, 0.0));
    user.session.pan_by(vec2(100.0, 50.0));
    // Pick a screen spot and click it.
    let target_image = pos2(33.0, 77.0);
    let screen = user.session.screen_of(target_image);
    user.click(screen);

    let placed = user.session.set().items()[0].geometry.as_point().unwrap();
    assert!(
        placed.distance(target_image) < 1e-3,
        "клик по экрану {screen:?} дал {placed:?} вместо {target_image:?}"
    );
}

// ----------------------------------------------------------------------
// Undo / redo
// ----------------------------------------------------------------------

#[test]
fn every_gesture_is_undoable() {
    let mut user = User::new();
    user.click(pos2(10.0, 10.0));
    assert_eq!(user.session.set().len(), 1);
    assert!(user.session.undo());
    assert!(user.session.set().is_empty());

    user.drag(
        pos2(0.0, 0.0),
        &[],
        pos2(40.0, 40.0),
        egui::Modifiers::default(),
    );
    assert_eq!(user.session.set().len(), 1);
    assert!(user.session.undo());
    assert!(user.session.set().is_empty());

    assert!(user.session.redo());
    assert_eq!(user.session.set().len(), 1);
    assert_eq!(user.session.set().items()[0].kind().as_str(), "rect");
}

#[test]
fn undo_redo_through_a_whole_session() {
    let mut user = User::new();
    user.click(pos2(10.0, 10.0));
    user.drag(pos2(0.0, 0.0), &[], pos2(30.0, 30.0), shift());
    user.click(pos2(50.0, 50.0));
    assert_eq!(user.session.set().len(), 3);

    for expected in [2, 1, 0] {
        assert!(user.session.undo());
        assert_eq!(user.session.set().len(), expected);
    }
    for expected in [1, 2, 3] {
        assert!(user.session.redo());
        assert_eq!(user.session.set().len(), expected);
    }
    assert!(!user.session.redo());
}

#[test]
fn renaming_is_undoable() {
    let mut user = User::new();
    let id = user.session.add_point(pos2(1.0, 1.0));
    user.session.set_name(id, "особое");
    assert_eq!(user.session.set().get(id).unwrap().display_name(), "особое");
    assert!(user.session.undo());
    assert_eq!(
        user.session.set().get(id).unwrap().display_name(),
        "point_1"
    );
}

// ----------------------------------------------------------------------
// JSON export and re-import
// ----------------------------------------------------------------------

/// Measures a few figures the way a user would.
fn measured() -> AnnotationSet {
    let mut user = User::new();
    user.session.set_new_folder("eyes");
    user.click(pos2(120.0, 80.0));
    user.drag(
        pos2(10.0, 20.0),
        &[],
        pos2(110.0, 120.0),
        egui::Modifiers::default(),
    );
    user.drag(pos2(300.0, 300.0), &[], pos2(360.0, 280.0), shift());
    user.session.set_new_folder("");
    user.click(pos2(400.0, 420.0));
    let id = user.session.set().ids().last().unwrap();
    user.session.set_name(id, "угол");
    user.session.set().clone()
}

#[test]
fn export_produces_the_documented_json() {
    let set = measured();
    let json = json_io::to_string_pretty(&set, None, 2).unwrap();

    assert!(json.contains("\"format\": \"picture-measure\""), "{json}");
    assert!(json.contains("\"version\": 1"), "{json}");
    assert!(json.contains("\"coordinate_system\": \"pixel\""), "{json}");
    assert!(json.contains("\"origin\": \"top-left\""), "{json}");

    let file: serde_json::Value = serde_json::from_str(&json).unwrap();
    let items = file["annotations"].as_array().unwrap();
    assert_eq!(items.len(), 4);

    assert_eq!(items[0]["type"], "point");
    assert_eq!(items[0]["name"], "point_1");
    assert_eq!(items[0]["folder"], "eyes");
    assert_eq!(items[0]["x"], 120);
    assert_eq!(items[0]["y"], 80);

    assert_eq!(items[1]["type"], "rect");
    assert_eq!(items[1]["name"], "rect_2");
    assert_eq!(items[1]["x"], 10);
    assert_eq!(items[1]["y"], 20);
    assert_eq!(items[1]["width"], 100);
    assert_eq!(items[1]["height"], 100);

    // Shift-drag from (300,300) to (360,280): 60x60 square growing up-left.
    assert_eq!(items[2]["name"], "rect_3");
    assert_eq!(items[2]["x"], 300);
    assert_eq!(items[2]["y"], 240);
    assert_eq!(items[2]["width"], 60);
    assert_eq!(items[2]["height"], 60);

    assert_eq!(items[3]["name"], "угол");
    assert!(items[3].get("folder").is_none(), "пустая папка не пишется");

    assert_eq!(file["counts"]["total"], 4);
    assert_eq!(file["counts"]["points"], 2);
    assert_eq!(file["counts"]["rects"], 2);
    assert_eq!(file["folders"], serde_json::json!(["eyes"]));
}

#[test]
fn coordinates_are_integers_when_whole() {
    let mut user = User::new();
    user.click(pos2(10.0, 20.0));
    let json = json_io::to_string_compact(user.session.set(), None, 2).unwrap();
    assert!(json.contains("\"x\":10,\"y\":20"), "{json}");
    assert!(
        !json.contains("10.0"),
        "целые значения не пишутся с дробью: {json}"
    );
}

#[test]
fn precision_is_configurable() {
    let mut user = User::new();
    user.session.add_point(pos2(10.123456, 20.987654));

    let coarse = json_io::to_string_compact(user.session.set(), None, 0).unwrap();
    assert!(coarse.contains("\"x\":10"), "{coarse}");

    let fine = json_io::to_string_compact(user.session.set(), None, 4).unwrap();
    assert!(fine.contains("10.1235"), "{fine}");
}

#[test]
fn export_then_import_round_trips_exactly() {
    let set = measured();
    let json = json_io::to_string_pretty(&set, None, 4).unwrap();
    let restored = json_io::from_str(&json).unwrap();
    assert_same_figures(&restored, &set);
    assert_eq!(restored.counts(), set.counts());
    assert_eq!(restored.folders(), set.folders());
    // Integer coordinates survive a round trip bit-for-bit.
    assert_eq!(
        restored.get(2).unwrap().geometry,
        set.get(2).unwrap().geometry
    );
}

#[test]
fn exported_files_can_be_edited_and_saved_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("annotations.json");

    let set = measured();
    std::fs::write(&path, json_io::to_string_pretty(&set, None, 2).unwrap()).unwrap();

    let mut user = User::new();
    let text = std::fs::read_to_string(&path).unwrap();
    user.session.load(json_io::from_str(&text).unwrap());
    assert_eq!(user.session.set().len(), 4);

    // Add one more figure and save.
    user.click(pos2(500.0, 500.0));
    std::fs::write(
        &path,
        json_io::to_string_pretty(user.session.set(), None, 2).unwrap(),
    )
    .unwrap();

    let final_set = json_io::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(final_set.len(), 5);
    assert_eq!(final_set.items()[4].display_name(), "point_5");
}

#[test]
fn image_metadata_travels_with_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("cat.png");
    // A 1x1 PNG is enough: the exporter never opens it.
    std::fs::write(
        &image,
        [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ],
    )
    .unwrap();

    let info = json_io::ImageInfo::from_path(&image, 640, 480);
    let set = measured();
    let json = json_io::to_string_pretty(&set, Some(info), 2).unwrap();

    assert_eq!(json_io::peek_image_info(&json).unwrap().width, 640);
    assert_eq!(json_io::peek_image_info(&json).unwrap().name, "cat.png");
    assert!(json.contains("\"path\""), "{json}");
}

#[test]
fn hand_written_files_load() {
    let text = r#"[
        {"type": "point", "name": "нос", "x": 10, "y": 20, "folder": "лицо"},
        {"type": "rect", "name": "рот", "x": 1, "y": 2, "width": 30, "height": 15}
    ]"#;
    let set = json_io::from_str(text).unwrap();
    assert_eq!(set.len(), 2);
    assert_eq!(set.items()[0].display_name(), "нос");
    assert_eq!(set.items()[0].folder(), "лицо");
    assert_eq!(
        set.items()[1].geometry.as_rect().unwrap().to_xywh(),
        [1.0, 2.0, 30.0, 15.0]
    );
}

#[test]
fn broken_files_produce_helpful_errors() {
    for (text, needle) in [
        ("{ not json", "некорректный JSON"),
        ("42", "неподдерживаемая структура"),
        (r#"[{"type":"rect","x":0,"y":0}]"#, "width"),
        (r#"[{"type":"rect","x":0,"y":0,"width":3}]"#, "height"),
        (r#"[{"type":"polygon","x":0,"y":0}]"#, "неизвестный тип"),
        (
            r#"[{"type":"rect","x":0,"y":0,"width":-1,"height":2}]"#,
            "отрицательный",
        ),
    ] {
        let error = json_io::from_str(text).unwrap_err().to_string();
        assert!(
            error.contains(needle),
            "«{error}» должно упоминать «{needle}»"
        );
    }
}

// ----------------------------------------------------------------------
// Split export
// ----------------------------------------------------------------------

#[test]
fn split_export_lays_files_out_by_folder() {
    let dir = tempfile::tempdir().unwrap();
    let set = measured();
    let count = picture_measure::split::write_split_json(dir.path(), &set, 2).unwrap();
    assert_eq!(count, 4);

    assert!(dir.path().join("eyes").join("point_1.json").is_file());
    assert!(dir.path().join("eyes").join("rect_2.json").is_file());
    assert!(dir.path().join("eyes").join("rect_3.json").is_file());
    assert!(dir.path().join("угол.json").is_file());

    // Every file holds exactly one figure.
    let text = std::fs::read_to_string(dir.path().join("eyes/rect_2.json")).unwrap();
    let single = json_io::from_str(&text).unwrap();
    assert_eq!(single.len(), 1);
    assert_eq!(single.items()[0].display_name(), "rect_2");
}

#[test]
fn split_export_names_do_not_collide() {
    let dir = tempfile::tempdir().unwrap();
    let mut set = AnnotationSet::new();
    for _ in 0..3 {
        let id = set.add_point(pos2(0.0, 0.0), Some("same"));
        assert!(id < 10);
        set.get_mut(id).unwrap().set_name("одинаковое");
    }
    picture_measure::split::write_split_json(dir.path(), &set, 2).unwrap();

    let mut found: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in std::fs::read_dir(dir.path().join("same")).unwrap() {
        let path = entry.unwrap().path();
        found.insert(path.file_name().unwrap().to_string_lossy().into_owned());
    }
    assert_eq!(found.len(), 3, "три разных файла: {found:?}");
    assert!(found.contains("одинаковое.json"), "{found:?}");
    assert!(found.contains("одинаковое_2.json"), "{found:?}");
    assert!(found.contains("одинаковое_3.json"), "{found:?}");
}

#[test]
fn split_export_refuses_to_escape_the_output_directory() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let mut set = AnnotationSet::new();
    let id = set.add_point(pos2(0.0, 0.0), Some("../../escape"));
    set.get_mut(id).unwrap().set_name("../вне");
    picture_measure::split::write_split_json(&out, &set, 2).unwrap();

    // Nothing landed outside `out`.
    assert!(!dir.path().join("escape").exists());
    assert!(!dir.path().parent().unwrap().join("вне.json").exists());
    assert!(!out.parent().unwrap().join(".._вне.json").exists());

    // The traversal components were dropped and the file is inside `out`.
    assert!(out.join("escape").join("_вне.json").is_file(), "{out:?}");
}

// ----------------------------------------------------------------------
// The whole flow, start to finish
// ----------------------------------------------------------------------

#[test]
fn a_realistic_annotating_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut user = User::new();

    // 1. Fit a 1920x1080 screenshot into the canvas and zoom in on a region.
    user.session.fit_view(CANVAS, vec2(1920.0, 1080.0));
    user.session.zoom_by(6.0, pos2(500.0, 400.0));

    // 2. Name every figure in a sub-folder and measure three objects.
    //    The mouse works in screen pixels; the annotation is stored in image
    //    pixels, so the zoom factor has to be taken into account.
    let view = user.session.view();

    user.session.set_new_folder("parts");
    user.click(pos2(300.0, 300.0)); // point_1 — a hole
    let board_from = pos2(100.0, 100.0);
    let board_to = pos2(260.0, 190.0);
    user.drag(
        board_from,
        &[pos2(180.0, 150.0)],
        board_to,
        egui::Modifiers::default(),
    ); // rect_2
    let sq_from = pos2(700.0, 700.0);
    let sq_to = pos2(820.0, 640.0);
    user.drag(sq_from, &[pos2(760.0, 660.0)], sq_to, shift()); // rect_3, square
    user.session.set_name(1, "отверстие");
    user.session.set_name(2, "плата");

    // 3. The square really is square in *image* coordinates.
    let square = user
        .session
        .set()
        .get(3)
        .unwrap()
        .geometry
        .as_rect()
        .unwrap();
    assert!(square.is_square(1e-2), "{square:?}");
    let drag = sq_to - sq_from;
    let expected = drag.x.abs().max(drag.y.abs()) / view.scale;
    assert!(
        (square.width() - expected).abs() < 1e-2,
        "сторона {expected} != {}",
        square.width()
    );
    // And the rectangle kept its dragged proportions.
    let board = user
        .session
        .set()
        .get(2)
        .unwrap()
        .geometry
        .as_rect()
        .unwrap();
    let dx = (board_to.x - board_from.x) / view.scale;
    let dy = (board_to.y - board_from.y) / view.scale;
    assert!((board.width() - dx).abs() < 1e-2, "{board:?} vs dx={dx}");
    assert!((board.height() - dy).abs() < 1e-2, "{board:?} vs dy={dy}");

    // 4. Save.
    let path = dir.path().join("parts.json");
    std::fs::write(
        &path,
        json_io::to_string_pretty(user.session.set(), None, 2).unwrap(),
    )
    .unwrap();

    // 5. Reload into a fresh session and confirm nothing was lost.
    let mut fresh = User::new();
    fresh
        .session
        .load(json_io::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap());

    assert_same_figures(fresh.session.set(), user.session.set());
    let names: Vec<String> = fresh
        .session
        .set()
        .iter()
        .map(picture_measure::model::Annotation::display_name)
        .collect();
    assert_eq!(names, ["отверстие", "плата", "rect_3"]);
    assert_eq!(fresh.session.set().folders(), vec!["parts".to_owned()]);
}

// ----------------------------------------------------------------------
// Robustness
// ----------------------------------------------------------------------

#[test]
fn clicking_outside_the_canvas_does_nothing() {
    let mut user = User::new();
    // Left of the canvas: that area belongs to the side panel.
    user.click(pos2(-50.0, 100.0));
    user.click(pos2(1500.0, 100.0));
    assert!(user.session.set().is_empty());
    assert!(!user.session.is_dirty());
}

#[test]
fn a_mixed_session_keeps_counts_consistent() {
    let mut user = User::new();
    for i in 0..5 {
        user.click(pos2(i as f32 * 10.0 + 5.0, 50.0));
        user.drag(
            pos2(0.0, 0.0),
            &[],
            pos2(i as f32 * 20.0 + 10.0, 30.0),
            egui::Modifiers::default(),
        );
    }
    let counts = user.session.counts();
    assert_eq!(counts.total, 10);
    assert_eq!(counts.points, 5);
    assert_eq!(counts.rects, 5);
    assert_eq!(user.session.set().ids().count(), 10);
}

#[test]
fn rapid_clicks_produce_distinct_points() {
    let mut user = User::new();
    for i in 0..20 {
        user.click(pos2(i as f32 * 3.0, 100.0));
    }
    assert_eq!(user.session.set().len(), 20);
    let ids: Vec<_> = user.session.set().ids().collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "все id различны");
}

#[test]
fn the_view_transform_inverts_cleanly() {
    let mut view = ViewTransform::identity();
    view.zoom_by(3.7, pos2(120.0, 340.0));
    view.pan_by(vec2(-55.0, 22.0));
    for image in [pos2(0.0, 0.0), pos2(1.0, 1.0), pos2(1920.0, 1080.0)] {
        let back = view.to_image(view.to_screen(image));
        assert!(back.distance(image) < 1e-2, "{back:?} != {image:?}");
    }
    assert!(view.to_image_vec(vec2(10.0, 0.0)).x < 10.0);
    assert_eq!(Vec2::splat(0.0), Vec2::ZERO);
}
