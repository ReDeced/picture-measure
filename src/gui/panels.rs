//! The surrounding chrome: toolbar, side panel, status bar and the frame loop.

#[cfg(test)]
use super::app::test_app;
use super::app::{ACCENT, App, bare_key, mono, pasted_text, shortcut, swatch};
use super::canvas;
use egui::{Align, Align2, Color32, Context, Key, Layout, RichText, TextEdit, Ui, vec2};
use picture_measure::i18n::{Lang, fill};
use picture_measure::json_io::{self, palette};
use picture_measure::model::{Annotation, Geometry, ShapeId, ShapeKind};
use picture_measure::session::Session;
use picture_measure::tools::Tool;
use std::path::PathBuf;

/// One frame of the whole application.
pub(crate) fn frame(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    handle_shortcuts(app, &ctx);
    handle_dropped_files(app, &ctx);
    poll_dialogs(app, &ctx);
    handle_pending_loads(app, &ctx);
    if app.fit_pending {
        app.fit_view();
    }

    top_bar(app, ui);
    side_panel(app, ui);
    status_bar(app, ui);
    central(app, ui);
}

/// Applies the answer of the system file dialog, if the user is done choosing.
///
/// Polling is non-blocking: until the user picks something the window keeps
/// drawing as usual.
fn poll_dialogs(app: &mut App, ctx: &Context) {
    app.poll_dialogs(ctx);
}

/// Handles the pending image/JSON loads requested by the CLI or drag & drop.
fn handle_pending_loads(app: &mut App, ctx: &Context) {
    if let Some(path) = app.pending_image.take() {
        app.load_image(ctx, &path);
    }
    if let Some(path) = app.pending_json.take() {
        app.load_annotations(&path);
    }
}

/// Keyboard shortcuts.
fn handle_shortcuts(app: &mut App, ctx: &Context) {
    let redo = shortcut(ctx, Key::Z) && ctx.input(|i| i.modifiers.shift);
    if shortcut(ctx, Key::S) {
        app.save_json_or_ask();
    }
    if shortcut(ctx, Key::C) {
        app.copy_json(ctx, true);
    }
    if shortcut(ctx, Key::A) {
        app.session.select_all();
    }
    if redo || shortcut(ctx, Key::Y) {
        app.session.redo();
    } else if shortcut(ctx, Key::Z) {
        app.session.undo();
    }
    if bare_key(ctx, Key::Escape) {
        app.session.clear_selection();
        app.session.cancel_gesture();
    }
    if bare_key(ctx, Key::Delete) || bare_key(ctx, Key::Backspace) {
        app.session.delete_selected();
    }
    if bare_key(ctx, Key::F) {
        app.fit_view();
    }
    for tool in Tool::ALL {
        if bare_key(ctx, tool.hotkey()) {
            app.session.set_tool(tool);
        }
    }
}

/// Loads files dropped onto the window.
fn handle_dropped_files(app: &mut App, ctx: &Context) {
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().into())
            .collect()
    });
    for path in dropped {
        let is_json = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"));
        if is_json {
            app.pending_json = Some(path);
        } else if json_io::is_image_path(&path) {
            app.pending_image = Some(path);
        } else {
            let template = app.t().unknown_format;
            let path = path.display().to_string();
            app.complain(template, &[("path", &path)]);
        }
    }
}

// ----------------------------------------------------------------------
// Toolbar
// ----------------------------------------------------------------------

