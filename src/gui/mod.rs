//! Application shell: panels, canvas painting and input routing.
//!
//! The binary crate is intentionally thin — all measurement logic lives in the
//! `picture_measure` library so it can be unit-tested without a window.

mod app;
mod canvas;
mod panels;

pub use app::{App, CliArgs};

/// Runs the GUI.
///
/// # Errors
///
/// Returns an error when the window cannot be created.
pub fn run(options: eframe::NativeOptions, cli: app::CliArgs) -> eframe::Result<()> {
    eframe::run_native(
        "picture-measure",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, cli)))),
    )
}
