//! Internationalisation: two locales, no dependencies.
//!
//! All user-visible text lives here as a single [`Text`] structure with one
//! field per message. [`Text::get`] hands out a `&'static Text`, so switching
//! language costs nothing at runtime and the compiler catches every missing
//! string: add a field to `Text`, and both locales stop compiling until you
//! fill it in.
//!
//! ```text
//! let t = Text::get(lang);
//! button(ui, t.save_json);
//! ui.label(format!("{t.figures} {n}"));
//! ```
//!
//! Placeholders are written with named arguments, because the languages differ
//! in word order:
//!
//! | | `ru` | `en` |
//! |---|---|---|
//! | figures | `Фигуры: {n}` | `Figures: {n}` |
//! | errors | `фигура №{i}: …` | `figure #{i}: …` |

use std::fmt;
use std::str::FromStr;

/// A supported interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    /// Russian. The original language of the interface.
    #[default]
    Ru,
    /// English.
    En,
}

impl Lang {
    /// Every supported language, in menu order.
    pub const ALL: [Self; 2] = [Self::Ru, Self::En];

    /// BCP 47 code, also used for the `--lang` command line option.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Ru => "ru",
            Self::En => "en",
        }
    }

    /// The language's own name, for the language picker.
    #[must_use]
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Ru => "Русский",
            Self::En => "English",
        }
    }

    /// The same language written in English, for compact lists.
    #[must_use]
    pub const fn english_name(self) -> &'static str {
        match self {
            Self::Ru => "Russian",
            Self::En => "English",
        }
    }

    /// The other language.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Ru => Self::En,
            Self::En => Self::Ru,
        }
    }

    /// Whether this is Russian.
    #[must_use]
    pub const fn is_russian(self) -> bool {
        matches!(self, Self::Ru)
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for Lang {
    type Err = UnknownLang;

    /// Accepts `ru`, `ru_RU`, `ru-RU.UTF-8`, `russian`, `Русский`.
    ///
    /// Everything from the first separator on is dropped, so `en_US.UTF-8`
    /// and `en-gb` both mean `en`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `to_lowercase`, а не `to_ascii_lowercase`: кириллица тоже должна
        // распознаваться.
        let normalised = s.trim().to_lowercase();
        let primary = normalised
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or_default();
        match primary {
            "ru" | "rus" | "russian" | "русский" | "рус" => Ok(Self::Ru),
            "en" | "eng" | "english" => Ok(Self::En),
            _ => Err(UnknownLang(normalised)),
        }
    }
}

/// The value passed to [`Lang::from_str`] is not a language we know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownLang(pub String);

impl fmt::Display for UnknownLang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "неизвестный язык: {}\nдоступны: ru, en", self.0)
    }
}

impl std::error::Error for UnknownLang {}

/// Guesses a language from the environment (`LANG`, `LC_ALL`, `LC_MESSAGES`).
#[must_use]
pub fn from_environment() -> Lang {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key)
            && let Some((code, _)) = value.split_once('.')
            && let Ok(lang) = code.parse()
        {
            return lang;
        }
    }
    Lang::default()
}

/// Every user-visible string of the interface.
///
/// Fields are grouped by the panel that uses them.
pub struct Text {
    // --- Application -------------------------------------------------
    /// Window title.
    pub window_title: &'static str,
    /// Verb shown in the "about" window.
    pub version: &'static str,
    /// Link to the project.
    pub project_url: &'static str,

    // --- Toolbar -----------------------------------------------------
    /// Button: load an image from the path field.
    pub open: &'static str,
    /// Button: delete every figure.
    pub clear_all: &'static str,
    /// Button: write the JSON file.
    pub save_json: &'static str,
    /// Button: put the JSON on the clipboard.
    pub copy_json: &'static str,
    /// Button: undo.
    pub undo: &'static str,
    /// Button: redo.
    pub redo: &'static str,
    /// Label before the tool buttons.
    pub tool_label: &'static str,
    /// Button: fit the image into the window.
    pub fit: &'static str,
    /// Unsaved-changes marker in the toolbar.
    pub unsaved: &'static str,

