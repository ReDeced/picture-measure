//! Нативные диалоги выбора файлов.
//!
//! На Linux это **XDG Desktop Portal** (чистый Rust, без GTK и Qt), на Windows
//! и macOS — системные окна. Библиотека [`rfd`] прячет различия за одним API.
//!
//! Диалог блокирующий, а окно рисования — нет, поэтому он выполняется на
//! отдельном потоке, а [`Dialogs::take_result`] раз в кадре забирает результат без
//! блокировки. Сами запросы намеренно отделены от бэкенда через [`Opener`]:
//! это позволяет проверять всю обвязку в тестах без D-Bus и без портала.
//!
//! ```text
//! App                     поток диалогов
//! ───                     ───────────────
//! dialogs.request(…)  ──► ждёт выбора
//! …
//! dialogs.take_result() ◄── (Request, Result<Outcome, String>)
//! ```

use picture_measure::i18n::{Lang, Text};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Расширения картинок, которые программа умеет открыть.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

/// Что диалог должен предложить выбрать.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Открыть изображение.
    Image {
        /// Каталог, с которого открыть диалог.
        directory: Option<PathBuf>,
    },
    /// Открыть файл разметки.
    Annotations {
        /// Каталог, с которого открыть диалог.
        directory: Option<PathBuf>,
    },
    /// Сохранить разметку.
    SaveAnnotations {
        /// Путь, предложенный пользователю.
        suggested: Option<PathBuf>,
    },
    /// Выбрать каталог для раздельного экспорта.
    Directory {
        /// Каталог, с которого открыть диалог.
        directory: Option<PathBuf>,
    },
}

impl Request {
    /// Заголовок окна диалога.
    #[must_use]
    pub fn title(&self, lang: Lang) -> &'static str {
        let t = Text::get(lang);
        match self {
            Self::Image { .. } => t.dialog_open_image,
            Self::Annotations { .. } => t.dialog_open_annotations,
            Self::SaveAnnotations { .. } => t.dialog_save_annotations,
            Self::Directory { .. } => t.dialog_pick_directory,
        }
    }

    /// Фильтры файлов: отображаемое имя и расширения.
    ///
    /// Метки намеренно не переводятся: их показывает файловый менеджер
    /// рабочего стола, а не программа, и у пользователя должен быть язык
    /// системы.
    #[must_use]
    pub fn filters(&self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Self::Image { .. } => &[(Text::LABEL_IMAGES, IMAGE_EXTENSIONS)],
            Self::Annotations { .. } | Self::SaveAnnotations { .. } => {
                &[(Text::LABEL_JSON, &["json"])]
            }
            Self::Directory { .. } => &[],
        }
    }

    /// Каталог, с которого логично открыть диалог.
    ///
    /// Для сохранения это родительский каталог предложенного пути, иначе
    /// пользователь увидит диалог не в том месте.
    #[must_use]
    pub fn start_directory(&self) -> Option<PathBuf> {
        match self {
            Self::Image { directory }
            | Self::Annotations { directory }
            | Self::Directory { directory } => directory.clone(),
            Self::SaveAnnotations { suggested } => suggested.as_deref().and_then(parent_of),
        }
    }

    /// Имя файла, предложенное пользователю.
    #[must_use]
    pub fn suggested_name(&self) -> Option<String> {
        match self {
            Self::SaveAnnotations { suggested } => suggested
                .as_deref()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned()),
            Self::Image { .. } | Self::Annotations { .. } | Self::Directory { .. } => None,
        }
    }
}

/// Результат диалога.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Пользователь выбрал путь.
    Chosen(PathBuf),
    /// Пользователь закрыл диалог, ничего не выбрав.
    Cancelled,
}

impl Outcome {
    /// Выбранный путь, если он есть.
    #[must_use]
    pub fn path(self) -> Option<PathBuf> {
        match self {
            Self::Chosen(path) => Some(path),
            Self::Cancelled => None,
        }
    }
}

/// Умеет показывать диалог. Реальная реализация — [`NativeOpener`].
pub trait Opener: Send + 'static {
    /// Показывает диалог и ждёт ответа пользователя.
    ///
    /// `Err` означает, что диалог показать не удалось: нет D-Bus, нет портала,
    /// нет файлового менеджера. Отмена — это [`Outcome::Cancelled`], а не
    /// ошибка.
    fn show(&mut self, request: Request, lang: Lang) -> Result<Outcome, String>;
}