fn top_bar(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    egui::Panel::top("toolbar").exact_size(74.0).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.button(t.open).clicked() {
                    app.open_image();
                }
                if ui.button(t.clear_all).clicked() {
                    app.session.clear_all();
                }
                ui.separator();
                if ui.button(t.save_json).clicked() {
                    app.save_json_or_ask();
                }
                if ui.button(t.copy_json).clicked() {
                    app.copy_json(ui.ctx(), true);
                }
                ui.separator();
                ui.add_enabled_ui(app.session.can_undo(), |ui| {
                    if ui.button(t.undo).clicked() {
                        app.session.undo();
                    }
                });
                ui.add_enabled_ui(app.session.can_redo(), |ui| {
                    if ui.button(t.redo).clicked() {
                        app.session.redo();
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(format!("v{}", picture_measure::version()));
                    if app.session.is_dirty() {
                        ui.label(RichText::new(t.unsaved).color(Color32::from_rgb(255, 170, 90)));
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(t.tool_label);
                let current = app.session.tool();
                for tool in Tool::ALL {
                    let button = egui::Button::new(tool.label(app.lang))
                        .selected(current == tool)
                        .shortcut_text(hotkey_label(tool.hotkey()));
                    if ui
                        .add(button)
                        .on_hover_text(tool.description(app.lang))
                        .clicked()
                    {
                        app.session.set_tool(tool);
                    }
                }
                ui.separator();
                if ui
                    .button(t.fit)
                    .on_hover_text(hotkey_label(Key::F))
                    .clicked()
                {
                    app.fit_view();
                }
                if ui.button("100%").clicked() {
                    app.reset_zoom();
                }
                ui.label(
                    RichText::new(format!("{:.0}%", app.session.zoom_percent()))
                        .monospace()
                        .strong(),
                );
            });
        });
    });
}

// ----------------------------------------------------------------------
// Side panel
// ----------------------------------------------------------------------

fn side_panel(app: &mut App, ui: &mut Ui) {
    egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(330.0)
        .size_range(220.0..=520.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_width(ui.available_width());
                section_image(app, ui);
                ui.add_space(6.0);
                section_new_folder(app, ui);
                ui.add_space(6.0);
                section_list(app, ui);
                ui.add_space(6.0);
                section_selected(app, ui);
                ui.add_space(6.0);
                section_export(app, ui);
            });
        });
}

fn section_image(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    group(ui, t.section_image);
    if app.image_name.is_empty() {
        ui.label(RichText::new(t.no_image).italics());
    } else {
        ui.label(RichText::new(&app.image_name).strong());
        ui.label(format!(
            "{}×{} px",
            app.image_size.x.round() as i64,
            app.image_size.y.round() as i64
        ));
        if let Some(path) = &app.image_path {
            ui.label(
                RichText::new(shorten(&path.display().to_string(), 38))
                    .small()
                    .monospace(),
            );
        }
    }
    ui.horizontal(|ui| {
        ui.label(t.path);
        ui.add(
            TextEdit::singleline(&mut app.open_path)
                .desired_width((ui.available_width() - 46.0).max(60.0))
                .hint_text(t.path_hint),
        );
    });
    ui.horizontal(|ui| {
        if ui.button(t.load).clicked() {
            app.open_image();
        }
        if ui.button(t.paste).clicked() {
            if let Some(text) = pasted_text(ui.ctx()) {
                text.trim().clone_into(&mut app.open_path);
                if !app.open_path.is_empty() {
                    app.pending_image = Some(PathBuf::from(app.open_path.clone()));
                }
            } else {
                let message = t.no_clipboard.to_owned();
                app.notify_error(message);
            }
        }
    });
    ui.label(RichText::new(t.drag_hint).small().weak());
}

fn section_new_folder(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    group(ui, t.section_new_figures);
    ui.horizontal(|ui| {
        ui.label(t.subfolder);
        let mut folder = app.session.new_folder().to_owned();
        if ui
            .add(
                TextEdit::singleline(&mut folder)
                    .desired_width((ui.available_width() - 84.0).max(60.0))
                    .hint_text(t.subfolder_hint),
            )
            .changed()
        {
            app.session.set_new_folder(&folder);
        }
    });
    let folders = app.session.set().folders();
    if folders.is_empty() {
        ui.label(RichText::new(t.no_folders).small().weak());
    } else {
        let current = app.session.new_folder().to_owned();
        ui.horizontal_wrapped(|ui| {
            for folder in folders {
                if ui
                    .selectable_label(current == folder, folder.clone())
                    .on_hover_text(t.subfolder_tooltip)
                    .clicked()
                {
                    app.session.set_new_folder(&folder);
                }
            }
        });
    }
    ui.label(RichText::new(t.default_name_note).small().weak());

    // Переключатель языка: каждая надпись написана по-русски, чтобы её узнали
    // независимо от текущего языка интерфейса.
    ui.horizontal(|ui| {
        ui.label(t.language);
        for lang in Lang::ALL {
            if ui
                .selectable_label(app.lang == lang, lang.native_name())
                .clicked()
            {
                app.set_lang(lang);
            }
        }
    });
}

