//! `picture-measure` — a small desktop tool for measuring images.
//!
//! Load a picture, click to place points, drag to place rectangles
//! (<kbd>Shift</kbd> for squares), name them, group them into sub-folders and
//! export everything as JSON.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Пояснение к `assert_is_empty` — в src/lib.rs.
#![allow(unknown_lints)]
#![allow(clippy::assert_is_empty)]

mod gui;

use gui::CliArgs;
use picture_measure::i18n::Lang;
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "\
picture-measure — измерение картинок точками и прямоугольниками

ИСПОЛЬЗОВАНИЕ:
    picture-measure [ИЗОБРАЖЕНИЕ] [РАЗМЕТКА.json]

    Без аргументов открывается пустое окно; изображение можно перетащить
    в него мышкой или указать путь на боковой панели.

АРГУМЕНТЫ:
    <ИЗОБРАЖЕНИЕ>       PNG, JPEG, WebP или BMP
    <РАЗМЕТКА.json>     готовые аннотации, которые нужно загрузить

ПАРАМЕТРЫ:
    --lang <ru|en>      язык интерфейса (по умолчанию — из переменной LANG)
    -h, --help          показать эту справку
    -V, --version       показать версию

УПРАВЛЕНИЕ:
    ЛКМ (клик)                поставить точку
    ЛКМ (зажать и потянуть)   нарисовать прямоугольник
    Shift + ЛКМ (потянуть)    нарисовать квадрат
    СКМ / ПКМ (потянуть)      панорама
    Колесо мыши                масштаб
    D / V / X / H              рисование / выбор / удаление / панорама

Язык интерфейса переключается прямо в программе: раздел «Новые фигуры».

Документация: https://github.com/ReDeced/picture-measure
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Action::Help => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("picture-measure {}", picture_measure::version());
            ExitCode::SUCCESS
        }
        Action::Run(cli) => match gui::run(window_options(cli.lang), cli) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("не удалось запустить окно: {error}");
                ExitCode::FAILURE
            }
        },
        Action::Error(message) => {
            eprintln!("{message}\n");
            eprint!("{HELP}");
            ExitCode::FAILURE
        }
    }
}

/// What the command line asked for.
#[derive(Debug, PartialEq)]
enum Action {
    /// Print the help text and exit.
    Help,
    /// Print the version and exit.
    Version,
    /// Start the GUI.
    Run(CliArgs),
    /// Print an error and exit.
    Error(String),
}

/// Parses the command line.
fn parse_args(args: &[String]) -> Action {
    let mut cli = CliArgs::default();
    let mut positional: Vec<String> = Vec::new();
    let mut args = args.iter().map(String::as_str);

    while let Some(arg) = args.next() {
        match arg {
            "-h" | "--help" => return Action::Help,
            "-V" | "--version" => return Action::Version,
            "--lang" => {
                let Some(value) = args.next() else {
                    return Action::Error("после --lang нужен код языка (ru или en)".to_owned());
                };
                match value.parse::<Lang>() {
                    Ok(lang) => cli.lang = Some(lang),
                    Err(error) => return Action::Error(error.to_string()),
                }
            }
            other if other.starts_with("--lang=") => {
                let value = &other["--lang=".len()..];
                match value.parse::<Lang>() {
                    Ok(lang) => cli.lang = Some(lang),
                    Err(error) => return Action::Error(error.to_string()),
                }
            }
            other if other.starts_with('-') => {
                return Action::Error(format!("неизвестный параметр: {other}"));
            }
            other => positional.push(other.to_owned()),
        }
    }

    if positional.len() > 2 {
        return Action::Error(format!(
            "слишком много аргументов: {} (ожидалось не больше двух)",
            positional.len()
        ));
    }

    if let Some(path) = positional.first() {
        if !picture_measure::json_io::is_image_path(std::path::Path::new(path)) {
            return Action::Error(format!(
                "неподдерживаемый формат изображения: {path} (нужны PNG, JPEG, WebP, BMP)"
            ));
        }
        cli.image = Some(PathBuf::from(path));
    }
    if let Some(path) = positional.get(1) {
        if !path.to_ascii_lowercase().ends_with(".json") {
            return Action::Error(format!("ожидался файл .json, а получено: {path}"));
        }
        cli.annotations = Some(PathBuf::from(path));
    }

    Action::Run(cli)
}

