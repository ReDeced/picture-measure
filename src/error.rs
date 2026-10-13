//! Localisation of the user-visible error messages.
//!
//! Errors are built once, without any text, and rendered per language. That
//! keeps the parsing logic free of formatting and lets the tests assert on the
//! *structure* of a problem rather than on a Russian sentence.
//!
//! ```
//! ```
//! use picture_measure::error::{Error, Problem};
//! use picture_measure::i18n::Lang;
//!
//! let error = Error::Figure {
//!     index: 0,
//!     problem: Problem::MissingWidth,
//! };
//! assert!(error.message(Lang::Ru).contains("width"));
//! assert!(error.message(Lang::En).contains("width"));
//! ```
//! ```

use crate::i18n::{Lang, fill};
use std::fmt;

/// Anything that can go wrong while reading or writing an annotation file.
#[derive(Debug)]
pub enum Error {
    /// The text is not valid JSON.
    Syntax {
        /// What the JSON parser complained about.
        source: serde_json::Error,
        /// Line and column, when the parser reported them.
        position: Option<(usize, usize)>,
    },
    /// Valid JSON that is not an annotation file.
    Structure {
        /// Which expectation was violated.
        problem: Structure,
    },
    /// One figure could not be converted.
    Figure {
        /// Zero-based index inside the `annotations` array.
        index: usize,
        /// What is wrong with it.
        problem: Problem,
    },
    /// A file could not be read or written.
    Io {
        /// The path that failed.
        path: String,
        /// What the operating system reported.
        source: std::io::Error,
    },
}

/// Why a document is not an annotation file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Structure {
    /// The top-level value is neither an object nor an array.
    NotAContainer,
    /// The `annotations` field exists but is not an array.
    AnnotationsNotAnArray,
    /// An object with no `annotations` field.
    AnnotationsMissing,
    /// An array element is not an object.
    FigureNotAnObject,
}

/// What is wrong with a single figure.
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    /// `type` is absent and cannot be inferred.
    TypeMissing,
    /// `type` names something we do not understand.
    TypeUnknown {
        /// The value that was written.
        given: String,
    },
    /// A rectangle has no `width`.
    MissingWidth,
    /// A rectangle has no `height`.
    MissingHeight,
    /// A rectangle has a negative size.
    NegativeSize {
        /// Width as written.
        width: f32,
        /// Height as written.
        height: f32,
    },
    /// A coordinate is not finite (NaN or infinity).
    CoordinateNotFinite,
}

impl PartialEq for Error {
    /// Compares what we can: the shape of the problem and, for wrapped
    /// operating-system or parser errors, their message.
    ///
    /// [`std::io::Error`] does not implement `PartialEq`, so two errors of the
    /// same kind are compared by their text instead.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Syntax {
                    source: a,
                    position: pa,
                },
                Self::Syntax {
                    source: b,
                    position: pb,
                },
            ) => a.to_string() == b.to_string() && pa == pb,
            (Self::Structure { problem: a }, Self::Structure { problem: b }) => a == b,
            (
                Self::Figure {
                    index: ia,
                    problem: a,
                },
                Self::Figure {
                    index: ib,
                    problem: b,
                },
            ) => ia == ib && a == b,
            (
                Self::Io {
                    path: pa,
                    source: a,
                },
                Self::Io {
                    path: pb,
                    source: b,
                },
            ) => pa == pb && a.to_string() == b.to_string(),
            _ => false,
        }
    }
}

impl Eq for Error {}

impl Error {
    /// Builds a JSON syntax error, extracting the position when available.
    #[must_use]
    pub fn syntax(source: serde_json::Error) -> Self {
        let position = Some((source.line(), source.column()));
        Self::Syntax { source, position }
    }

