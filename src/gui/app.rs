//! The `eframe::App` implementation.
//!
//! All the state lives in [`picture_measure::session::Session`]; this file only
//! owns what is strictly about the window: the texture, the file paths and the
//! little bits of widget state.

use std::path::{Path, PathBuf};

#[cfg(test)]
use super::dialogs::{Canned, Opener};
use super::dialogs::{Dialogs, Request, directory_of};
use egui::{
    Color32, Context, FontId, Key, Rect as EguiRect, TextureHandle, TextureOptions, Ui, Vec2, vec2,
};
use picture_measure::i18n::{self, Lang, Text, fill};
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
    /// Interface language; detected from the environment when absent.
    pub lang: Option<Lang>,
}

/// The whole application.
pub struct App {
    /// The egui context, kept so the title can be updated when the language
    /// changes.
    pub(crate) ctx: Context,
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
    /// Interface language.
    pub(crate) lang: Lang,

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
    /// Native file pickers, running on their own thread.
    pub(crate) dialogs: Dialogs,
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
            ctx: ctx.clone(),
            session: Session::new(),
            texture: None,
            image_path: None,
            image_name: String::new(),
            image_size: Vec2::ZERO,
            open_path: String::new(),
            // Путь заполняется сам при открытии картинки. Пока картинки
            // нет, оставляем поле пустым: иначе «Сохранить JSON» решит, что
            // путь уже выбран, и запишет файл туда, куда никто не просил.
            json_path: String::new(),
            precision: 2,
            show_labels: true,
            show_grid: true,
            show_ruler: true,
            split_dir: String::new(),
            last_split: String::new(),
            show_help: false,
            show_about: false,
            lang: cli.lang.unwrap_or_else(i18n::from_environment),
            canvas_rect: None,
            fit_pending: true,
            pending_image: cli.image,
            pending_json: cli.annotations,
            context_target: None,
            dialogs: Dialogs::native(cli.lang.unwrap_or_else(i18n::from_environment)),
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