/// Диалоги через [`rfd`]: XDG-портал на Linux, системные окна на Windows и
/// macOS.
#[derive(Debug, Default)]
pub struct NativeOpener;

impl NativeOpener {
    /// Создаёт бэкенд.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Собирает диалог под конкретный запрос.
    fn build(request: &Request, lang: Lang) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new().set_title(request.title(lang));
        for (name, extensions) in request.filters() {
            dialog = dialog.add_filter(*name, extensions);
        }
        if let Some(directory) = request.start_directory()
            && directory.is_dir()
        {
            dialog = dialog.set_directory(directory);
        }
        if let Some(name) = request.suggested_name() {
            dialog = dialog.set_file_name(name);
        }
        dialog
    }
}

impl Opener for NativeOpener {
    fn show(&mut self, request: Request, lang: Lang) -> Result<Outcome, String> {
        let dialog = Self::build(&request, lang);
        let chosen = match request {
            Request::Image { .. } | Request::Annotations { .. } => dialog.pick_file(),
            Request::SaveAnnotations { .. } => dialog.save_file(),
            Request::Directory { .. } => dialog.pick_folder(),
        };
        Ok(match chosen {
            Some(path) => Outcome::Chosen(path),
            None => Outcome::Cancelled,
        })
    }
}

/// Диалоговые окна на отдельном потоке.
///
/// Держит не больше одного открытого диалога: пока пользователь выбирает файл,
/// второй запрос отвергается, иначе окна начали бы накладываться.
///
/// Язык кладётся в запрос, а не хранится на потоке: иначе смена языка в
/// открытом приложении не изменила бы заголовок следующего диалога.
#[derive(Debug)]
pub struct Dialogs {
    send: Sender<(u64, Lang, Request)>,
    receive: Receiver<(u64, Result<Outcome, String>)>,
    next_id: u64,
    /// Открытый диалог: его номер и что именно спрашивали.
    pending: Option<(u64, Request)>,
    rejected: bool,
    language: Lang,
}

impl Dialogs {
    /// Запускает поток диалогов с системным бэкендом.
    #[must_use]
    pub fn native(lang: Lang) -> Self {
        Self::with_opener(Box::new(NativeOpener::new()), lang)
    }

    /// Запускает поток диалогов с произвольным бэкендом. Для тестов.
    #[must_use]
    pub fn with_opener(mut opener: Box<dyn Opener>, lang: Lang) -> Self {
        let (send, requests) = channel::<(u64, Lang, Request)>();
        let (out, receive) = channel::<(u64, Result<Outcome, String>)>();
        std::thread::Builder::new()
            .name("picture-measure-dialogs".to_owned())
            .spawn(move || {
                for (id, lang, request) in requests {
                    let result = opener.show(request, lang);
                    if out.send((id, result)).is_err() {
                        break;
                    }
                }
            })
            .expect("не удалось запустить поток диалогов");
        Self {
            send,
            receive,
            next_id: 1,
            pending: None,
            rejected: false,
            language: lang,
        }
    }

    /// Меняет язык заголовков следующих диалогов.
    ///
    /// Уже открытый диалог сохранит прежний заголовок: переименовать системное
    /// окно нельзя.
    pub fn set_language(&mut self, lang: Lang) {
        self.language = lang;
    }

    /// Просит показать диалог.
    ///
    /// Возвращает `false`, если уже открыт другой диалог: повторите позже.
    pub fn request(&mut self, request: Request) -> bool {
        if self.pending.is_some() {
            self.rejected = true;
            return false;
        }
        let id = self.next_id;
        self.next_id += 1;
        if self
            .send
            .send((id, self.language, request.clone()))
            .is_err()
        {
            return false;
        }
        self.pending = Some((id, request));
        true
    }

    /// Открытый сейчас диалог, если он есть.
    #[cfg(test)]
    #[must_use]
    pub fn pending_request(&self) -> Option<&Request> {
        self.pending.as_ref().map(|(_, request)| request)
    }

    /// `true`, пока диалог открыт и ждёт ответа.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }

    /// `true`, если последний запрос отвергнут из-за занятого диалога.
    #[must_use]
    pub fn was_rejected(&self) -> bool {
        self.rejected
    }

    /// Сбрасывает признак отвергнутого запроса.
    pub fn clear_rejected(&mut self) {
        self.rejected = false;
    }