    /// Renders the message in `lang`.
    ///
    /// Errors coming from `serde_json` and the standard library stay in
    /// English, because translating a Rust or JSON parser is neither possible
    /// nor desirable; only our own sentences are localised.
    #[must_use]
    pub fn message(&self, lang: Lang) -> String {
        match self {
            Self::Syntax { source, position } => {
                let detail = match position {
                    Some((line, column)) => format!(" (строка {line}, столбец {column})"),
                    None => String::new(),
                };
                match lang {
                    Lang::Ru => format!("некорректный JSON{detail}: {source}"),
                    Lang::En => format!("invalid JSON{detail}: {source}"),
                }
            }
            Self::Structure { problem } => {
                let reason = match (lang, problem) {
                    (Lang::Ru, Structure::NotAContainer) => "ожидался объект или массив",
                    (Lang::Ru, Structure::AnnotationsNotAnArray) => {
                        "поле \"annotations\" должно быть массивом"
                    }
                    (Lang::Ru, Structure::AnnotationsMissing) => {
                        "в объекте нет поля \"annotations\""
                    }
                    (Lang::Ru, Structure::FigureNotAnObject) => {
                        "элемент массива должен быть объектом"
                    }
                    (Lang::En, Structure::NotAContainer) => "expected an object or an array",
                    (Lang::En, Structure::AnnotationsNotAnArray) => {
                        "the \"annotations\" field must be an array"
                    }
                    (Lang::En, Structure::AnnotationsMissing) => {
                        "the object has no \"annotations\" field"
                    }
                    (Lang::En, Structure::FigureNotAnObject) => {
                        "an array element must be an object"
                    }
                };
                match lang {
                    Lang::Ru => format!("неподдерживаемая структура файла: {reason}"),
                    Lang::En => format!("unsupported file structure: {reason}"),
                }
            }
            Self::Figure { index, problem } => {
                let reason = problem.message(lang);
                match lang {
                    Lang::Ru => format!("фигура №{}: {reason}", index + 1),
                    Lang::En => format!("figure #{}: {reason}", index + 1),
                }
            }
            Self::Io { path, source } => match lang {
                Lang::Ru => format!("не удалось прочитать {path}: {source}"),
                Lang::En => format!("could not read {path}: {source}"),
            },
        }
    }

    /// `true` when retrying might help, e.g. a temporary file lock.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Io {
                source: std::io::Error { .. },
                ..
            }
        )
    }
}

impl Problem {
    /// The sentence describing this problem, without the figure index.
    #[must_use]
    pub fn message(&self, lang: Lang) -> String {
        match (lang, self) {
            (Lang::Ru, Self::TypeMissing) => {
                "не указан тип фигуры (ожидается \"point\" или \"rect\")".to_owned()
            }
            (Lang::En, Self::TypeMissing) => {
                "the figure type is missing (expected \"point\" or \"rect\")".to_owned()
            }
            (Lang::Ru, Self::TypeUnknown { given }) => {
                format!("неизвестный тип фигуры \"{given}\" (ожидается \"point\" или \"rect\")")
            }
            (Lang::En, Self::TypeUnknown { given }) => {
                format!("unknown figure type \"{given}\" (expected \"point\" or \"rect\")")
            }
            (Lang::Ru, Self::MissingWidth) => {
                "у прямоугольника отсутствует поле \"width\"".to_owned()
            }
            (Lang::En, Self::MissingWidth) => "the rectangle has no \"width\" field".to_owned(),
            (Lang::Ru, Self::MissingHeight) => {
                "у прямоугольника отсутствует поле \"height\"".to_owned()
            }
            (Lang::En, Self::MissingHeight) => "the rectangle has no \"height\" field".to_owned(),
            (Lang::Ru, Self::NegativeSize { width, height }) => {
                format!("отрицательный размер прямоугольника ({width}, {height})")
            }
            (Lang::En, Self::NegativeSize { width, height }) => {
                format!("the rectangle has a negative size ({width}, {height})")
            }
            (Lang::Ru, Self::CoordinateNotFinite) => {
                "координата не является конечным числом".to_owned()
            }
            (Lang::En, Self::CoordinateNotFinite) => {
                "a coordinate is not a finite number".to_owned()
            }
        }
    }
}