fn section_list(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let counts = app.session.counts();
    let title = fill(
        t.figures,
        &[
            ("n", &counts.total.to_string()),
            ("p", &counts.points.to_string()),
            ("r", &counts.rects.to_string()),
        ],
    );
    group(ui, &title);
    ui.horizontal(|ui| {
        if ui.button(t.select_all).clicked() {
            app.session.select_all();
        }
        if ui.button(t.clear_selection).clicked() {
            app.session.clear_selection();
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button(t.renumber)
            .on_hover_text(t.renumber_hint)
            .clicked()
        {
            app.session.renumber();
            let template = app.t().renumbered;
            app.say(template, &[]);
        }
        if ui.button(t.delete_selected).clicked() {
            app.session.delete_selected();
        }
    });

    egui::ScrollArea::vertical()
        .max_height(240.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.session.set().is_empty() {
                ui.label(RichText::new(app.t().list_empty).italics());
            }
            // Snapshot first: the closure needs `&mut app` while painting.
            let rows: Vec<ListRow> = app
                .session
                .set()
                .iter()
                .map(|annotation| ListRow {
                    id: annotation.id,
                    kind: annotation.kind(),
                    name: annotation.display_name(),
                    custom: annotation.has_custom_name(),
                    folder: annotation.folder().to_owned(),
                    color: annotation.color,
                    tip: Session::describe(annotation, app.lang),
                })
                .collect();

            for row in rows {
                let selected = app.session.is_selected(row.id);
                let response = egui::Frame::NONE
                    .fill(if selected {
                        Color32::from_black_alpha(70)
                    } else {
                        Color32::TRANSPARENT
                    })
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, row.color);
                            ui.label(match row.kind {
                                ShapeKind::Point => "•",
                                ShapeKind::Rect => "▭",
                            });
                            let mut label = row.name.clone();
                            if row.custom {
                                label.push('*');
                            }
                            let text = if row.folder.is_empty() {
                                label
                            } else {
                                format!("{label}  ·  {}", row.folder)
                            };
                            let name_response = ui.selectable_label(selected, text);
                            name_response.clone().on_hover_text(row.tip.clone());
                            if name_response.clicked() {
                                app.session.select_only(row.id);
                            }
                            if name_response
                                .context_menu(|ui| {
                                    let t = app.t();
                                    if ui.button(t.menu_delete).clicked() {
                                        app.session.delete_one(row.id);
                                        ui.close();
                                    }
                                    if ui.button(t.menu_reset_name).clicked() {
                                        app.session.set_name(row.id, "");
                                        ui.close();
                                    }
                                })
                                .is_some()
                            {
                                app.session.select_only(row.id);
                            }
                            if ui
                                .small_button("✕")
                                .on_hover_text(app.t().menu_delete)
                                .clicked()
                            {
                                app.session.delete_one(row.id);
                            }
                        });
                    })
                    .response;
                response.on_hover_text(row.tip);
            }
        });
}

/// A row of the figure list, snapshotted so the UI can mutate `app` freely.
struct ListRow {
    id: ShapeId,
    kind: ShapeKind,
    name: String,
    custom: bool,
    folder: String,
    color: Color32,
    tip: String,
}

