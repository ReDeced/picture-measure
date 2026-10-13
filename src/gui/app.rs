//! The `eframe::App` implementation.
//!
//! All the state lives in [`picture_measure::session::Session`]; this file only
//! owns what is strictly about the window: the texture, the file paths and the
//! little bits of widget state.

use std::path::{Path, PathBuf};

use egui::{
    Color32, Context, FontId, Key, Rect as EguiRect, TextureHandle, TextureOptions, Ui, Vec2, vec2,
};
use picture_measure::json_io::{self, ImageInfo};
use picture_measure::model::ShapeId;
use picture_measure::session::Session;

/// Files passed on the command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliArgs {
    /// Image to open on start-up.
    pub image: Option<PathBuf>,
    /// JSON file with annotations to load on start-up.
    pub annotations: Option<PathBuf>,
}

/// The whole application.
pub struct App {
    /// Everything the measurement session needs.
    pub(crate) session: Session,
    /// Decoded image, uploaded to the GPU.
    pub(crate) texture: Option<TextureHandle>,
    /// Where the image came from.
    pub(crate) image_path: Option<PathBuf>,
    /// Display name of the image.
    pub(crate) image_name: String,
    /// Size of the image in pixels.
    pub(crate) image_size: Vec2,

    /// Text field with the image path to open.
    pub(crate) open_path: String,
    /// Text field with the JSON path to write.
    pub(crate) json_path: String,
    /// Decimal places written to JSON.
    pub(crate) precision: u32,
    /// Draw the name of every figure.
    pub(crate) show_labels: bool,
    /// Draw the coordinate grid.
    pub(crate) show_grid: bool,
    /// Draw the ruler lines that follow the cursor.
    pub(crate) show_ruler: bool,
    /// Target directory for the one-file-per-figure export.
    pub(crate) split_dir: String,
    /// Result of the last split export.
    pub(crate) last_split: String,
    /// Whether the hot-key window is open.
    pub(crate) show_help: bool,
    /// Whether the about window is open.
    pub(crate) show_about: bool,

    /// Rectangle of the canvas of the previous frame.
    pub(crate) canvas_rect: Option<EguiRect>,
    /// `true` while the view has to be re-fitted to the image.
    pub(crate) fit_pending: bool,
    /// Pending image load requested by drag & drop or the CLI.
    pub(crate) pending_image: Option<PathBuf>,
    /// Pending JSON load.
    pub(crate) pending_json: Option<PathBuf>,
    /// Figure the context menu was opened on.
    pub(crate) context_target: Option<ShapeId>,
}