    /// Забирает любой ответ, не блокируя. Используется в тестах.
    pub fn poll(&mut self) -> Option<(u64, Result<Outcome, String>)> {
        self.receive.try_recv().ok()
    }

    /// Забирает ответ на текущий запрос, если он уже пришёл.
    ///
    /// Возвращает вместе с ответом и сам запрос: выбранный путь один и тот же
    /// для картинки, разметки и папки, и что с ним делать, решает вид диалога,
    /// а не вид пути.
    ///
    /// Ответы на отменённые диалоги молча выбрасываются.
    pub fn take_result(&mut self) -> Option<(Request, Result<Outcome, String>)> {
        // Забирать `pending` можно только вместе с ответом. Если сделать это
        // раньше, диалог, ответ на который придёт через пару кадров, потеряется
        // навсегда: поля уже нет, а ответ придёт некуда.
        loop {
            let id = self.pending.as_ref()?.0;
            let (got, result) = self.poll()?;
            if got == id {
                let (_, request) = self.pending.take()?;
                return Some((request, result));
            }
        }
    }
}

/// Бэкенд для тестов: отвечает заранее заданным результатом и не показывает
/// ничего настоящего.
#[cfg(test)]
pub(crate) enum Canned {
    /// Пользователь закрыл диалог.
    Cancelled,
    /// Пользователь выбрал этот путь.
    Chosen(PathBuf),
    /// Диалог показать не удалось.
    Failed(String),
}

#[cfg(test)]
impl Opener for Canned {
    fn show(&mut self, _request: Request, _lang: Lang) -> Result<Outcome, String> {
        match self {
            Self::Cancelled => Ok(Outcome::Cancelled),
            Self::Chosen(path) => Ok(Outcome::Chosen(path.clone())),
            Self::Failed(error) => Err(error.clone()),
        }
    }
}

/// Каталог рядом с уже открытым файлом — откуда открывать диалоги.
#[must_use]
pub fn directory_of(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(parent_of)
}