fn section_selected(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    group(ui, t.section_properties);
    if app.session.selection().is_empty() {
        ui.label(RichText::new(t.nothing_selected).italics());
        return;
    }
    if app.session.selection().len() > 1 {
        let n = app.session.selection().len().to_string();
        ui.label(fill(t.selected_count, &[("n", &n)]));
        ui.horizontal_wrapped(|ui| {
            ui.label(t.colour_for_all);
            for color in palette() {
                if swatch(ui, color, false).clicked() {
                    app.session.set_selection_color(color);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(t.folder);
            let mut folder = app.session.new_folder().to_owned();
            if ui
                .add(
                    TextEdit::singleline(&mut folder)
                        .desired_width((ui.available_width() - 104.0).max(50.0))
                        .hint_text(t.subfolder_hint),
                )
                .changed()
            {
                app.session.set_new_folder(&folder);
            }
            if ui.button("→").on_hover_text(t.subfolder_tooltip).clicked() {
                let folder = app.session.new_folder().to_owned();
                app.session.set_selection_folder(&folder);
            }
        });
        return;
    }

    let Some(id) = app.session.selection().first().copied() else {
        return;
    };
    let Some(annotation) = app.session.set().get(id).cloned() else {
        app.session.clear_selection();
        return;
    };

    let mut name = annotation.display_name();
    ui.horizontal(|ui| {
        ui.label(t.name);
        if ui
            .add(
                TextEdit::singleline(&mut name)
                    .desired_width((ui.available_width() - 44.0).max(60.0)),
            )
            .changed()
        {
            app.session.set_name(id, &name);
        }
    });
    if annotation.has_custom_name() {
        let auto = fill(t.auto_name, &[("name", &annotation.default_name())]);
        ui.horizontal(|ui| {
            ui.label(RichText::new(auto).small().weak());
            if ui.button(t.reset_name).clicked() {
                app.session.set_name(id, "");
            }
        });
    }

    let mut folder = annotation.folder().to_owned();
    ui.horizontal(|ui| {
        ui.label(t.folder);
        if ui
            .add(
                TextEdit::singleline(&mut folder)
                    .desired_width((ui.available_width() - 52.0).max(60.0))
                    .hint_text(t.folder_hint),
            )
            .changed()
        {
            app.session.set_folder(id, &folder);
        }
    });

    ui.horizontal_wrapped(|ui| {
        ui.label(t.colour);
        for color in palette() {
            if swatch(ui, color, color == annotation.color).clicked() {
                app.session.set_color(id, color);
            }
        }
    });

    match annotation.geometry {
        Geometry::Point(p) => {
            ui.label(format!("x = {:.2}    y = {:.2}", p.x, p.y));
        }
        Geometry::Rect(r) => {
            ui.label(format!(
                "x = {:.2}    y = {:.2}\nw = {:.2}    h = {:.2}",
                r.min.x,
                r.min.y,
                r.width(),
                r.height()
            ));
        }
    }

    ui.horizontal(|ui| {
        if ui.button(t.duplicate).clicked() {
            app.session.duplicate(id);
            let template = app.t().duplicated;
            let name = app
                .session
                .set()
                .get(id)
                .map_or_else(String::new, Annotation::display_name);
            app.say(template, &[("name", &name)]);
        }
        if ui.button(t.delete).clicked() {
            app.session.delete_one(id);
        }
    });
}

fn section_export(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    group(ui, t.section_export);
    ui.horizontal(|ui| {
        ui.label(t.json_path);
        ui.add(
            TextEdit::singleline(&mut app.json_path)
                .desired_width((ui.available_width() - 68.0).max(60.0))
                .hint_text(t.json_path_hint),
        );
    });
    ui.horizontal(|ui| {
        ui.label(t.precision);
        ui.add(egui::Slider::new(&mut app.precision, 0..=4).suffix(" "));
    });
    ui.horizontal(|ui| {
        if ui.button(t.save_json).clicked() {
            app.save_json_or_ask();
        }
        if ui.button(t.copy_json).clicked() {
            app.copy_json(ui.ctx(), true);
        }
    });
    ui.horizontal(|ui| {
        let open = ui
            .add_enabled_ui(!app.dialogs.is_open(), |ui| {
                ui.button(t.load_json).on_hover_text(t.browse_hint)
            })
            .inner;
        if open.clicked() {
            app.ask_annotations();
        }
    });
    ui.label(t.split_dir);
    let mut dir = app.split_dir.clone();
    let browse = ui
        .add_enabled_ui(!app.dialogs.is_open(), |ui| {
            ui.button(t.browse).on_hover_text(t.browse_hint)
        })
        .inner;
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut dir)
                .desired_width((ui.available_width() - 88.0 - 92.0).max(60.0))
                .hint_text(t.split_dir_hint),
        );
        if browse.clicked() {
            app.ask_directory();
            dir.clear();
        }
        if ui.button(t.write).clicked() {
            app.split_dir.clone_from(&dir);
            app.save_split(&dir);
        }
    });
    ui.label(RichText::new(t.split_hint).small().weak());
    if !app.last_split.is_empty() {
        ui.label(RichText::new(&app.last_split).small().monospace());
    }

    ui.horizontal(|ui| {
        ui.checkbox(&mut app.show_labels, t.show_labels);
        ui.checkbox(&mut app.show_grid, t.show_grid);
        ui.checkbox(&mut app.show_ruler, t.show_rulers);
    });
    ui.horizontal(|ui| {
        if ui.button(t.hotkeys).clicked() {
            app.show_help = true;
        }
        if ui.button(t.about).clicked() {
            app.show_about = true;
        }
    });
}