impl App {
    /// Builds the application and applies any command line arguments.
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>, cli: CliArgs) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_theme(egui::ThemePreference::Dark);
        ctx.style_mut_of(egui::Theme::Dark, |style| {
            style.spacing.item_spacing = vec2(6.0, 5.0);
            style.spacing.button_padding = vec2(7.0, 3.0);
            style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(38, 40, 46);
            style.visuals.widgets.noninteractive.weak_bg_fill = Color32::from_rgb(31, 33, 38);
            style.visuals.window_fill = Color32::from_rgb(31, 33, 38);
            style.visuals.panel_fill = Color32::from_rgb(31, 33, 38);
        });

        Self {
            session: Session::new(),
            texture: None,
            image_path: None,
            image_name: String::new(),
            image_size: Vec2::ZERO,
            open_path: String::new(),
            json_path: json_io::default_json_path(None),
            precision: 2,
            show_labels: true,
            show_grid: true,
            show_ruler: true,
            split_dir: String::new(),
            last_split: String::new(),
            show_help: false,
            show_about: false,
            canvas_rect: None,
            fit_pending: true,
            pending_image: cli.image,
            pending_json: cli.annotations,
            context_target: None,
        }
    }

    /// Shows a transient message in the status bar.
    pub(crate) fn notify(&mut self, text: impl Into<String>) {
        self.session.notify(text);
    }

    /// Shows a transient error message in the status bar.
    pub(crate) fn notify_error(&mut self, text: impl Into<String>) {
        self.session.notify_error(text);
    }

    // ------------------------------------------------------------------
    // Loading
    // ------------------------------------------------------------------

    /// Loads an image, replacing the current one and clearing the figures.
    pub(crate) fn load_image(&mut self, ctx: &Context, path: &Path) {
        match load_texture(ctx, path) {
            Ok((texture, size)) => {
                self.image_path = Some(path.to_path_buf());
                self.image_name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                self.image_size = size;
                self.texture = Some(texture);
                self.json_path = json_io::default_json_path(Some(path));
                self.fit_pending = true;
                self.session
                    .load(picture_measure::model::AnnotationSet::new());
                self.notify(format!("Загружено: {}", self.image_name));
            }
            Err(error) => {
                self.notify_error(format!("Не удалось открыть {}: {error}", path.display()));
            }
        }
    }

    /// Loads annotations from a JSON file.
    pub(crate) fn load_annotations(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(text) => match json_io::from_str(&text) {
                Ok(set) => {
                    let counts = set.counts();
                    self.session.load(set);
                    if let Some(info) = json_io::peek_image_info(&text)
                        && self.texture.is_none()
                        && !info.path.is_empty()
                        && Path::new(&info.path).is_file()
                    {
                        self.pending_image = Some(PathBuf::from(&info.path));
                    }
                    self.notify(format!(
                        "Загружено {} фигур (точек: {}, прямоугольников: {})",
                        counts.total, counts.points, counts.rects
                    ));
                }
                Err(error) => self.notify_error(format!("Ошибка чтения JSON: {error}")),
            },
            Err(error) => {
                self.notify_error(format!("Не удалось прочитать {}: {error}", path.display()));
            }
        }
    }

    // ------------------------------------------------------------------
    // Saving
    // ------------------------------------------------------------------

    /// Serialises the current annotations.
    pub fn to_json(&self, pretty: bool) -> Result<String, json_io::JsonError> {
        let info = self.image_info();
        if pretty {
            json_io::to_string_pretty(self.session.set(), info, self.precision)
        } else {
            json_io::to_string_compact(self.session.set(), info, self.precision)
        }
    }

    fn image_info(&self) -> Option<ImageInfo> {
        let path = self.image_path.as_deref()?;
        Some(ImageInfo::from_path(
            path,
            self.image_size.x.round() as u32,
            self.image_size.y.round() as u32,
        ))
    }

    /// Writes the JSON file and clears the dirty flag.
    pub(crate) fn save_json(&mut self) {
        let path = self.json_path.trim().to_owned();
        if path.is_empty() {
            self.notify_error("Укажите путь для сохранения JSON");
            return;
        }
        match self.to_json(true) {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => {
                    self.session.mark_saved();
                    self.notify(format!("Сохранено: {path}"));
                }
                Err(error) => {
                    self.notify_error(format!("Не удалось сохранить {path}: {error}"));
                }
            },
            Err(error) => self.notify_error(format!("Не удалось сериализовать: {error}")),
        }
    }

    /// Copies the JSON to the system clipboard.
    pub(crate) fn copy_json(&mut self, ctx: &Context, pretty: bool) {
        match self.to_json(pretty) {
            Ok(text) => {
                ctx.copy_text(text.clone());
                self.notify(format!("JSON скопирован в буфер ({} символов)", text.len()));
            }
            Err(error) => self.notify_error(format!("Не удалось сериализовать: {error}")),
        }
    }

    /// Writes one JSON file per figure into `dir`, grouped by folder.
    pub(crate) fn save_split(&mut self, dir: &str) {
        let base = dir.trim();
        if base.is_empty() {
            self.notify_error("Укажите папку для раздельного экспорта");
            return;
        }
        let root = PathBuf::from(base);
        if let Err(error) = std::fs::create_dir_all(&root) {
            self.notify_error(format!("Не удалось создать {base}: {error}"));
            return;
        }
        match picture_measure::split::write_split_json(&root, self.session.set(), self.precision) {
            Ok(count) => {
                let text = format!("{count} файлов в подпапках внутри {base}");
                self.last_split.clone_from(&text);
                self.notify(text);
            }
            Err(error) => self.notify_error(format!("Ошибка экспорта: {error}")),
        }
    }

    // ------------------------------------------------------------------
    // View
    // ------------------------------------------------------------------

    /// Fits the image into the canvas, once the canvas has a real size.
    pub(crate) fn fit_view(&mut self) {
        let Some(rect) = self
            .canvas_rect
            .filter(|r| r.width() > 1.0 && r.height() > 1.0)
        else {
            return;
        };
        self.session.fit_view(rect, self.image_size);
        self.fit_pending = false;
    }

    /// Reset the zoom to 1:1.
    pub(crate) fn reset_zoom(&mut self) {
        if let Some(rect) = self.canvas_rect {
            self.session.reset_zoom(rect);
        }
        self.fit_pending = false;
    }

    /// `true` once an image has been loaded.
    #[must_use]
    pub fn has_image(&self) -> bool {
        self.texture.is_some()
    }

    /// `true` when the pointer sits inside `canvas`.
    pub(crate) fn pointer_over_canvas(&self, canvas: EguiRect) -> bool {
        self.session
            .pointer()
            .position()
            .is_some_and(|p| canvas.contains(p))
    }
}