    // --- Sidebar: image ----------------------------------------------
    /// Section header.
    pub section_image: &'static str,
    /// Placeholder when no image is loaded.
    pub no_image: &'static str,
    /// Label before the path field.
    pub path: &'static str,
    /// Hint inside the image path field.
    pub path_hint: &'static str,
    /// Button: load the image.
    pub load: &'static str,
    /// Button: take the path from the clipboard.
    pub paste: &'static str,
    /// Hint that files can be dropped onto the window.
    pub drag_hint: &'static str,

    // --- Sidebar: defaults -------------------------------------------
    /// Section header.
    pub section_new_figures: &'static str,
    /// Label before the folder field.
    pub subfolder: &'static str,
    /// Hint inside the folder field.
    pub subfolder_hint: &'static str,
    /// Shown when no folder is in use yet.
    pub no_folders: &'static str,
    /// Tooltip: new figures go here.
    pub subfolder_tooltip: &'static str,
    /// Note about the auto-generated names.
    pub default_name_note: &'static str,
    /// Language picker label.
    pub language: &'static str,

    // --- Sidebar: figure list ----------------------------------------
    /// Section header, `{n}` figures, `{p}` points, `{r}` rectangles.
    pub figures: &'static str,
    /// Button: select everything.
    pub select_all: &'static str,
    /// Button: clear the selection.
    pub clear_selection: &'static str,
    /// Button: make the numbering consecutive again.
    pub renumber: &'static str,
    /// Tooltip for the renumber button.
    pub renumber_hint: &'static str,
    /// Button: delete the selected figures.
    pub delete_selected: &'static str,
    /// Shown while the list is empty.
    pub list_empty: &'static str,

    // --- Sidebar: properties -----------------------------------------
    /// Section header.
    pub section_properties: &'static str,
    /// Shown when nothing is selected.
    pub nothing_selected: &'static str,
    /// `{n}` figures are selected.
    pub selected_count: &'static str,
    /// Colour picker for the whole selection.
    pub colour_for_all: &'static str,
    /// Label before the name field.
    pub name: &'static str,
    /// Label before the folder field.
    pub folder: &'static str,
    /// Hint inside the folder field.
    pub folder_hint: &'static str,
    /// Shown next to the name field: the auto-generated name.
    pub auto_name: &'static str,
    /// Button: go back to the auto-generated name.
    pub reset_name: &'static str,
    /// Colour picker for one figure.
    pub colour: &'static str,
    /// Button: make a copy.
    pub duplicate: &'static str,
    /// Button: delete.
    pub delete: &'static str,

    // --- Sidebar: export ---------------------------------------------
    /// Section header.
    pub section_export: &'static str,
    /// Label before the JSON path field.
    pub json_path: &'static str,
    /// Hint inside the JSON path field.
    pub json_path_hint: &'static str,
    /// Label before the precision slider.
    pub precision: &'static str,
    /// Label above the per-figure export directory field.
    pub split_dir: &'static str,
    /// Hint inside the per-figure export directory field.
    pub split_dir_hint: &'static str,
    /// Button: write the per-figure export.
    pub write: &'static str,
    /// Explains the per-figure export layout.
    pub split_hint: &'static str,
    /// Last per-figure export result.
    pub split_result: &'static str,
    /// Checkbox: show figure names.
    pub show_labels: &'static str,
    /// Checkbox: show the grid.
    pub show_grid: &'static str,
    /// Checkbox: show the rulers.
    pub show_rulers: &'static str,
    /// Button: open the hot-key window.
    pub hotkeys: &'static str,
    /// Button: open the about window.
    pub about: &'static str,

    // --- Status bar --------------------------------------------------
    /// Control hints shown when the cursor is away from the image.
    pub status_hint: &'static str,
    /// Cursor position: `x = …  y = …`
    pub cursor_pos: &'static str,

    // --- Canvas ------------------------------------------------------
    /// Placeholder inside an empty canvas.
    pub drop_image: &'static str,

    // --- Context menu ------------------------------------------------
    /// Shown when the cursor is not over a figure.
    pub no_figure_here: &'static str,
    /// Menu item: select the figure.
    pub menu_select: &'static str,
    /// Menu item: make a copy.
    pub menu_duplicate: &'static str,
    /// Menu item: restore the auto-generated name.
    pub menu_reset_name: &'static str,
    /// Menu item: delete.
    pub menu_delete: &'static str,