// ----------------------------------------------------------------------
// Status bar and canvas
// ----------------------------------------------------------------------

fn status_bar(app: &mut App, ui: &mut Ui) {
    let t = app.t();
    let position = app
        .session
        .pointer()
        .position()
        .map(|p| app.session.view().to_image(p));
    egui::Panel::bottom("status")
        .exact_size(24.0)
        .show(ui, |ui| {
            if let Some(message) = app.session.take_message() {
                let color = if message.is_error {
                    Color32::from_rgb(255, 120, 120)
                } else {
                    Color32::from_rgb(140, 220, 160)
                };
                ui.label(RichText::new(message.text).color(color));
            } else if let Some(p) = position {
                let template = t.cursor_pos;
                let text = fill(
                    template,
                    &[
                        ("x", &format!("{:>9.2}", p.x)),
                        ("y", &format!("{:>9.2}", p.y)),
                    ],
                );
                ui.label(RichText::new(text).monospace());
            } else {
                // Без сообщения и без картинки показываем, что умеет программа.
                let _ = app.has_image();
                ui.label(RichText::new(t.status_hint).weak());
            }
        });
}

fn central(app: &mut App, ui: &mut Ui) {
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let ctx = ui.ctx().clone();
        canvas::show(app, &ctx, ui);
        if app.show_help {
            help_window(app, &ctx);
        }
        if app.show_about {
            about_window(app, &ctx);
        }
    });
}

fn help_window(app: &mut App, ctx: &Context) {
    let t = app.t();
    let lang = app.lang;
    let mut open = app.show_help;
    egui::Window::new(t.hotkeys_title)
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let rows = help_rows(lang);
            egui::Grid::new("help-grid")
                .num_columns(2)
                .striped(true)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(t.hotkeys_action).strong());
                    ui.label(RichText::new(t.hotkeys_meaning).strong());
                    ui.end_row();
                    for (action, meaning) in rows {
                        ui.label(RichText::new(action).monospace());
                        ui.label(meaning);
                        ui.end_row();
                    }
                });
        });
    app.show_help = open;
}

/// One row of the hot-key table: what to press, and what it does.
type HelpRow = (&'static str, &'static str);

/// The hot-key table in `lang`.
#[must_use]
pub(crate) fn help_rows(lang: Lang) -> Vec<HelpRow> {
    match lang {
        Lang::Ru => vec![
            ("ЛКМ (клик)", "точка"),
            ("ЛКМ (протяжка)", "прямоугольник"),
            ("ЛКМ + Shift (протяжка)", "квадрат"),
            ("СКМ / ПКМ (протяжка)", "панорама"),
            ("Alt + ЛКМ", "панорама"),
            ("Колесо мыши", "масштаб"),
            ("D / V / X / H", "рисование / выбор / удаление / панорама"),
            ("F", "вписать изображение"),
            ("Ctrl+S", "сохранить JSON"),
            ("Ctrl+C", "копировать JSON"),
            ("Ctrl+Z / Ctrl+Shift+Z", "отменить / повторить"),
            ("Ctrl+A", "выбрать всё"),
            ("Esc", "снять выделение"),
            ("Delete", "удалить выбранные"),
            ("ПКМ по фигуре", "контекстное меню"),
        ],
        Lang::En => vec![
            ("Left click", "point"),
            ("Left click + drag", "rectangle"),
            ("Left click + Shift + drag", "square"),
            ("Middle / right drag", "pan"),
            ("Alt + left click", "pan"),
            ("Mouse wheel", "zoom"),
            ("D / V / X / H", "draw / select / delete / pan"),
            ("F", "fit the image"),
            ("Ctrl+S", "save JSON"),
            ("Ctrl+C", "copy JSON"),
            ("Ctrl+Z / Ctrl+Shift+Z", "undo / redo"),
            ("Ctrl+A", "select all"),
            ("Esc", "clear the selection"),
            ("Delete", "delete the selected figures"),
            ("Right click on a figure", "context menu"),
        ],
    }
}

