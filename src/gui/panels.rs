//! The surrounding chrome: toolbar, side panel, status bar and the frame loop.

use super::app::{ACCENT, App, bare_key, mono, pasted_text, shortcut, status_hint, swatch};
use super::canvas;
use egui::{Align, Align2, Color32, Context, Key, Layout, RichText, TextEdit, Ui, vec2};
use picture_measure::json_io::{self, palette};
use picture_measure::model::{Geometry, ShapeId, ShapeKind};
use picture_measure::session::Session;
use picture_measure::tools::Tool;
use std::path::PathBuf;

/// One frame of the whole application.
pub(crate) fn frame(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    handle_shortcuts(app, &ctx);
    handle_dropped_files(app, &ctx);
    handle_pending_loads(app, &ctx);
    if app.fit_pending {
        app.fit_view();
    }

    top_bar(app, ui);
    side_panel(app, ui);
    status_bar(app, ui);
    central(app, ui);
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
        app.save_json();
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
            app.notify_error(format!("Не удалось определить формат: {}", path.display()));
        }
    }
}

// ----------------------------------------------------------------------
// Toolbar
// ----------------------------------------------------------------------

fn top_bar(app: &mut App, ui: &mut Ui) {
    egui::Panel::top("toolbar").exact_size(74.0).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.button("📂 Открыть").clicked() {
                    let path = app.open_path.trim().to_owned();
                    if path.is_empty() {
                        app.notify_error("Введите путь к изображению на панели слева");
                    } else {
                        app.pending_image = Some(PathBuf::from(path));
                    }
                }
                if ui.button("🗑 Очистить").clicked() {
                    app.session.clear_all();
                }
                ui.separator();
                if ui.button("💾 Сохранить JSON").clicked() {
                    app.save_json();
                }
                if ui.button("📋 Копировать JSON").clicked() {
                    app.copy_json(ui.ctx(), true);
                }
                ui.separator();
                ui.add_enabled_ui(app.session.can_undo(), |ui| {
                    if ui.button("↶ Отменить").clicked() {
                        app.session.undo();
                    }
                });
                ui.add_enabled_ui(app.session.can_redo(), |ui| {
                    if ui.button("↷ Повторить").clicked() {
                        app.session.redo();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Инструмент:");
                let current = app.session.tool();
                for tool in Tool::ALL {
                    let button = egui::Button::new(tool.label())
                        .selected(current == tool)
                        .shortcut_text(hotkey_label(tool.hotkey()));
                    if ui.add(button).on_hover_text(tool.description()).clicked() {
                        app.session.set_tool(tool);
                    }
                }
                ui.separator();
                if ui.button("⤢ Вписать").on_hover_text("F").clicked() {
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
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if app.session.is_dirty() {
                        ui.label(
                            RichText::new("● есть изменения")
                                .color(Color32::from_rgb(255, 170, 90)),
                        );
                    }
                    ui.label(format!("v{}", picture_measure::version()));
                });
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
    group(ui, "Изображение");
    if app.image_name.is_empty() {
        ui.label(RichText::new("не открыто").italics());
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
        ui.label("Путь:");
        ui.add(
            TextEdit::singleline(&mut app.open_path)
                .desired_width((ui.available_width() - 46.0).max(60.0))
                .hint_text("/путь/к/картинке.png"),
        );
    });
    ui.horizontal(|ui| {
        if ui.button("Открыть").clicked() {
            let path = app.open_path.trim().to_owned();
            if path.is_empty() {
                app.notify_error("Введите путь к изображению");
            } else {
                app.pending_image = Some(PathBuf::from(path));
            }
        }
        if ui.button("Вставить").clicked() {
            if let Some(text) = pasted_text(ui.ctx()) {
                text.trim().clone_into(&mut app.open_path);
                if !app.open_path.is_empty() {
                    app.pending_image = Some(PathBuf::from(app.open_path.clone()));
                }
            } else {
                app.notify_error("В буфере обмена нет текста");
            }
        }
    });
    ui.label(
        RichText::new("Можно просто перетащить файл в окно")
            .small()
            .weak(),
    );
}

fn section_new_folder(app: &mut App, ui: &mut Ui) {
    group(ui, "Новые фигуры");
    ui.horizontal(|ui| {
        ui.label("Подпапка:");
        let mut folder = app.session.new_folder().to_owned();
        if ui
            .add(
                TextEdit::singleline(&mut folder)
                    .desired_width((ui.available_width() - 84.0).max(60.0))
                    .hint_text("например eyes"),
            )
            .changed()
        {
            app.session.set_new_folder(&folder);
        }
    });
    let folders = app.session.set().folders();
    if folders.is_empty() {
        ui.label(RichText::new("Подпапок пока нет").small().weak());
    } else {
        let current = app.session.new_folder().to_owned();
        ui.horizontal_wrapped(|ui| {
            for folder in folders {
                if ui
                    .selectable_label(current == folder, folder.clone())
                    .on_hover_text("Все новые фигуры попадут сюда")
                    .clicked()
                {
                    app.session.set_new_folder(&folder);
                }
            }
        });
    }
    ui.label(
        RichText::new(format!(
            "Имя по умолчанию: {} / {}",
            picture_measure::POINT_PREFIX,
            picture_measure::RECT_PREFIX
        ))
        .small()
        .weak(),
    );
}

fn section_list(app: &mut App, ui: &mut Ui) {
    let counts = app.session.counts();
    group(
        ui,
        &format!(
            "Фигуры: {} (точек {}, прямоугольников {})",
            counts.total, counts.points, counts.rects
        ),
    );
    ui.horizontal(|ui| {
        if ui.button("Выбрать все").clicked() {
            app.session.select_all();
        }
        if ui.button("Снять выделение").clicked() {
            app.session.clear_selection();
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button("Перенумеровать")
            .on_hover_text("Сделать имена point_1, rect_2, … подряд")
            .clicked()
        {
            app.session.renumber();
        }
        if ui.button("Удалить выбранные").clicked() {
            app.session.delete_selected();
        }
    });

    egui::ScrollArea::vertical()
        .max_height(240.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.session.set().is_empty() {
                ui.label(RichText::new("Пока пусто").italics());
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
                    tip: Session::describe(annotation),
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
                                    if ui.button("Удалить").clicked() {
                                        app.session.delete_one(row.id);
                                        ui.close();
                                    }
                                    if ui.button("Вернуть имя по умолчанию").clicked()
                                    {
                                        app.session.set_name(row.id, "");
                                        ui.close();
                                    }
                                })
                                .is_some()
                            {
                                app.session.select_only(row.id);
                            }
                            if ui.small_button("✕").on_hover_text("Удалить").clicked() {
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
    group(ui, "Свойства выбранного");
    if app.session.selection().is_empty() {
        ui.label(RichText::new("Ничего не выбрано").italics());
        return;
    }
    if app.session.selection().len() > 1 {
        ui.label(format!("Выбрано фигур: {}", app.session.selection().len()));
        ui.horizontal_wrapped(|ui| {
            ui.label("Цвет для всех:");
            for color in palette() {
                if swatch(ui, color, false).clicked() {
                    app.session.set_selection_color(color);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Папка:");
            let mut folder = app.session.new_folder().to_owned();
            if ui
                .add(
                    TextEdit::singleline(&mut folder)
                        .desired_width((ui.available_width() - 104.0).max(50.0))
                        .hint_text("eyes"),
                )
                .changed()
            {
                app.session.set_new_folder(&folder);
            }
            if ui
                .button("→")
                .on_hover_text("Переложить выбранные в эту папку")
                .clicked()
            {
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
        ui.label("Имя:");
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
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("авто-имя: {}", annotation.default_name()))
                    .small()
                    .weak(),
            );
            if ui.button("Сбросить").clicked() {
                app.session.set_name(id, "");
            }
        });
    }

    let mut folder = annotation.folder().to_owned();
    ui.horizontal(|ui| {
        ui.label("Папка:");
        if ui
            .add(
                TextEdit::singleline(&mut folder)
                    .desired_width((ui.available_width() - 52.0).max(60.0))
                    .hint_text("без папки"),
            )
            .changed()
        {
            app.session.set_folder(id, &folder);
        }
    });

    ui.horizontal_wrapped(|ui| {
        ui.label("Цвет:");
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
        if ui.button("Дублировать").clicked() {
            app.session.duplicate(id);
        }
        if ui.button("Удалить").clicked() {
            app.session.delete_one(id);
        }
    });
}

fn section_export(app: &mut App, ui: &mut Ui) {
    group(ui, "Экспорт");
    ui.horizontal(|ui| {
        ui.label("Путь JSON:");
        ui.add(
            TextEdit::singleline(&mut app.json_path)
                .desired_width((ui.available_width() - 68.0).max(60.0))
                .hint_text("annotations.json"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Знаков после запятой:");
        ui.add(
            egui::Slider::new(&mut app.precision, 0..=4)
                .suffix(" ")
                .text("decimal"),
        );
    });
    ui.horizontal(|ui| {
        if ui.button("Сохранить").clicked() {
            app.save_json();
        }
        if ui.button("Копировать").clicked() {
            app.copy_json(ui.ctx(), true);
        }
    });
    ui.label("Папка для раздельного экспорта:");
    let mut dir = app.split_dir.clone();
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut dir)
                .desired_width((ui.available_width() - 88.0).max(60.0))
                .hint_text("out/"),
        );
        if ui.button("Записать").clicked() {
            app.split_dir.clone_from(&dir);
            app.save_split(&dir);
        }
    });
    ui.label(
        RichText::new("Каждая фигура сохраняется отдельным файлом: <папка>/<имя>.json")
            .small()
            .weak(),
    );
    if !app.last_split.is_empty() {
        ui.label(RichText::new(&app.last_split).small().monospace());
    }

    ui.horizontal(|ui| {
        ui.checkbox(&mut app.show_labels, "Подписи");
        ui.checkbox(&mut app.show_grid, "Сетка");
        ui.checkbox(&mut app.show_ruler, "Линейки");
    });
    ui.horizontal(|ui| {
        if ui.button("Горячие клавиши").clicked() {
            app.show_help = true;
        }
        if ui.button("О проекте").clicked() {
            app.show_about = true;
        }
    });
}

// ----------------------------------------------------------------------
// Status bar and canvas
// ----------------------------------------------------------------------

fn status_bar(app: &mut App, ui: &mut Ui) {
    egui::Panel::bottom("status")
        .exact_size(24.0)
        .show(ui, |ui| {
            let message = app.session.take_message();
            ui.horizontal(|ui| {
                if let Some(message) = message {
                    let color = if message.is_error {
                        Color32::from_rgb(255, 120, 120)
                    } else {
                        Color32::from_rgb(140, 220, 160)
                    };
                    ui.label(RichText::new(message.text).color(color));
                } else if app.has_image() {
                    match app
                        .session
                        .pointer()
                        .position()
                        .map(|p| app.session.view().to_image(p))
                    {
                        Some(p) => {
                            ui.label(
                                RichText::new(format!("x = {:>9.2}   y = {:>9.2}", p.x, p.y))
                                    .monospace(),
                            );
                        }
                        None => {
                            ui.label(RichText::new(status_hint()).weak());
                        }
                    }
                } else {
                    ui.label(RichText::new(status_hint()).weak());
                }
            });
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
    let mut open = app.show_help;
    egui::Window::new("Горячие клавиши")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let rows: [(&str, &str); 15] = [
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
            ];
            egui::Grid::new("help-grid")
                .num_columns(2)
                .striped(true)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Действие").strong());
                    ui.label(RichText::new("Смысл").strong());
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

fn about_window(app: &mut App, ctx: &Context) {
    let mut open = app.show_about;
    egui::Window::new("О программе")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new("picture-measure").strong().size(18.0));
            ui.label(format!("версия {}", picture_measure::version()));
            ui.separator();
            ui.label("Измеряйте картинки точками и прямоугольниками,");
            ui.label("называйте их и выгружайте разметку в JSON.");
            ui.separator();
            ui.hyperlink_to(
                "github.com/ReDeced/picture-measure",
                "https://github.com/ReDeced/picture-measure",
            );
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "Координаты: пиксели, начало — левый верхний угол.\n\
                     Имена по умолчанию: {}_N и {}_N, где N — номер фигуры.",
                    picture_measure::POINT_PREFIX,
                    picture_measure::RECT_PREFIX
                ))
                .small()
                .weak(),
            );
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
    fn every_tool_has_a_hotkey_and_a_label() {
        // Интерфейс полностью русскоязычный — такие подписи видит пользователь.
        let labels: std::collections::BTreeSet<&str> =
            Tool::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(
            labels,
            ["Выбор", "Панорама", "Рисование", "Удаление"].into(),
            "набор подписей инструментов изменился"
        );

        let mut hotkeys = std::collections::HashSet::new();
        for tool in Tool::ALL {
            assert_ne!(tool.hotkey(), Key::Escape, "Esc занят для снятия выделения");
            let key = tool.hotkey();
            assert!(
                hotkeys.insert(key),
                "горячая клавиша {key:?} продублирована"
            );
            assert!(
                tool.description().trim().chars().count() > 10,
                "у инструмента {:?} слишком короткое описание",
                tool.label()
            );
        }
    }

    #[test]
    fn layout_types_are_available() {
        let _: Layout = Layout::left_to_right(Align::Center);
        let _ = vec2(1.0, 2.0);
        let _: Color32 = Color32::RED;
    }
}