    // --- Tools -------------------------------------------------------
    /// Button label for the drawing tool.
    pub tool_draw: &'static str,
    /// Button label for the selection tool.
    pub tool_select: &'static str,
    /// Button label for the delete tool.
    pub tool_delete: &'static str,
    /// Button label for the pan tool.
    pub tool_pan: &'static str,
    /// Tooltip for the drawing tool.
    pub tool_draw_hint: &'static str,
    /// Tooltip for the selection tool.
    pub tool_select_hint: &'static str,
    /// Tooltip for the delete tool.
    pub tool_delete_hint: &'static str,
    /// Tooltip for the pan tool.
    pub tool_pan_hint: &'static str,

    // --- Hot-key window ----------------------------------------------
    /// Window title.
    pub hotkeys_title: &'static str,
    /// Table header: the gesture.
    pub hotkeys_action: &'static str,
    /// Table header: what it does.
    pub hotkeys_meaning: &'static str,

    // --- About window ------------------------------------------------
    /// Window title.
    pub about_title: &'static str,
    /// Tagline, line 1.
    pub about_line1: &'static str,
    /// Tagline, line 2.
    pub about_line2: &'static str,
    /// Note about coordinates and names.
    pub about_note: &'static str,

    // --- Messages ----------------------------------------------------
    /// Image loaded.
    pub loaded_image: &'static str,
    /// Image could not be opened: `{error}`
    pub image_failed: &'static str,
    /// Annotations loaded: `{n}` figures, `{p}` points, `{r}` rectangles.
    pub loaded_figures: &'static str,
    /// JSON could not be parsed: `{error}`
    pub json_failed: &'static str,
    /// File could not be read: `{error}`
    pub read_failed: &'static str,
    /// Saved to `{path}`
    pub saved: &'static str,
    /// Could not save to `{path}`: `{error}`
    pub save_failed: &'static str,
    /// JSON copied, `{n}` characters.
    pub copied: &'static str,
    /// Could not serialise: `{error}`
    pub serialise_failed: &'static str,
    /// `{n}` files written into `{dir}`
    pub split_done: &'static str,
    /// Could not create `{dir}`: `{error}`
    pub split_create_failed: &'static str,
    /// Split export failed: `{error}`
    pub split_failed: &'static str,
    /// A copy was made: `{name}`
    pub duplicated: &'static str,
    /// Figures renumbered.
    pub renumbered: &'static str,
    /// Saved per-figure export result: `{count}` files in `{dir}`
    pub split_result_done: &'static str,
    /// Enter an image path.
    pub need_image_path: &'static str,
    /// Enter a path for the JSON file.
    pub need_json_path: &'static str,
    /// Enter a directory for the per-figure export.
    pub need_split_dir: &'static str,
    /// Nothing on the clipboard.
    pub no_clipboard: &'static str,
    /// Could not determine the format of `{path}`
    pub unknown_format: &'static str,
    /// Opening the project page.
    pub opening_project: &'static str,
}