    /// Every string of the current language.
    pub(crate) fn t(&self) -> &'static Text {
        Text::get(self.lang)
    }

    /// Switches the interface language and refreshes the window title.
    pub(crate) fn set_lang(&mut self, lang: Lang) {
        if self.lang != lang {
            self.lang = lang;
            self.dialogs.set_language(lang);
            let title = self.t().window_title.to_owned();
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    /// Shows a localised informational message.
    ///
    /// `args` fills the `{placeholders}` of the template.
    pub(crate) fn say(&mut self, template: &str, args: &[(&str, &str)]) {
        self.session.notify(fill(template, args));
    }

    /// Shows a localised error message.
    pub(crate) fn complain(&mut self, template: &str, args: &[(&str, &str)]) {
        self.session.notify_error(fill(template, args));
    }

    /// Runs one frame of the whole interface without a window.
    ///
    /// Used by the tests: egui builds the real widget tree and produces real
    /// text shapes, so a test can assert not only "no panic" but also "the
    /// English labels are really on screen".
    #[cfg(test)]
    pub(crate) fn render_headless(&mut self) -> Vec<String> {
        // `ctx` клонируем заранее: замыкание уже занимает `&mut self`.
        let ctx = self.ctx.clone();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::gui::panels::frame(self, ui);
        });

        // Настоящий бэкенд загружает текстуры шрифтов; в тесте «бэкенда» нет,
        // поэтому сливаем дельты в менеджер текстур вручную, иначе egui
        // упадёт на Drop с «unapplied deltas».
        let mut delta = output.textures_delta;
        delta.clear();
        let manager = ctx.tex_manager();
        let mut textures = manager.write();
        let set = std::mem::take(&mut delta.set);
        for (id, images) in set {
            for image in images {
                textures.set(id, image);
            }
        }

        output
            .shapes
            .into_iter()
            .filter_map(|clipped| match clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect()
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
                // Поле «Путь» — это ввод «что открыть», а не показ текущего
                // файла. Если оставить в нём путь, следующее нажатие «Открыть»
                // просто перезагрузит ту же картинку и до диалога не дойдёт.
                // Открытый файл и так показан в блоке «Изображение».
                self.open_path.clear();
                self.json_path = json_io::default_json_path(Some(path));
                self.fit_pending = true;
                self.session
                    .load(picture_measure::model::AnnotationSet::new());
                let template = self.t().loaded_image;
                let name = self.image_name.clone();
                self.say(template, &[("name", &name)]);
            }
            Err(error) => {
                let template = self.t().image_failed;
                let path = path.display().to_string();
                self.complain(template, &[("path", &path), ("error", &error)]);
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
                    let template = self.t().loaded_figures;
                    let (n, p, r) = (
                        counts.total.to_string(),
                        counts.points.to_string(),
                        counts.rects.to_string(),
                    );
                    self.say(template, &[("n", &n), ("p", &p), ("r", &r)]);
                }
                Err(error) => {
                    let template = self.t().json_failed;
                    let message = error.message(self.lang);
                    self.complain(template, &[("error", &message)]);
                }
            },
            Err(error) => {
                let template = self.t().read_failed;
                let path = path.display().to_string();
                self.complain(template, &[("path", &path), ("error", &error.to_string())]);
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
            let message = self.t().need_json_path.to_owned();
            self.notify_error(message);
            return;
        }
        match self.to_json(true) {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => {
                    self.session.mark_saved();
                    let template = self.t().saved;
                    self.say(template, &[("path", &path)]);
                }
                Err(error) => {
                    let template = self.t().save_failed;
                    let message = error.to_string();
                    self.complain(template, &[("path", &path), ("error", &message)]);
                }
            },
            Err(error) => {
                let template = self.t().serialise_failed;
                let message = error.message(self.lang);
                self.complain(template, &[("error", &message)]);
            }
        }
    }

    /// Copies the JSON to the system clipboard.
    pub(crate) fn copy_json(&mut self, ctx: &Context, pretty: bool) {
        match self.to_json(pretty) {
            Ok(text) => {
                let length = text.len().to_string();
                ctx.copy_text(text);
                let template = self.t().copied;
                self.say(template, &[("n", &length)]);
            }
            Err(error) => {
                let template = self.t().serialise_failed;
                let message = error.message(self.lang);
                self.complain(template, &[("error", &message)]);
            }
        }
    }

    /// Writes one JSON file per figure into `dir`, grouped by folder.
    pub(crate) fn save_split(&mut self, dir: &str) {
        let base = dir.trim();
        if base.is_empty() {
            let message = self.t().need_split_dir.to_owned();
            self.notify_error(message);
            return;
        }
        let root = PathBuf::from(base);
        if let Err(error) = std::fs::create_dir_all(&root) {
            let template = self.t().split_create_failed;
            let message = error.to_string();
            self.complain(template, &[("dir", base), ("error", &message)]);
            return;
        }
        match picture_measure::split::write_split_json(&root, self.session.set(), self.precision) {
            Ok(count) => {
                let template = self.t().split_result_done;
                let text = fill(template, &[("n", &count.to_string()), ("dir", base)]);
                self.last_split.clone_from(&text);
                self.notify(text);
            }
            Err(error) => {
                let template = self.t().split_failed;
                let message = error.message(self.lang);
                self.complain(template, &[("error", &message)]);
            }
        }
    }

    // ------------------------------------------------------------------
    // Native dialogs
    // ------------------------------------------------------------------

    /// Открывает картинку: по введённому пути или через системный диалог.
    ///
    /// Пустое поле — это не ошибка, а «я не знаю, что открыть», поэтому
    /// вместо сообщения об ошибке показывается окно выбора файлов.
    pub(crate) fn open_image(&mut self) {
        match Self::trimmed_path(&self.open_path) {
            Some(path) => self.pending_image = Some(path),
            None => self.ask_image(),
        }
    }

    /// Сохраняет разметку: по указанному пути или через системный диалог.
    ///
    /// Путь подставляется сам при открытии картинки, поэтому чаще всего
    /// кнопка просто пишет файл; окно появляется, только когда пути ещё нет.
    pub(crate) fn save_json_or_ask(&mut self) {
        if self.json_path.trim().is_empty() {
            self.ask_save();
        } else {
            self.save_json();
        }
    }

    /// Просит системный диалог выбора картинки.
    ///
    /// Открывается рядом с уже загруженной картинкой, а если её нет — рядом
    /// с тем, что пользователь уже напечатал в поле.
    pub(crate) fn ask_image(&mut self) {
        let start = self
            .image_path
            .clone()
            .or_else(|| Self::trimmed_path(&self.open_path));
        self.ask(Request::Image {
            directory: directory_of(start.as_deref()),
        });
    }

    /// Просит системный диалог выбора файла разметки.
    pub(crate) fn ask_annotations(&mut self) {
        let start = self
            .image_path
            .clone()
            .or_else(|| Self::trimmed_path(&self.json_path));
        self.ask(Request::Annotations {
            directory: directory_of(start.as_deref()),
        });
    }

    /// Просит системный диалог сохранения разметки.
    ///
    /// Уже выбранный путь предлагается как имя файла, поэтому повторное
    /// сохранение не заставляет подтверждать то же имя заново.
    pub(crate) fn ask_save(&mut self) {
        self.ask(Request::SaveAnnotations {
            suggested: Self::trimmed_path(&self.json_path),
        });
    }

    /// Просит системный диалог выбора папки для раздельного экспорта.
    pub(crate) fn ask_directory(&mut self) {
        let start = Self::trimmed_path(&self.split_dir).or_else(|| self.image_path.clone());
        self.ask(Request::Directory {
            directory: directory_of(start.as_deref()),
        });
    }

    /// Отправляет запрос в поток диалогов.
    ///
    /// Пока один диалог открыт, второй игнорируется: два системных окна
    /// поверх друг друга пользователю не нужны.
    fn ask(&mut self, request: Request) {
        if self.dialogs.request(request) {
            return;
        }
        if self.dialogs.was_rejected() {
            let template = self.t().dialog_busy;
            self.complain(template, &[]);
            self.dialogs.clear_rejected();
        }
    }

    /// Забирает ответ диалога, если он готов, и применяет его.
    ///
    /// Вызывается каждый кадр: `try_recv` не блокирует окно.
    pub(crate) fn poll_dialogs(&mut self, ctx: &Context) {
        if self.dialogs.is_open() {
            // Пока открыт системный диалог, окно нужно будить: события из
            // чужого окна сюда не приходят, поэтому egui по умолчанию будет
            // перерисовывать интерфейс только по движению мыши. Ответ диалога
            // лежал бы в канале до случайного тычка — выглядело бы как
            // «диалог не работает».
            ctx.request_repaint();
        }
        let Some((request, result)) = self.dialogs.take_result() else {
            return;
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                // Диалог показать не удалось — путь по-прежнему можно ввести
                // руками, поэтому сообщаем и продолжаем работать.
                let template = self.t().dialog_failed;
                self.complain(template, &[("error", &error)]);
                return;
            }
        };
        // Отмена — обычное дело, а не ошибка: молча ничего не делаем.
        if let Some(path) = outcome.path() {
            self.apply_choice(&request, &path, ctx);
        }
    }

    /// Применяет выбранный путь.
    ///
    /// Что именно с ним сделать, решает вид диалога, а не расширение файла:
    /// окно сохранения разрешает любой путь, и угадывать по нему — лотерея.
    fn apply_choice(&mut self, request: &Request, path: &Path, ctx: &Context) {
        let text = path.to_string_lossy().into_owned();
        match request {
            Request::Image { .. } => {
                // Путь подставит `load_image`, если загрузка удалась.
                self.load_image(ctx, path);
            }
            Request::Annotations { .. } => {
                self.json_path = text;
                self.load_annotations(path);
            }
            Request::SaveAnnotations { .. } => {
                self.json_path = text;
                self.save_json();
            }
            Request::Directory { .. } => {
                self.split_dir = text;
            }
        }
    }

    /// Поле пути без окружающих пробелов, если оно не пустое.
    fn trimmed_path(raw: &str) -> Option<PathBuf> {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
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
        return Err(format!(
            "изображение имеет нулевой размер ({}×{})",
            image.width(),
            image.height()
        ));
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

/// An [`App`] with no window attached, for tests.
#[cfg(test)]
pub(crate) fn test_app() -> App {
    test_app_with(Box::new(Canned::Cancelled))
}

/// An [`App`] whose file dialogs answer `opener`.
#[cfg(test)]
pub(crate) fn test_app_with(opener: Box<dyn Opener>) -> App {
    App {
        ctx: Context::default(),
        session: Session::new(),
        dialogs: Dialogs::with_opener(opener, Lang::Ru),
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
        lang: Lang::Ru,
        canvas_rect: None,
        fit_pending: false,
        pending_image: None,
        pending_json: None,
        context_target: None,
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        crate::gui::panels::frame(self, ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {}

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(24, 25, 28).to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn both_locales_have_a_useful_status_hint() {
        for lang in [Lang::Ru, Lang::En] {
            let hint = Text::get(lang).status_hint;
            assert!(hint.contains("Shift"), "{lang}: в подсказке нет Shift");
            assert!(hint.len() > 20, "{lang}: подсказка слишком короткая");
        }
        assert_ne!(
            Text::get(Lang::Ru).status_hint,
            Text::get(Lang::En).status_hint
        );
    }

    #[test]
    fn switching_language_changes_the_wording() {
        let mut app = test_app();
        assert_eq!(app.lang, Lang::Ru);
        assert_ne!(app.t().save_json, Text::get(Lang::En).save_json);
        app.set_lang(Lang::En);
        assert_eq!(app.lang, Lang::En);
        assert_eq!(app.t().save_json, Text::get(Lang::En).save_json);
        app.set_lang(Lang::En); // повторное переключение ничего не ломает
        assert_eq!(app.lang, Lang::En);
    }

    #[test]
    fn browse_buttons_reach_the_screen() {
        // Кнопки выбора пути обязаны быть видны: если их нет в разметке,
        // пользователь нажимает в пустое место и concludes, что диалогов нет.
        for lang in Lang::ALL {
            let mut app = test_app();
            app.set_lang(lang);
            let text = app.render_headless();
            let join = text.join("\n");
            assert!(
                join.contains(app.t().browse),
                "{lang}: нет кнопки «{}» в разметке",
                app.t().browse
            );
            assert!(
                join.contains(app.t().load_json),
                "{lang}: нет кнопки «{}» в разметке",
                app.t().load_json
            );
        }
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
        let path = Path::new("/definitely/not/here.png");
        let Err(error) = load_texture(&ctx, path) else {
            panic!("отсутствующий файл должен приводить к ошибке");
        };
        // Текст ошибки задаёт операционная система, поэтому проверяем только то,
        // что он во что-то осмысленное, а не заглушку.
        assert!(
            error.chars().count() > 10,
            "неинформативное сообщение об ошибке: {error}"
        );
        assert!(
            !error.contains("unwrap") && !error.contains("panic"),
            "внутренние детали не должны попадать в сообщение: {error}"
        );
    }

    #[test]
    fn json_export_mentions_the_image() {
        let app = test_app();
        assert!(!app.has_image());
        assert_eq!(app.session.view().scale, 1.0);

        let json = app.to_json(true).unwrap();
        assert!(json.contains("\"annotations\": []"), "{json}");
        assert!(!json.contains("\"image\""), "без картинки поля image нет");
    }

    #[test]
    fn json_export_with_figures() {
        let mut app = test_app();
        app.session.add_point(egui::pos2(12.0, 34.0));
        let json = app.to_json(true).unwrap();
        assert!(json.contains("\"name\": \"point_1\""), "{json}");
        assert!(json.contains("\"x\": 12"), "{json}");
        let compact = app.to_json(false).unwrap();
        assert!(!compact.contains('\n'), "компактный вариант в одну строку");
    }

    #[test]
    fn describe_covers_both_kinds() {
        let mut app = test_app();
        let point = app.session.add_point(egui::pos2(1.5, 2.5));
        let rect = app
            .session
            .add_rect(picture_measure::geometry::Rect::from_xywh(
                0.0, 0.0, 10.0, 20.0,
            ));
        let p = Session::describe(app.session.set().get(point).unwrap(), Lang::Ru);
        assert!(p.contains("точка"), "{p}");
        assert!(p.contains("1.5"), "{p}");
        let r = Session::describe(app.session.set().get(rect).unwrap(), Lang::En);
        assert!(r.contains("rectangle"), "{r}");
        assert!(r.contains("10.0×20.0"), "{r}");
    }

    #[test]
    fn saving_without_a_path_is_an_error_not_a_panic() {
        let mut app = test_app();
        app.save_json();
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn saving_writes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.json");
        let mut app = test_app();
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
        let mut app = test_app();
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
        let mut app = test_app();
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
        let mut app = test_app();
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
        let mut app = test_app();
        app.load_annotations(&path);
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn loading_a_missing_json_reports_the_problem() {
        let mut app = test_app();
        app.load_annotations(Path::new("/no/such/file.json"));
        assert!(app.session.message().unwrap().is_error);
    }

    #[test]
    fn clipboard_copy_reports_the_size() {
        let ctx = Context::default();
        let mut app = test_app();
        app.session.add_point(egui::pos2(1.0, 2.0));
        app.copy_json(&ctx, true);
        let message = app.session.message().unwrap();
        assert!(!message.is_error);
        assert!(message.text.contains("буфер"), "{}", message.text);
    }

    #[test]
    fn messages_can_be_set() {
        let mut app = test_app();
        app.notify("ок");
        assert!(!app.session.message().unwrap().is_error);
        app.notify_error("плохо");
        assert!(app.session.message().unwrap().is_error);
    }

    // ------------------------------------------------------------------
    // Native dialogs
    // ------------------------------------------------------------------

    /// Крутит кадры, пока диалог не ответит, и применяет ответ.
    fn answer_dialog(app: &mut App) {
        let ctx = app.ctx.clone();
        for _ in 0..400 {
            app.poll_dialogs(&ctx);
            if !app.dialogs.is_open() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("диалог так и не ответил");
    }

    /// Пишет настоящий PNG, чтобы диалог выбора картинки было что открыть.
    fn sample_png(path: &std::path::Path) {
        let image = image::RgbImage::from_pixel(4, 3, image::Rgb([10, 20, 30]));
        image.save(path).unwrap();
    }

    #[test]
    fn picking_an_image_loads_it() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("cat.png");
        sample_png(&image_path);
        let mut app = test_app_with(Box::new(Canned::Chosen(image_path.clone())));

        app.ask_image();
        assert!(app.dialogs.is_open(), "диалог должен открыться");
        answer_dialog(&mut app);

        assert!(app.has_image(), "картинка должна быть загружена");
        assert_eq!(app.image_path.as_deref(), Some(image_path.as_path()));
        assert!(
            app.open_path.is_empty(),
            "после загрузки поле ввода должно быть пустым"
        );
        assert!(
            app.session.message().is_some_and(|m| !m.is_error),
            "загрузка через диалог не должна считаться ошибкой"
        );
    }

    #[test]
    fn picking_a_save_path_writes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("chosen.json");
        let mut app = test_app_with(Box::new(Canned::Chosen(out.clone())));
        app.session.add_point(egui::pos2(3.0, 4.0));

        app.ask_save();
        answer_dialog(&mut app);

        assert_eq!(app.json_path, out.to_string_lossy());
        let text = std::fs::read_to_string(&out).expect("файл должен быть записан");
        assert_eq!(json_io::from_str(&text).unwrap().len(), 1);
        assert!(!app.session.is_dirty());
    }

    #[test]
    fn the_save_dialog_only_fills_the_field_when_asked_to() {
        // Диалог аннотаций читает файл, а не пишет его: путать эти два действия
        // нельзя, иначе «открыть» уничтожит несохранённую разметку.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("existing.json");
        std::fs::write(&file, r#"{"version":1,"image":"x.png","figures":[]}"#).unwrap();
        let mut app = test_app_with(Box::new(Canned::Chosen(file.clone())));
        app.json_path = dir
            .path()
            .join("untouched.json")
            .to_string_lossy()
            .into_owned();

        app.ask_annotations();
        answer_dialog(&mut app);

        assert_eq!(app.json_path, file.to_string_lossy());
        assert!(!dir.path().join("untouched.json").exists());
    }

    #[test]
    fn picking_a_folder_fills_the_export_field_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("split");
        let mut app = test_app_with(Box::new(Canned::Chosen(out.clone())));
        app.session.add_point(egui::pos2(1.0, 1.0));

        app.ask_directory();
        answer_dialog(&mut app);

        assert_eq!(app.split_dir, out.to_string_lossy());
        assert!(
            !out.exists(),
            "выбор папки не должен создавать её: это делает кнопка «Записать»"
        );
    }

    #[test]
    fn cancelling_changes_nothing() {
        let mut app = test_app_with(Box::new(Canned::Cancelled));
        app.open_path = "/tmp/осталось.json".to_owned();
        app.ask_image();
        answer_dialog(&mut app);

        assert_eq!(app.open_path, "/tmp/осталось.json");
        assert!(!app.has_image());
        assert!(
            app.session.message().is_none(),
            "отмена — не ошибка, сообщать о ней незачем"
        );
    }

    #[test]
    fn a_dialog_that_cannot_open_reports_it_and_keeps_the_fields() {
        let mut app = test_app_with(Box::new(Canned::Failed("нет D-Bus".to_owned())));
        app.open_path = "/tmp/руками.json".to_owned();
        app.ask_image();
        answer_dialog(&mut app);

        let message = app
            .session
            .message()
            .expect("пользователь должен узнать почему");
        assert!(message.is_error);
        assert!(message.text.contains("D-Bus"), "{message:?}");
        assert_eq!(
            app.open_path, "/tmp/руками.json",
            "введённый вручную путь должен остаться"
        );
    }

    #[test]
    fn each_picker_suggests_a_sensible_starting_place() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app();

        app.ask_save();
        assert_eq!(
            app.dialogs.pending_request(),
            Some(&Request::SaveAnnotations { suggested: None })
        );
        answer_dialog(&mut app);

        app.json_path = dir.path().join("a.json").to_string_lossy().into_owned();
        app.ask_save();
        let Some(Request::SaveAnnotations {
            suggested: Some(path),
        }) = app.dialogs.pending_request().cloned()
        else {
            panic!("ожидался запрос на сохранение");
        };
        assert_eq!(
            Request::SaveAnnotations {
                suggested: Some(path.clone())
            }
            .start_directory(),
            Some(dir.path().to_path_buf()),
            "диалог должен открыться рядом с текущим файлом"
        );
        answer_dialog(&mut app);

        app.ask_image();
        assert!(matches!(
            app.dialogs.pending_request(),
            Some(Request::Image { .. })
        ));
        answer_dialog(&mut app);

        app.ask_annotations();
        assert!(matches!(
            app.dialogs.pending_request(),
            Some(Request::Annotations { .. })
        ));
        answer_dialog(&mut app);

        app.ask_directory();
        assert!(matches!(
            app.dialogs.pending_request(),
            Some(Request::Directory { .. })
        ));
    }

    #[test]
    fn an_empty_path_field_opens_the_dialog_instead_of_complaining() {
        // Кнопка «Открыть» без пути — это не ошибка, а просьба спросить у
        // пользователя. Раньше здесь выводилось сообщение и ничего не
        // происходило, и пользователь считал, что диалогов нет вовсе.
        let mut app = test_app();
        app.open_image();
        let request = app
            .dialogs
            .pending_request()
            .cloned()
            .expect("пустое поле должно вызывать диалог выбора картинки");
        assert!(matches!(request, Request::Image { .. }), "{request:?}");
        assert!(!app.session.message().is_some_and(|m| m.is_error));
    }

    #[test]
    fn opening_another_picture_asks_again() {
        // Регрессия: после первой загрузки поле «Путь» оставалось заполненным,
        // и «Открыть» навсегда входила в режим «перезагрузить тот же файл» —
        // портал больше не вызывался, вторая картинка не открывалась.
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("one.png");
        let second = dir.path().join("two.png");
        sample_png(&first);
        sample_png(&second);

        let mut app = test_app_with(Box::new(Canned::Chosen(first.clone())));
        app.ask_image();
        answer_dialog(&mut app);
        assert!(app.has_image());
        assert_eq!(app.image_path.as_deref(), Some(first.as_path()));
        assert!(
            app.open_path.is_empty(),
            "поле ввода должно снова опустеть, иначе портал больше не спросит"
        );

        app.open_image();
        assert!(
            matches!(app.dialogs.pending_request(), Some(Request::Image { .. })),
            "вторая картинка должна открываться через диалог"
        );
        answer_dialog(&mut app);

        // И через аргумент командной строки тоже.
        app.pending_image = Some(second.clone());
        app.render_headless();
        assert_eq!(app.image_path.as_deref(), Some(second.as_path()));
        assert!(app.open_path.is_empty());
    }

    #[test]
    fn a_typed_path_is_used_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("cat.png");
        sample_png(&image);
        let mut app = test_app();
        app.open_path = image.to_string_lossy().into_owned();

        app.open_image();

        assert!(
            app.dialogs.pending_request().is_none(),
            "введённый путь надо открыть, а не спросить заново"
        );
        // Отложенная загрузка разбирается на кадре — прокручиваем его.
        app.render_headless();
        assert!(app.has_image());
    }

    #[test]
    fn a_fresh_app_has_not_chosen_where_to_save() {
        let app = test_app();
        assert!(
            app.json_path.is_empty(),
            "до открытия картинки путь не выбран — программа должна спросить"
        );
    }

    #[test]
    fn saving_without_a_path_opens_the_save_dialog() {
        let mut app = test_app();
        app.session.add_point(egui::pos2(2.0, 2.0));
        app.json_path = "   ".to_owned(); // пробелы — тоже «пусто»

        app.save_json_or_ask();

        assert!(
            matches!(
                app.dialogs.pending_request(),
                Some(Request::SaveAnnotations { .. })
            ),
            "без пути программа должна спросить, куда сохранять"
        );
        assert!(
            !app.session.is_dirty() || app.dialogs.is_open(),
            "файл не создан, пока путь не выбран"
        );
    }

    #[test]
    fn saving_with_a_path_writes_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.json");
        let mut app = test_app();
        app.json_path = out.to_string_lossy().into_owned();
        app.session.add_point(egui::pos2(5.0, 6.0));

        app.save_json_or_ask();

        assert!(app.dialogs.pending_request().is_none());
        assert!(out.is_file(), "путь был указан — писать нужно сразу");
    }

    #[test]
    fn a_second_picker_is_refused_while_one_is_open() {
        let mut app = test_app_with(Box::new(Canned::Cancelled));
        app.ask_image();
        app.ask_save();
        let message = app
            .session
            .message()
            .expect("нужно сказать, что диалог уже открыт");
        assert!(message.is_error);
        assert!(
            matches!(app.dialogs.pending_request(), Some(Request::Image { .. })),
            "первый запрос должен выжить"
        );
    }

    #[test]
    fn view_helpers_are_safe_without_a_canvas() {
        let mut app = test_app();
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
