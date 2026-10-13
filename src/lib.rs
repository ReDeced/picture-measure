//! # picture-measure
//!
//! Core, UI-independent model of an image measuring session plus the geometry
//! helpers used by the mouse interaction layer.
//!
//! The crate is split so that everything that can be unit-tested lives in the
//! library, while the `eframe`/`egui` shell lives in the binary crate:
//!
//! * [`geometry`] — axis-aligned rectangles and vector math.
//! * [`model`] — points, rectangles, annotations and the annotation set.
//! * [`view`] — the pan/zoom transform between screen and image coordinates.
//! * [`tools`] — the canvas state machine (click, drag, shift-square, ...).
//! * [`pointer`] — reconstruction of multi-button mouse input from `egui` events.
//! * [`history`] — undo/redo stack.
//! * [`session`] — the whole interaction pipeline, from raw events to edits.
//! * [`json_io`] — the on-disk JSON format (export *and* import).
//! * [`split`] — one-JSON-file-per-figure export into sub-folders.

// `assert_is_empty` появился в clippy 1.99, то есть позже нашего MSRV (1.95).
// На старых тулчейнах линт ещё не существует, поэтому сначала разрешаем
// `unknown_lints`, и только потом сам линт. На новых — предупреждений нет.
// Проверки `assert!(x.is_empty())` на коллекциях читаются как есть,
// переписывать их в `assert_eq!(len, 0)` незачем.
#![allow(unknown_lints)]
#![allow(clippy::assert_is_empty)]

pub mod geometry;
pub mod history;
pub mod json_io;
pub mod model;
pub mod pointer;
pub mod session;
pub mod split;
pub mod tools;
pub mod view;

/// Default prefix used for the auto-generated name of a point annotation.
pub const POINT_PREFIX: &str = "point";
/// Default prefix used for the auto-generated name of a rectangle annotation.
pub const RECT_PREFIX: &str = "rect";

/// Builds the default auto-generated annotation name.
///
/// `N` is the stable, never-reused [`model::ShapeId`] of the figure, so a name
/// never changes when unrelated annotations are added or removed.
#[must_use]
pub fn default_name(kind: model::ShapeKind, id: model::ShapeId) -> String {
    format!("{}_{id}", kind.prefix())
}

/// Short version string used in the UI status bar.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