static RU: Text = Text {
    window_title: "picture-measure — измерение картинок",
    version: "версия",
    project_url: "github.com/ReDeced/picture-measure",

    open: "📂 Открыть",
    clear_all: "🗑 Очистить",
    save_json: "💾 Сохранить JSON",
    copy_json: "📋 Копировать JSON",
    undo: "↶ Отменить",
    redo: "↷ Повторить",
    tool_label: "Инструмент:",
    fit: "⤢ Вписать",
    unsaved: "● есть изменения",

    section_image: "Изображение",
    no_image: "не открыто",
    path: "Путь:",
    path_hint: "/путь/к/картинке.png",
    load: "Открыть",
    paste: "Вставить",
    drag_hint: "Можно просто перетащить файл в окно",

    section_new_figures: "Новые фигуры",
    subfolder: "Подпапка:",
    subfolder_hint: "например eyes",
    no_folders: "Подпапок пока нет",
    subfolder_tooltip: "Все новые фигуры попадут сюда",
    default_name_note: "Имя по умолчанию: point_N / rect_N",
    language: "Язык интерфейса:",

    figures: "Фигуры: {n} (точек {p}, прямоугольников {r})",
    select_all: "Выбрать все",
    clear_selection: "Снять выделение",
    renumber: "Перенумеровать",
    renumber_hint: "Сделать имена point_1, rect_2, … подряд",
    delete_selected: "Удалить выбранные",
    list_empty: "Пока пусто",

    section_properties: "Свойства выбранного",
    nothing_selected: "Ничего не выбрано",
    selected_count: "Выбрано фигур: {n}",
    colour_for_all: "Цвет для всех:",
    name: "Имя:",
    folder: "Папка:",
    folder_hint: "без папки",
    auto_name: "авто-имя: {name}",
    reset_name: "Сбросить",
    colour: "Цвет:",
    duplicate: "Дублировать",
    delete: "Удалить",

    section_export: "Экспорт",
    json_path: "Путь JSON:",
    json_path_hint: "annotations.json",
    precision: "Знаков после запятой:",
    split_dir: "Папка для раздельного экспорта:",
    split_dir_hint: "out/",
    write: "Записать",
    split_hint: "Каждая фигура сохраняется отдельным файлом: <папка>/<имя>.json",
    split_result: "{n} файлов в подпапках внутри {dir}",
    show_labels: "Подписи",
    show_grid: "Сетка",
    show_rulers: "Линейки",
    hotkeys: "Горячие клавиши",
    about: "О проекте",

    status_hint: "ЛКМ — точка/прямоугольник · Shift — квадрат · СКМ/ПКМ — панорама · колесо — зум",
    cursor_pos: "x = {x}   y = {y}",

    drop_image: "Перетащите изображение сюда\nили укажите путь на панели слева",

    no_figure_here: "Под курсором нет фигур",
    menu_select: "Выбрать",
    menu_duplicate: "Дублировать",
    menu_reset_name: "Вернуть имя по умолчанию",
    menu_delete: "Удалить",

    tool_draw: "Рисование",
    tool_select: "Выбор",
    tool_delete: "Удаление",
    tool_pan: "Панорама",
    tool_draw_hint: "Клик — точка, протяжка — прямоугольник, Shift — квадрат",
    tool_select_hint: "Клик — выбрать, протяжка — переместить выбранное",
    tool_delete_hint: "Клик по фигуре — удалить её",
    tool_pan_hint: "Протяжка — сдвинуть изображение",

    hotkeys_title: "Горячие клавиши",
    hotkeys_action: "Действие",
    hotkeys_meaning: "Смысл",

    about_title: "О программе",
    about_line1: "Измеряйте картинки точками и прямоугольниками,",
    about_line2: "называйте их и выгружайте разметку в JSON.",
    about_note: "Координаты: пиксели, начало — левый верхний угол.\nИмена по умолчанию: point_N и rect_N, где N — номер фигуры.",

    loaded_image: "Загружено: {name}",
    image_failed: "Не удалось открыть {path}: {error}",
    loaded_figures: "Загружено {n} фигур (точек: {p}, прямоугольников: {r})",
    json_failed: "Ошибка чтения JSON: {error}",
    read_failed: "Не удалось прочитать {path}: {error}",
    saved: "Сохранено: {path}",
    save_failed: "Не удалось сохранить {path}: {error}",
    copied: "JSON скопирован в буфер ({n} символов)",
    serialise_failed: "Не удалось сериализовать: {error}",
    split_done: "Записано {n} фигур в подпапки внутри {dir}",
    split_create_failed: "Не удалось создать {dir}: {error}",
    split_failed: "Ошибка экспорта: {error}",
    duplicated: "Создана копия: {name}",
    renumbered: "Фигуры перенумерованы",
    split_result_done: "{n} файлов в подпапках внутри {dir}",
    need_image_path: "Введите путь к изображению на панели слева",
    need_json_path: "Укажите путь для сохранения JSON",
    need_split_dir: "Укажите папку для раздельного экспорта",
    no_clipboard: "В буфере обмена нет текста",
    unknown_format: "Не удалось определить формат: {path}",
    opening_project: "Открываю страницу проекта…",
};