fn about_window(app: &mut App, ctx: &Context) {
    let t = app.t();
    let mut open = app.show_about;
    egui::Window::new(t.about_title)
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new("picture-measure").strong().size(18.0));
            ui.label(format!("{} {}", t.version, picture_measure::version()));
            ui.separator();
            ui.label(t.about_line1);
            ui.label(t.about_line2);
            ui.separator();
            ui.hyperlink_to(t.project_url, "https://github.com/ReDeced/picture-measure");
            ui.separator();
            ui.label(RichText::new(t.about_note).small().weak());
        });
    app.show_about = open;
}

/// Human-readable key label for tool buttons.
fn hotkey_label(key: Key) -> String {
    format!("{key:?}")
}

/// Draws a framed section header.
fn group(ui: &mut Ui, title: &str) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 3.0, Color32::from_black_alpha(60));
    ui.painter().text(
        rect.left_top() + vec2(6.0, 3.0),
        Align2::LEFT_TOP,
        title,
        mono(12.0),
        ACCENT,
    );
}

/// Shortens a long path for display.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let tail: String = text
        .chars()
        .rev()
        .take(max - 3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_keeps_the_tail() {
        assert_eq!(shorten("/home/user/a.png", 20), "/home/user/a.png");
        let long = "/very/long/path/to/some/picture/file.png";
        let short = shorten(long, 12);
        assert!(short.starts_with('…'), "{short}");
        assert!(short.chars().count() <= 12, "{short}");
        assert!(short.ends_with("file.png"), "{short}");
        assert!(short.len() < long.len());
        assert_eq!(shorten("", 5), "");
        assert_eq!(shorten("   ", 5), "   ");
    }

    #[test]
    fn click_threshold_is_a_few_pixels() {
        let threshold = picture_measure::tools::DRAG_THRESHOLD_SCREEN;
        assert!((2.0..=10.0).contains(&threshold), "{threshold}");
    }

    #[test]
    fn every_tool_has_a_hotkey_and_a_label_in_both_languages() {
        let expected = [
            (Lang::Ru, ["Выбор", "Панорама", "Рисование", "Удаление"]),
            (Lang::En, ["Draw", "Pan", "Select", "Delete"]),
        ];
        for (lang, want) in expected {
            let labels: std::collections::BTreeSet<&str> =
                Tool::ALL.iter().map(|t| t.label(lang)).collect();
            assert_eq!(
                labels,
                want.into_iter().collect::<std::collections::BTreeSet<_>>(),
                "{lang}: набор подписей инструментов изменился"
            );
        }

        let mut hotkeys = std::collections::HashSet::new();
        for tool in Tool::ALL {
            assert_ne!(tool.hotkey(), Key::Escape, "Esc занят для снятия выделения");
            let key = tool.hotkey();
            assert!(
                hotkeys.insert(key),
                "горячая клавиша {key:?} продублирована"
            );
            for lang in Lang::ALL {
                assert!(
                    tool.description(lang).trim().chars().count() > 10,
                    "{lang}: у инструмента {:?} слишком короткое описание",
                    tool.label(lang)
                );
                assert_ne!(
                    tool.description(Lang::Ru),
                    tool.description(Lang::En),
                    "описание инструмента {:?} не переведено",
                    tool.label(Lang::Ru)
                );
            }
        }
    }

    #[test]
    fn the_hotkey_table_is_translated() {
        for lang in Lang::ALL {
            let rows = help_rows(lang);
            assert_eq!(rows.len(), 15, "{lang}: изменилось число строк");
            for (action, meaning) in rows {
                assert!(
                    !action.trim().is_empty(),
                    "{lang}: пустая клавиша в строке «{meaning}»"
                );
                assert!(
                    meaning.trim().chars().count() >= 3,
                    "{lang}: слишком короткое пояснение для «{action}»"
                );
            }
        }
        assert_ne!(help_rows(Lang::Ru), help_rows(Lang::En));
    }

    /// Прогоняет весь интерфейс через egui без окна и возвращает весь текст,
    /// который egui реально разместил на экране.
    ///
    /// egui собирает настоящее дерево виджетов, поэтому тест видит ровно те
    /// надписи, которые окажутся на экране.
    fn render_everything(app: &mut App) -> Vec<String> {
        app.render_headless()
    }

    /// Склеивает размещённый текст в одну строку для удобного поиска.
    #[must_use]
    fn rendered_text(app: &mut App) -> String {
        render_everything(app).join("\u{1}")
    }

    #[test]
    fn the_english_interface_really_renders() {
        let mut app = test_app();
        app.lang = Lang::En;
        app.image_name = "photo.png".to_owned();
        app.image_size = vec2(1920.0, 1080.0);
        app.canvas_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(1200.0, 800.0),
        ));
        app.session.add_point(egui::pos2(10.0, 20.0));
        app.session
            .add_rect(picture_measure::geometry::Rect::from_xywh(
                0.0, 0.0, 40.0, 30.0,
            ));
        app.session.set_new_folder("eyes");

        let text = rendered_text(&mut app);

        // Надписи панелей.
        for expected in [
            "Open",
            "Clear",
            "Save JSON",
            "Copy JSON",
            "Undo",
            "Redo",
            "Tool:",
            "Fit",
            "Image",
            "New figures",
            "Sub-folder:",
            "Figures:",
            "Selected figure",
            "Export",
            "JSON path:",
            "Decimal places:",
            "Labels",
            "Grid",
            "Rulers",
            "Hotkeys",
            "About",
        ] {
            assert!(
                text.contains(expected),
                "на экране нет строки «{expected}»:\n{text}"
            );
        }
        // Подписи инструментов — на своём, английском языке.
        for tool in Tool::ALL {
            let label = tool.label(Lang::En);
            assert!(text.contains(label), "нет кнопки «{label}»:\n{text}");
        }
        // Счётчик фигур собран с подстановками.
        assert!(
            text.contains("Figures: 2"),
            "счётчик не подставился:\n{text}"
        );
        // И русских надписей быть не должно.
        for forbidden in ["Открыть", "Сохранить", "Инструмент", "Экспорт", "Фигуры"]
        {
            assert!(
                !text.contains(forbidden),
                "русская строка «{forbidden}» попала в английский интерфейс:\n{text}"
            );
        }
    }

    #[test]
    fn the_russian_interface_really_renders() {
        let mut app = test_app();
        app.lang = Lang::Ru;
        app.canvas_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(1200.0, 800.0),
        ));
        app.session.add_point(egui::pos2(10.0, 20.0));
        let text = rendered_text(&mut app);

        for expected in [
            "Открыть",
            "Инструмент:",
            "Изображение",
            "Экспорт",
            "Рисование",
        ] {
            assert!(
                text.contains(expected),
                "на экране нет строки «{expected}»:\n{text}"
            );
        }
        assert!(
            text.contains("Фигуры: 1"),
            "счётчик не подставился:\n{text}"
        );
        for forbidden in ["Open", "Save JSON", "Tool:", "Export"] {
            assert!(
                !text.contains(forbidden),
                "английская строка «{forbidden}» попала в русский интерфейс:\n{text}"
            );
        }
    }

    #[test]
    fn switching_language_changes_what_is_drawn() {
        let mut app = test_app();
        app.canvas_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(1200.0, 800.0),
        ));

        let ru = rendered_text(&mut app);
        assert!(ru.contains("Инструмент:"));

        app.set_lang(Lang::En);
        let en = rendered_text(&mut app);
        assert!(en.contains("Tool:"));
        assert!(!en.contains("Инструмент:"));

        app.set_lang(Lang::Ru);
        assert!(rendered_text(&mut app).contains("Инструмент:"));
    }

    #[test]
    fn no_message_leaves_its_placeholders_unfilled() {
        // Ошибки и статусы показываются через `fill`; незаполненное `{ключ}`
        // означало бы, что шаблон перепутали с текстом.
        let mut app = test_app();
        app.lang = Lang::Ru;
        app.canvas_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(1200.0, 800.0),
        ));

        // Пустой путь — намеренная ошибка: сообщение должно быть
        // заполненным, а не содержать сырой `{placeholder}`.
        app.json_path = String::new();
        app.save_json();

        let message = app.session.message().expect("сообщение об ошибке").clone();
        assert!(
            !message.text.contains('{'),
            "незаполненный ключ: {message:?}"
        );
        assert!(message.is_error, "это должно быть ошибкой");

        app.load_annotations(std::path::Path::new("/no/such/file.json"));
        let message = app.session.message().expect("сообщение об ошибке").clone();
        assert!(
            !message.text.contains('{'),
            "незаполненный ключ: {message:?}"
        );
    }

    #[test]
    fn broken_json_reports_the_problem_in_the_chosen_language() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, r#"[{"type":"rect","x":0,"y":0}]"#).unwrap();

        let mut app = test_app();
        app.lang = Lang::En;
        app.load_annotations(&path);
        let en = app.session.message().expect("сообщение").clone();
        assert!(en.is_error);
        assert!(en.text.contains("width"), "{en:?}");

        app.lang = Lang::Ru;
        app.load_annotations(&path);
        let ru = app.session.message().expect("сообщение").clone();
        assert!(ru.text.contains("width"), "{ru:?}");
        assert_ne!(en.text, ru.text, "перевод не применился");
    }

    #[test]
    fn the_whole_interface_builds_in_both_languages() {
        for lang in Lang::ALL {
            let mut app = test_app();
            app.lang = lang;
            // Немного фигур, чтобы наполнить список и блок свойств.
            let a = app.session.add_point(egui::pos2(10.0, 20.0));
            app.session
                .add_rect(picture_measure::geometry::Rect::from_xywh(
                    0.0, 0.0, 40.0, 30.0,
                ));
            app.session.select_only(a);
            app.session.set_new_folder("eyes");

            // Три кадра: первый строит дерево, остальные проверяют, что
            // состояние (выделение, прокрутка, попапы) устойчиво.
            for _ in 0..3 {
                render_everything(&mut app);
            }
            assert!(
                !app.ctx.memory(|_| false) && app.session.set().len() == 2,
                "{lang}: интерфейс потерял фигуры"
            );
        }
    }

    #[test]
    fn the_about_and_help_windows_open_in_both_languages() {
        for lang in Lang::ALL {
            for window in [true, false] {
                let mut app = test_app();
                app.lang = lang;
                app.show_help = window;
                app.show_about = window;
                app.canvas_rect = Some(egui::Rect::from_min_max(
                    egui::pos2(0.0, 0.0),
                    egui::pos2(800.0, 600.0),
                ));
                for _ in 0..2 {
                    render_everything(&mut app);
                }
            }
        }
    }

    #[test]
    fn a_full_workspace_renders_with_an_image_and_a_selection() {
        let mut app = test_app();
        app.lang = Lang::En;
        app.image_name = "photo.png".to_owned();
        app.image_size = vec2(1920.0, 1080.0);
        app.canvas_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(1200.0, 800.0),
        ));
        let a = app.session.add_point(egui::pos2(5.0, 5.0));
        app.session
            .add_rect(picture_measure::geometry::Rect::from_xywh(
                1.0, 2.0, 30.0, 40.0,
            ));
        app.session.select_only(a);
        app.session.set_folder(a, "eyes");
        app.session.renumber();
        for _ in 0..3 {
            render_everything(&mut app);
        }
        assert_eq!(app.session.set().len(), 2);
    }

    #[test]
    fn layout_types_are_available() {
        let _: Layout = Layout::left_to_right(Align::Center);
        let _ = vec2(1.0, 2.0);
        let _: Color32 = Color32::RED;
    }
}