/// Decodes an image file and uploads it as a texture.
fn load_texture(ctx: &Context, path: &Path) -> Result<(TextureHandle, Vec2), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let image = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    if image.width() < 1 || image.height() < 1 {
        return Err("изображение имеет нулевой размер".to_owned());
    }
    let size = vec2(image.width() as f32, image.height() as f32);
    let rgba = image.to_rgba8();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );
    let texture = ctx.load_texture(
        path.to_string_lossy().into_owned(),
        color_image,
        TextureOptions::LINEAR,
    );
    Ok((texture, size))
}

/// Colour of the selection outline and of section headers.
pub(crate) const ACCENT: Color32 = picture_measure::model::SELECTION_COLOR;

/// `StrokeKind::Outside`, used for every figure outline.
pub(crate) const OUTSIDE: egui::StrokeKind = egui::StrokeKind::Outside;

/// Draws a colour swatch button.
pub(crate) fn swatch(ui: &mut Ui, color: Color32, selected: bool) -> egui::Response {
    let size = vec2(18.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, color);
    painter.rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, if selected { ACCENT } else { Color32::GRAY }),
        egui::StrokeKind::Outside,
    );
    response.on_hover_text(json_io::format_color(color))
}

/// Monospace font used for coordinates and measurements.
pub(crate) fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Bottom status bar hint.
pub(crate) fn status_hint() -> &'static str {
    "ЛКМ — точка/прямоугольник · Shift — квадрат · СКМ/ПКМ — панорама · колесо — зум"
}

/// Stroke width used for figure outlines.
pub(crate) fn outline_width(scale: f32) -> f32 {
    (2.0 * scale).clamp(1.0, 2.5)
}

/// `true` when <kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + `key` was pressed outside of a
/// text field.
pub(crate) fn shortcut(ctx: &Context, key: Key) -> bool {
    if ctx.egui_wants_keyboard_input() {
        return false;
    }
    ctx.input(|i| i.key_pressed(key) && (i.modifiers.ctrl || i.modifiers.command))
}

/// `true` when a bare key (no Ctrl/Cmd) was pressed outside of a text field.
pub(crate) fn bare_key(ctx: &Context, key: Key) -> bool {
    if ctx.egui_wants_keyboard_input() {
        return false;
    }
    ctx.input(|i| i.key_pressed(key) && !i.modifiers.ctrl && !i.modifiers.command)
}