/// Window title before the GUI has a chance to set its own.
fn window_title_for(lang: Option<Lang>) -> String {
    picture_measure::i18n::Text::get(lang.unwrap_or_else(picture_measure::i18n::from_environment))
        .window_title
        .to_owned()
}

/// Default window options.
fn window_options(cli_lang: Option<Lang>) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(window_title_for(cli_lang))
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([860.0, 560.0])
            .with_drag_and_drop(true),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse_args(&args(&["--help"])), Action::Help);
        assert_eq!(parse_args(&args(&["-h"])), Action::Help);
        assert_eq!(parse_args(&args(&["--version"])), Action::Version);
        assert_eq!(parse_args(&args(&["-V"])), Action::Version);
    }

    #[test]
    fn no_arguments_opens_an_empty_window() {
        assert_eq!(parse_args(&args(&[])), Action::Run(CliArgs::default()));
    }

    #[test]
    fn image_and_json_are_accepted() {
        let Action::Run(cli) = parse_args(&args(&["/tmp/a.png", "/tmp/a.json"])) else {
            panic!("expected Run");
        };
        assert_eq!(cli.image, Some(PathBuf::from("/tmp/a.png")));
        assert_eq!(cli.annotations, Some(PathBuf::from("/tmp/a.json")));
    }

    #[test]
    fn unknown_flags_are_rejected() {
        let Action::Error(message) = parse_args(&args(&["--wat"])) else {
            panic!("expected an error");
        };
        assert!(message.contains("--wat"), "{message}");
    }

    #[test]
    fn language_can_be_chosen() {
        for (input, want) in [
            (vec!["--lang", "en"], Lang::En),
            (vec!["--lang", "ru"], Lang::Ru),
            (vec!["--lang=en"], Lang::En),
            (vec!["--lang=ru"], Lang::Ru),
        ] {
            let Action::Run(cli) = parse_args(&args(&input)) else {
                panic!("{input:?}: ожидался запуск программы");
            };
            assert_eq!(cli.lang, Some(want), "{input:?}");
        }
    }

    #[test]
    fn language_without_a_value_is_rejected() {
        assert!(matches!(parse_args(&args(&["--lang"])), Action::Error(_)));
    }

    #[test]
    fn unknown_language_is_rejected() {
        let Action::Error(message) = parse_args(&args(&["--lang", "de"])) else {
            panic!("ожидалась ошибка");
        };
        assert!(message.contains("de"), "{message}");
    }

    #[test]
    fn language_can_come_before_the_files() {
        let Action::Run(cli) = parse_args(&args(&["--lang", "en", "/tmp/a.png"])) else {
            panic!("expected Run");
        };
        assert_eq!(cli.lang, Some(Lang::En));
        assert_eq!(cli.image, Some(PathBuf::from("/tmp/a.png")));
    }

    #[test]
    fn the_window_title_follows_the_language() {
        assert!(window_title_for(Some(Lang::Ru)).contains("измерение"));
        assert!(window_title_for(Some(Lang::En)).contains("measurement"));
        assert_ne!(
            window_title_for(Some(Lang::Ru)),
            window_title_for(Some(Lang::En))
        );
    }

    #[test]
    fn help_documents_the_language_option() {
        assert!(HELP.contains("--lang"));
        assert!(HELP.contains("ru|en"));
    }

    #[test]
    fn bad_formats_are_rejected() {
        assert!(matches!(
            parse_args(&args(&["/tmp/a.txt"])),
            Action::Error(_)
        ));
        assert!(matches!(
            parse_args(&args(&["/tmp/a.png", "/tmp/a.txt"])),
            Action::Error(_)
        ));
        assert!(matches!(
            parse_args(&args(&["a", "b", "c"])),
            Action::Error(_)
        ));
    }

    #[test]
    fn help_text_mentions_the_key_bindings() {
        assert!(HELP.contains("Shift"));
        assert!(HELP.contains("точка"));
        assert!(HELP.contains("прямоугольник"));
        assert!(HELP.contains("--version"));
    }

    #[test]
    fn window_options_are_sane() {
        let options = window_options(None);
        assert!(options.viewport.inner_size.unwrap().x >= 800.0);
    }
}
