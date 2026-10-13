//! `picture-measure` — a small desktop tool for measuring images.
//!
//! Load a picture, click to place points, drag to place rectangles
//! (<kbd>Shift</kbd> for squares), name them, group them into sub-folders and
//! export everything as JSON.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;

use gui::CliArgs;
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
    -h, --help          показать эту справку
    -V, --version       показать версию

УПРАВЛЕНИЕ:
    ЛКМ (клик)                поставить точку
    ЛКМ (зажать и потянуть)   нарисовать прямоугольник
    Shift + ЛКМ (потянуть)    нарисовать квадрат
    СКМ / ПКМ (потянуть)      панорама
    Колесо мыши                масштаб
    D / V / X / H              рисование / выбор / удаление / панорама

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
        Action::Run(cli) => match gui::run(window_options(), cli) {
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

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Action::Help,
            "-V" | "--version" => return Action::Version,
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

/// Default window options.
fn window_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("picture-measure — измерение картинок")
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
        let options = window_options();
        assert!(options.viewport.inner_size.unwrap().x >= 800.0);
    }
}