/// Renders `{template}` with `{error}` replaced by `message`.
#[must_use]
pub fn with_error(template: &str, error: &Error, lang: Lang) -> String {
    let text = error.message(lang);
    fill(template, &[("error", &text)])
}

impl fmt::Display for Error {
    /// Falls back to Russian, the original language.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Lang::Ru))
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syntax { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::Structure { .. } | Self::Figure { .. } => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::syntax(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io {
            path: String::new(),
            source: value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_problem_is_readable_in_both_languages() {
        let problems = [
            Problem::TypeMissing,
            Problem::TypeUnknown {
                given: "polygon".to_owned(),
            },
            Problem::MissingWidth,
            Problem::MissingHeight,
            Problem::NegativeSize {
                width: -3.0,
                height: 4.0,
            },
            Problem::CoordinateNotFinite,
        ];
        for problem in problems {
            for lang in Lang::ALL {
                let text = problem.message(lang);
                assert!(text.chars().count() > 10, "{lang}: {problem:?} → {text}");
            }
            assert_ne!(
                problem.message(Lang::Ru),
                problem.message(Lang::En),
                "{problem:?}: переводы совпали"
            );
        }
    }

    #[test]
    fn problem_messages_quote_the_offending_value() {
        let problem = Problem::TypeUnknown {
            given: "poligon".to_owned(),
        };
        assert!(problem.message(Lang::Ru).contains("poligon"));
        assert!(problem.message(Lang::En).contains("poligon"));
    }

    #[test]
    fn figure_errors_are_numbered_from_one_for_humans() {
        let error = Error::Figure {
            index: 0,
            problem: Problem::MissingWidth,
        };
        assert!(error.message(Lang::Ru).starts_with("фигура №1"));
        assert!(error.message(Lang::En).starts_with("figure #1"));

        let error = Error::Figure {
            index: 41,
            problem: Problem::MissingHeight,
        };
        assert!(error.message(Lang::Ru).starts_with("фигура №42"));
        assert!(error.message(Lang::En).starts_with("figure #42"));
    }

    #[test]
    fn structure_problems_are_all_covered() {
        for problem in [
            Structure::NotAContainer,
            Structure::AnnotationsNotAnArray,
            Structure::AnnotationsMissing,
            Structure::FigureNotAnObject,
        ] {
            assert_ne!(
                Error::Structure { problem }.message(Lang::Ru),
                Error::Structure { problem }.message(Lang::En),
                "{problem:?}"
            );
        }
    }

    #[test]
    fn syntax_errors_keep_the_parser_position() {
        let source = serde_json::from_str::<serde_json::Value>("{\n  \"a\": }\n}").unwrap_err();
        let error = Error::syntax(source);
        let ru = error.message(Lang::Ru);
        assert!(ru.starts_with("некорректный JSON"), "{ru}");
        assert!(ru.contains("строка"), "{ru}");
        assert!(error.message(Lang::En).contains("invalid JSON"));
    }

    #[test]
    fn display_defaults_to_russian() {
        let error = Error::Figure {
            index: 2,
            problem: Problem::TypeMissing,
        };
        assert_eq!(error.to_string(), error.message(Lang::Ru));
    }

    #[test]
    fn with_error_substitutes_the_rendered_message() {
        let error = Error::Figure {
            index: 0,
            problem: Problem::MissingWidth,
        };
        let rendered = with_error("ошибка: {error}", &error, Lang::En);
        assert!(rendered.starts_with("ошибка: figure #1"), "{rendered}");
        assert!(rendered.contains("width"), "{rendered}");
    }

    #[test]
    fn io_errors_name_the_path() {
        let error = Error::Io {
            path: "/tmp/a.json".to_owned(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "nope"),
        };
        assert!(error.message(Lang::Ru).contains("/tmp/a.json"));
        assert!(error.message(Lang::En).contains("/tmp/a.json"));
        assert!(error.is_transient());
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn parsing_errors_have_no_source() {
        let error = Error::Structure {
            problem: Structure::NotAContainer,
        };
        assert!(std::error::Error::source(&error).is_none());
    }
}