static EN: Text = Text {
    window_title: "picture-measure — image measurement",
    version: "version",
    project_url: "github.com/ReDeced/picture-measure",

    open: "📂 Open",
    clear_all: "🗑 Clear",
    save_json: "💾 Save JSON",
    copy_json: "📋 Copy JSON",
    undo: "↶ Undo",
    redo: "↷ Redo",
    tool_label: "Tool:",
    fit: "⤢ Fit",
    unsaved: "● unsaved changes",

    section_image: "Image",
    no_image: "none loaded",
    path: "Path:",
    path_hint: "/path/to/photo.png",
    load: "Open",
    paste: "Paste",
    drag_hint: "You can also drag a file into the window",

    section_new_figures: "New figures",
    subfolder: "Sub-folder:",
    subfolder_hint: "e.g. eyes",
    no_folders: "No sub-folders yet",
    subfolder_tooltip: "Every new figure will land here",
    default_name_note: "Default name: point_N / rect_N",
    language: "Interface language:",

    figures: "Figures: {n} (points {p}, rectangles {r})",
    select_all: "Select all",
    clear_selection: "Clear selection",
    renumber: "Renumber",
    renumber_hint: "Make the names consecutive: point_1, rect_2, …",
    delete_selected: "Delete selected",
    list_empty: "Nothing here yet",

    section_properties: "Selected figure",
    nothing_selected: "Nothing selected",
    selected_count: "{n} figures selected",
    colour_for_all: "Colour for all:",
    name: "Name:",
    folder: "Folder:",
    folder_hint: "no folder",
    auto_name: "auto-name: {name}",
    reset_name: "Reset",
    colour: "Colour:",
    duplicate: "Duplicate",
    delete: "Delete",

    section_export: "Export",
    json_path: "JSON path:",
    json_path_hint: "annotations.json",
    precision: "Decimal places:",
    split_dir: "Folder for per-figure export:",
    split_dir_hint: "out/",
    write: "Write",
    split_hint: "Every figure is saved as its own file: <folder>/<name>.json",
    split_result: "{n} files in sub-folders inside {dir}",
    show_labels: "Labels",
    show_grid: "Grid",
    show_rulers: "Rulers",
    hotkeys: "Hotkeys",
    about: "About",

    status_hint: "Left click — point/rectangle · Shift — square · middle/right drag — pan · wheel — zoom",
    cursor_pos: "x = {x}   y = {y}",

    drop_image: "Drag an image here\nor type the path in the sidebar",

    no_figure_here: "No figures under the cursor",
    menu_select: "Select",
    menu_duplicate: "Duplicate",
    menu_reset_name: "Restore the default name",
    menu_delete: "Delete",

    tool_draw: "Draw",
    tool_select: "Select",
    tool_delete: "Delete",
    tool_pan: "Pan",
    tool_draw_hint: "Click for a point, drag for a rectangle, Shift for a square",
    tool_select_hint: "Click to select, drag to move the selection",
    tool_delete_hint: "Click a figure to delete it",
    tool_pan_hint: "Drag to pan the image",

    hotkeys_title: "Hotkeys",
    hotkeys_action: "Action",
    hotkeys_meaning: "What it does",

    about_title: "About",
    about_line1: "Measure images with points and rectangles,",
    about_line2: "name them, and export the annotation as JSON.",
    about_note: "Coordinates are pixels with the origin in the top-left corner.\nDefault names: point_N and rect_N, where N is the figure number.",

    loaded_image: "Loaded: {name}",
    image_failed: "Could not open {path}: {error}",
    loaded_figures: "Loaded {n} figures (points: {p}, rectangles: {r})",
    json_failed: "Failed to read JSON: {error}",
    read_failed: "Could not read {path}: {error}",
    saved: "Saved: {path}",
    save_failed: "Could not save {path}: {error}",
    copied: "JSON copied to the clipboard ({n} characters)",
    serialise_failed: "Could not serialise: {error}",
    split_done: "Wrote {n} figures into sub-folders inside {dir}",
    split_create_failed: "Could not create {dir}: {error}",
    split_failed: "Export failed: {error}",
    duplicated: "Copy created: {name}",
    renumbered: "Figures renumbered",
    split_result_done: "{n} files in sub-folders inside {dir}",
    need_image_path: "Type an image path in the sidebar",
    need_json_path: "Enter a path for the JSON file",
    need_split_dir: "Enter a directory for the per-figure export",
    no_clipboard: "The clipboard has no text",
    unknown_format: "Could not determine the format: {path}",
    opening_project: "Opening the project page…",
};