/// Родительский каталог пути.
///
/// У пустого имени файла нет родителя, и открывать диалог «в никуда» незачем:
/// в этом случае лучше показать домашний каталог, как и делает портал.
fn parent_of(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_translated() {
        for request in [
            Request::Image { directory: None },
            Request::Annotations { directory: None },
            Request::SaveAnnotations { suggested: None },
            Request::Directory { directory: None },
        ] {
            let ru = request.title(Lang::Ru);
            let en = request.title(Lang::En);
            assert!(!ru.is_empty());
            assert_ne!(ru, en, "{request:?}: заголовок не переведён");
        }
    }

    #[test]
    fn image_dialog_offers_exactly_what_we_can_open() {
        let request = Request::Image { directory: None };
        assert_eq!(request.filters().len(), 1);
        let (_, extensions) = request.filters()[0];
        assert_eq!(extensions, IMAGE_EXTENSIONS);
        // Расширения фильтра обязаны совпадать с тем, что принимает загрузчик:
        // иначе диалог предложит файл, который программа не откроет.
        for extension in IMAGE_EXTENSIONS {
            let name = format!("a.{extension}");
            assert!(
                picture_measure::json_io::is_image_path(Path::new(&name)),
                "фильтр предлагает {name}, а программа её не откроет"
            );
        }
        for extension in ["png", "jpg", "jpeg", "webp", "bmp"] {
            assert!(
                IMAGE_EXTENSIONS.contains(&extension),
                "{extension} поддерживается, но отсутствует в фильтре"
            );
        }
    }

    #[test]
    fn json_dialogs_filter_json_only() {
        for request in [
            Request::Annotations { directory: None },
            Request::SaveAnnotations { suggested: None },
        ] {
            let (_, extensions) = request.filters()[0];
            assert_eq!(extensions, ["json"], "{request:?}");
        }
    }

    #[test]
    fn directory_dialog_has_no_filters() {
        let request = Request::Directory { directory: None };
        assert!(
            request.filters().is_empty(),
            "у выбора папки фильтр не нужен"
        );
        assert!(request.suggested_name().is_none());
    }

    #[test]
    fn save_dialog_starts_beside_the_current_file() {
        let request = Request::SaveAnnotations {
            suggested: Some(PathBuf::from("/tmp/pics/cat.json")),
        };
        assert_eq!(
            request.start_directory(),
            Some(PathBuf::from("/tmp/pics")),
            "диалог должен открыться в каталоге файла"
        );
        assert_eq!(request.suggested_name().as_deref(), Some("cat.json"));
    }

    #[test]
    fn save_dialog_with_a_bare_name_has_no_directory() {
        let request = Request::SaveAnnotations {
            suggested: Some(PathBuf::from("cat.json")),
        };
        assert_eq!(request.start_directory(), None);
        assert_eq!(request.suggested_name().as_deref(), Some("cat.json"));
    }

    #[test]
    fn open_dialogs_use_the_given_directory() {
        let directory = Some(PathBuf::from("/home/user/pics"));
        for request in [
            Request::Image {
                directory: directory.clone(),
            },
            Request::Annotations {
                directory: directory.clone(),
            },
            Request::Directory {
                directory: directory.clone(),
            },
        ] {
            assert_eq!(request.start_directory(), directory, "{request:?}");
            assert!(request.suggested_name().is_none(), "{request:?}");
        }
    }

    #[test]
    fn outcome_exposes_the_path() {
        assert_eq!(
            Outcome::Chosen(PathBuf::from("/tmp/a.png")).path(),
            Some(PathBuf::from("/tmp/a.png"))
        );
        assert_eq!(Outcome::Cancelled.path(), None);
    }

    #[test]
    fn directory_of_ignores_bare_names() {
        assert_eq!(directory_of(None), None);
        assert_eq!(directory_of(Some(Path::new("a.png"))), None);
        assert_eq!(
            directory_of(Some(Path::new("/tmp/a.png"))),
            Some(PathBuf::from("/tmp"))
        );
    }

    /// Ответ заранее заданного сценария.
    struct Fake {
        reply: Box<dyn Fn(Request, Lang) -> Result<Outcome, String> + Send>,
    }

    impl Opener for Fake {
        fn show(&mut self, request: Request, lang: Lang) -> Result<Outcome, String> {
            (self.reply)(request, lang)
        }
    }

    /// Запоминает запросы и отвечает с задержкой.
    struct Recorder {
        delay: std::time::Duration,
        seen: Sender<Request>,
        reply: fn(Request) -> Result<Outcome, String>,
    }

    impl Opener for Recorder {
        fn show(&mut self, request: Request, _lang: Lang) -> Result<Outcome, String> {
            std::thread::sleep(self.delay);
            let _ = self.seen.send(request.clone());
            (self.reply)(request)
        }
    }

    /// Ждёт ответ диалога, имитируя ход кадров приложения.
    fn await_result(dialogs: &mut Dialogs) -> Option<(Request, Result<Outcome, String>)> {
        for _ in 0..400 {
            if let Some(result) = dialogs.take_result() {
                return Some(result);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        None
    }

    fn always(path: &str) -> impl Fn(Request, Lang) -> Result<Outcome, String> + Send {
        let path = PathBuf::from(path);
        move |_, _| Ok(Outcome::Chosen(path.clone()))
    }

    #[test]
    fn a_picked_path_arrives_on_a_later_frame() {
        let mut dialogs = Dialogs::with_opener(
            Box::new(Fake {
                reply: Box::new(always("/tmp/picked.png")),
            }),
            Lang::Ru,
        );
        assert!(!dialogs.is_open());
        assert!(dialogs.request(Request::Image { directory: None }));
        assert!(dialogs.is_open());

        assert_eq!(
            await_result(&mut dialogs)
                .and_then(|(_, result)| result.ok())
                .and_then(Outcome::path),
            Some(PathBuf::from("/tmp/picked.png"))
        );
        assert!(!dialogs.is_open(), "диалог закрылся");
    }

    #[test]
    fn an_answer_arriving_later_is_still_picked_up() {
        let (seen, _received) = channel();
        let mut dialogs = Dialogs::with_opener(
            Box::new(Recorder {
                delay: std::time::Duration::from_millis(250),
                seen,
                reply: |_| Ok(Outcome::Chosen(PathBuf::from("/tmp/late.png"))),
            }),
            Lang::Ru,
        );
        assert!(dialogs.request(Request::Image { directory: None }));
        // Пустые кадры до того, как пользователь нажал «Открыть».
        for _ in 0..20 {
            assert!(dialogs.take_result().is_none(), "ответ ещё не готов");
            assert!(dialogs.is_open(), "диалог всё ещё открыт");
        }
        assert_eq!(
            await_result(&mut dialogs)
                .and_then(|(_, result)| result.ok())
                .and_then(Outcome::path),
            Some(PathBuf::from("/tmp/late.png")),
            "ответ, пришедший не в первый кадр, не должен теряться"
        );
        assert!(!dialogs.is_open());
    }

    #[test]
    fn cancelling_is_not_an_error() {
        let mut dialogs = Dialogs::with_opener(
            Box::new(Fake {
                reply: Box::new(|_, _| Ok(Outcome::Cancelled)),
            }),
            Lang::Ru,
        );
        dialogs.request(Request::Image { directory: None });
        let outcome = await_result(&mut dialogs)
            .map(|(_, result)| result)
            .expect("отмена не должна быть ошибкой")
            .expect("отмена не должна быть ошибкой");
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(outcome.path().is_none());
    }

    #[test]
    fn a_broken_backend_reports_an_error_and_does_not_wedge() {
        let mut dialogs = Dialogs::with_opener(
            Box::new(Fake {
                reply: Box::new(|_, _| Err("нет D-Bus".to_owned())),
            }),
            Lang::Ru,
        );
        dialogs.request(Request::Image { directory: None });
        let error = await_result(&mut dialogs)
            .map(|(_, result)| result)
            .expect("ответ не пришёл")
            .expect_err("ожидалась ошибка");
        assert!(error.contains("D-Bus"), "{error}");
        // Ошибка не должна оставлять диалог «открытым» навечно.
        assert!(!dialogs.is_open());
        assert!(dialogs.request(Request::Image { directory: None }));
    }

    #[test]
    fn only_one_dialog_at_a_time() {
        let (seen, _received) = channel();
        let mut dialogs = Dialogs::with_opener(
            Box::new(Recorder {
                delay: std::time::Duration::from_millis(400),
                seen,
                reply: |_| Ok(Outcome::Cancelled),
            }),
            Lang::Ru,
        );

        assert!(dialogs.request(Request::Image { directory: None }));
        assert!(!dialogs.request(Request::SaveAnnotations { suggested: None }));
        assert!(
            dialogs.was_rejected(),
            "второй запрос должен быть отвергнут"
        );

        assert!(await_result(&mut dialogs).is_some());
        dialogs.clear_rejected();
        assert!(!dialogs.was_rejected());
        assert!(dialogs.request(Request::Directory { directory: None }));
    }

    #[test]
    fn the_request_reaches_the_backend_unchanged() {
        let (seen, received) = channel();
        let mut dialogs = Dialogs::with_opener(
            Box::new(Recorder {
                delay: std::time::Duration::from_millis(5),
                seen,
                reply: |_| Ok(Outcome::Cancelled),
            }),
            Lang::Ru,
        );
        let request = Request::SaveAnnotations {
            suggested: Some(PathBuf::from("/tmp/pics/cat.json")),
        };
        dialogs.request(request.clone());
        assert_eq!(
            dialogs.pending_request(),
            Some(&request),
            "приложение должно знать, о чём спрашивало"
        );
        assert_eq!(received.recv().expect("бэкенд не вызван"), request);
        let (returned, result) = await_result(&mut dialogs).expect("ответ не пришёл");
        assert_eq!(returned, request, "ответ приходит вместе с запросом");
        assert_eq!(result.expect("отмена не ошибка"), Outcome::Cancelled);
        assert_eq!(dialogs.pending_request(), None);
    }

    #[test]
    fn the_backend_sees_the_current_language() {
        let mut dialogs = Dialogs::with_opener(
            Box::new(Fake {
                reply: Box::new(|_, lang| Ok(Outcome::Chosen(PathBuf::from(lang.code())))),
            }),
            Lang::Ru,
        );
        for want in [Lang::En, Lang::Ru, Lang::En] {
            dialogs.set_language(want);
            dialogs.request(Request::Image { directory: None });
            let got = await_result(&mut dialogs)
                .and_then(|(_, result)| result.ok())
                .and_then(Outcome::path)
                .expect("нет пути");
            assert_eq!(got, PathBuf::from(want.code()));
        }
    }

    #[test]
    fn polling_an_idle_dialog_is_harmless() {
        let mut dialogs = Dialogs::with_opener(
            Box::new(Fake {
                reply: Box::new(|_, _| Ok(Outcome::Cancelled)),
            }),
            Lang::Ru,
        );
        assert!(dialogs.take_result().is_none());
        assert!(dialogs.poll().is_none());
        assert!(!dialogs.is_open());
    }
}