/// Text that was pasted this frame, if any.
pub(crate) fn pasted_text(ctx: &Context) -> Option<String> {
    ctx.input(|i| {
        i.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        crate::gui::panels::frame(self, ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(24, 25, 28).to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app with no window attached, for unit tests.
    fn bare_app() -> App {
        App {
            session: Session::new(),
            texture: None,
            image_path: None,
            image_name: String::new(),
            image_size: Vec2::ZERO,
            open_path: String::new(),
            json_path: String::new(),
            precision: 2,
            show_labels: true,
            show_grid: true,
            show_ruler: true,
            split_dir: String::new(),
            last_split: String::new(),
            show_help: false,
            show_about: false,
            canvas_rect: None,
            fit_pending: false,
            pending_image: None,
            pending_json: None,
            context_target: None,
        }
    }

    #[test]
    fn outlines_stay_visible_at_every_zoom() {
        assert!(
            (outline_width(0.01) - 1.0).abs() < 1e-6,
            "тонкий контур на мелком масштабе"
        );
        assert!((outline_width(1.0) - 2.0).abs() < 1e-6, "обычный масштаб");
        assert!(
            (outline_width(10.0) - 2.5).abs() < 1e-6,
            "толстый, но не слишком"
        );
        assert!(outline_width(1000.0) <= 2.5);
    }

    #[test]
    fn status_hint_is_useful() {
        assert!(status_hint().contains("точка"));
        assert!(status_hint().contains("Shift"));
    }

    #[test]
    fn kind_colors_differ() {
        assert_ne!(
            picture_measure::model::POINT_COLOR,
            picture_measure::model::RECT_COLOR
        );
        assert_eq!(ACCENT, picture_measure::model::SELECTION_COLOR);
    }

    #[test]
    fn loading_a_missing_file_fails_cleanly() {
        let ctx = Context::default();
        let Err(error) = load_texture(&ctx, Path::new("/definitely/not/here.png")) else {
            panic!("a missing file must fail");
        };
        assert!(!error.is_empty());
    }

    #[test]
    fn json_export_mentions_the_image() {
        let app = bare_app();
        assert!(!app.has_image());
        assert_eq!(app.session.view().scale, 1.0);

        let json = app.to_json(true).unwrap();
        assert!(json.contains("\"annotations\": []"), "{json}");
        assert!(!json.contains("\"image\""), "без картинки поля image нет");
    }

    #[test]
    fn json_export_with_figures() {
        let mut app = bare_app();
        app.session.add_point(egui::pos2(12.0, 34.0));
        let json = app.to_json(true).unwrap();
        assert!(json.contains("\"name\": \"point_1\""), "{json}");
        assert!(json.contains("\"x\": 12"), "{json}");
        let compact = app.to_json(false).unwrap();
        assert!(!compact.contains('\n'), "компактный вариант в одну строку");
    }

    #[test]
    fn describe_covers_both_kinds() {
        let mut app = bare_app();
        let point = app.session.add_point(egui::pos2(1.5, 2.5));
        let rect = app
            .session
            .add_rect(picture_measure::geometry::Rect::from_xywh(
                0.0, 0.0, 10.0, 20.0,
            ));
        let p = Session::describe(app.session.set().get(point).unwrap());
        assert!(p.contains("точка"), "{p}");
        assert!(p.contains("1.5"), "{p}");
        let r = Session::describe(app.session.set().get(rect).unwrap());
        assert!(r.contains("прямоугольник"), "{r}");
        assert!(r.contains("10.0×20.0"), "{r}");
    }

    #[test]
    fn saving_without_a_path_is_an_error_not_a_panic() {
        let mut app = bare_app();
        app.save_json();
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn saving_writes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.json");
        let mut app = bare_app();
        app.json_path = path.to_string_lossy().into_owned();
        app.session.add_point(egui::pos2(1.0, 2.0));
        app.save_json();

        let message = app.session.message().unwrap();
        assert!(!message.is_error, "{message:?}");
        assert!(
            !app.session.is_dirty(),
            "после сохранения файл считается актуальным"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(json_io::from_str(&text).unwrap().len(), 1);
    }

    #[test]
    fn split_export_creates_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("split");
        let mut app = bare_app();
        let id = app.session.add_point(egui::pos2(1.0, 2.0));
        app.session.set_folder(id, "eyes");
        app.split_dir = out.to_string_lossy().into_owned();
        app.save_split(&app.split_dir.clone());

        let message = app.session.message().unwrap();
        assert!(!message.is_error, "{message:?}");
        assert!(out.join("eyes").join("point_1.json").is_file());
        assert!(app.last_split.contains("1 файлов"), "{}", app.last_split);
    }

    #[test]
    fn split_export_without_a_directory_is_an_error() {
        let mut app = bare_app();
        app.save_split("   ");
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn loading_json_replaces_the_figures() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("in.json");
        std::fs::write(
            &path,
            r#"{"annotations":[{"type":"rect","name":"r","x":1,"y":2,"width":3,"height":4}]}"#,
        )
        .unwrap();
        let mut app = bare_app();
        app.session.add_point(egui::pos2(9.0, 9.0));
        app.load_annotations(&path);

        let message = app.session.message().unwrap();
        assert!(!message.is_error, "{message:?}");
        assert_eq!(app.session.set().len(), 1);
        assert_eq!(app.session.set().items()[0].display_name(), "r");
    }

    #[test]
    fn loading_broken_json_reports_the_problem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{ not json").unwrap();
        let mut app = bare_app();
        app.load_annotations(&path);
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn loading_a_missing_json_reports_the_problem() {
        let mut app = bare_app();
        app.load_annotations(Path::new("/no/such/file.json"));
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn clipboard_copy_reports_the_size() {
        let ctx = Context::default();
        let mut app = bare_app();
        app.session.add_point(egui::pos2(1.0, 2.0));
        app.copy_json(&ctx, true);
        let message = app.session.message().unwrap();
        assert!(!message.is_error);
        assert!(message.text.contains("буфер"), "{}", message.text);
    }

    #[test]
    fn messages_can_be_set() {
        let mut app = bare_app();
        app.notify("ок");
        assert!(!app.session.message().unwrap().is_error);
        app.notify_error("плохо");
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn view_helpers_are_safe_without_a_canvas() {
        let mut app = bare_app();
        app.fit_view();
        app.reset_zoom();
        assert_eq!(app.session.view().scale, 1.0);
        app.canvas_rect = Some(EguiRect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(800.0, 600.0),
        ));
        app.image_size = vec2(100.0, 100.0);
        app.fit_view();
        assert!(app.session.view().scale > 1.0);
    }
}