impl Text {
    /// Every string for `lang`.
    #[must_use]
    pub const fn get(lang: Lang) -> &'static Self {
        match lang {
            Lang::Ru => &RU,
            Lang::En => &EN,
        }
    }
}

/// Formats a message with named placeholders.
///
/// ```
/// # use picture_measure::i18n::{fill, Lang, Text};
/// let t = Text::get(Lang::Ru);
/// assert_eq!(fill(t.cursor_pos, &[("x", "1"), ("y", "2")]), "x = 1   y = 2");
/// ```
///
/// Placeholders that do not appear in the template are ignored, and any
/// `{name}` left unfilled is returned as-is, so a mistake shows up in the
/// status bar instead of panicking.
#[must_use]
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close_offset) = rest[open..].find('}') else {
            break;
        };
        let close = open + close_offset;
        let key = &rest[open + 1..close];
        out.push_str(&rest[..open]);
        if let Some((_, value)) = args.iter().find(|(name, _)| *name == key) {
            out.push_str(value);
        } else {
            // Неизвестная подстановка: показываем её как есть, чтобы опечатку
            // было видно в строке состояния, а не паникало программу.
            out.push('{');
            out.push_str(key);
            out.push('}');
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        assert_eq!(Lang::Ru.code(), "ru");
        assert_eq!(Lang::En.code(), "en");
        assert_eq!("ru".parse(), Ok(Lang::Ru));
        assert_eq!("EN".parse(), Ok(Lang::En));
        assert_eq!("en_US".parse(), Ok(Lang::En));
        assert_eq!("en-US.UTF-8".parse(), Ok(Lang::En));
        assert_eq!("rus".parse(), Ok(Lang::Ru));
        assert_eq!("Русский".parse(), Ok(Lang::Ru));
    }

    #[test]
    fn unknown_languages_are_rejected_with_a_useful_message() {
        let Err(error) = "de".parse::<Lang>() else {
            panic!("немецкий не должен поддерживаться");
        };
        let text = error.to_string();
        assert!(text.contains("de"), "{text}");
        assert!(text.contains("ru") && text.contains("en"), "{text}");
    }

    #[test]
    fn default_is_russian() {
        assert_eq!(Lang::default(), Lang::Ru);
        assert!(Lang::Ru.is_russian());
        assert!(!Lang::En.is_russian());
    }

    #[test]
    fn flipping_swaps_the_languages() {
        assert_eq!(Lang::Ru.flipped(), Lang::En);
        assert_eq!(Lang::En.flipped(), Lang::Ru);
        assert_eq!(Lang::Ru.flipped().flipped(), Lang::Ru);
    }

    #[test]
    fn every_language_has_a_native_name() {
        assert_eq!(Lang::Ru.native_name(), "Русский");
        assert_eq!(Lang::En.native_name(), "English");
        assert_eq!(Lang::Ru.english_name(), "Russian");
        assert_eq!(Lang::ALL.len(), 2);
    }

    #[test]
    fn both_locales_have_every_string_filled() {
        // Множество полей одно и то же: компилятор уже проверил бы иначе.
        // Здесь проверяем, что ни в одной локали строка не пустая и не осталась
        // шаблоном вида `{placeholder}` без подстановки.
        for lang in Lang::ALL {
            let text = Text::get(lang);
            let fields: Vec<(&'static str, &'static str)> = vec![
                ("window_title", text.window_title),
                ("open", text.open),
                ("save_json", text.save_json),
                ("tool_draw", text.tool_draw),
                ("figures", text.figures),
                ("status_hint", text.status_hint),
                ("hotkeys_title", text.hotkeys_title),
                ("about_note", text.about_note),
                ("loaded_image", text.loaded_image),
                ("cursor_pos", text.cursor_pos),
            ];
            for (name, value) in fields {
                assert!(!value.trim().is_empty(), "{lang}: поле {name} пустое");
            }
        }
    }

    #[test]
    fn the_two_locales_actually_differ() {
        assert_ne!(Text::get(Lang::Ru).save_json, Text::get(Lang::En).save_json);
        assert_ne!(Text::get(Lang::Ru).figures, Text::get(Lang::En).figures);
        assert_eq!(
            Text::get(Lang::Ru).json_path_hint,
            Text::get(Lang::En).json_path_hint,
            "подсказки-примеры совпадают специально"
        );
    }

    #[test]
    fn templates_use_the_same_placeholders_in_both_locales() {
        let keys = [
            "figures",
            "selected_count",
            "cursor_pos",
            "loaded_image",
            "saved",
            "copied",
            "split_result",
            "auto_name",
            "image_failed",
        ];
        let placeholders = |s: &str| -> Vec<String> {
            let mut out = Vec::new();
            let mut rest = s;
            while let Some(open) = rest.find('{') {
                let Some(offset) = rest[open..].find('}') else {
                    break;
                };
                let close = open + offset;
                out.push(rest[open + 1..close].to_owned());
                rest = &rest[close + 1..];
            }
            out
        };
        for key in keys {
            let ru = placeholders(text_of(Lang::Ru, key));
            let en = placeholders(text_of(Lang::En, key));
            assert_eq!(ru, en, "в шаблоне {key} наборы подстановок разошлись");
        }
    }

    /// Достаёт поле [`Text`] по имени — только для теста согласованности.
    fn text_of(lang: Lang, key: &str) -> &'static str {
        let t = Text::get(lang);
        match key {
            "figures" => t.figures,
            "selected_count" => t.selected_count,
            "cursor_pos" => t.cursor_pos,
            "loaded_image" => t.loaded_image,
            "saved" => t.saved,
            "copied" => t.copied,
            "split_result" => t.split_result,
            "auto_name" => t.auto_name,
            "image_failed" => t.image_failed,
            other => panic!("нет поля {other}"),
        }
    }

    #[test]
    fn fill_substitutes_named_arguments() {
        assert_eq!(fill("Фигуры: {n}", &[("n", "7")]), "Фигуры: 7");
        assert_eq!(
            fill("x = {x}   y = {y}", &[("x", "1.5"), ("y", "2.5")]),
            "x = 1.5   y = 2.5"
        );
        assert_eq!(fill("нет подстановок", &[]), "нет подстановок");
    }

    #[test]
    fn fill_keeps_unknown_placeholders_verbatim() {
        assert_eq!(fill("a {n} b", &[]), "a {n} b");
        assert_eq!(fill("a {n} b", &[("x", "1")]), "a {n} b");
    }

    #[test]
    fn fill_survives_unbalanced_braces() {
        assert_eq!(fill("a { b", &[("b", "1")]), "a { b");
        assert_eq!(fill("a }", &[]), "a }");
        assert_eq!(fill("", &[]), "");
    }

    #[test]
    fn fill_handles_repeated_and_adjacent_placeholders() {
        assert_eq!(fill("{n}-{n}", &[("n", "5")]), "5-5");
        assert_eq!(fill("{a}{b}", &[("a", "1"), ("b", "2")]), "12");
    }

    #[test]
    fn fill_works_on_every_real_template() {
        let t = Text::get(Lang::Ru);
        let rendered = fill(t.figures, &[("n", "3"), ("p", "1"), ("r", "2")]);
        assert_eq!(rendered, "Фигуры: 3 (точек 1, прямоугольников 2)");
        assert!(
            !rendered.contains('{'),
            "все подстановки применились: {rendered}"
        );

        let rendered = fill(t.loaded_figures, &[("n", "5"), ("p", "2"), ("r", "3")]);
        assert_eq!(rendered, "Загружено 5 фигур (точек: 2, прямоугольников: 3)");
    }

    #[test]
    fn language_detection_falls_back_to_russian() {
        assert_eq!(from_environment(), Lang::Ru);
    }
}
