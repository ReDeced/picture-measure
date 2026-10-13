<picture align="right" width="96" height="96" src="assets/icon.svg" alt="logo">

# picture-measure

Настольная программа для измерения картинок: открываете изображение, расставляете
мышкой **точки** и **прямоугольники**, называете их, раскладываете по подпапкам и
выгружаете готовую разметку в JSON.

<!-- English summary below -->

| | |
|---|---|
| **Запуск** | `cargo run --release` |
| **Форматы картинок** | PNG, JPEG, WebP, BMP |
| **Формат разметки** | JSON (см. [docs/JSON_FORMAT.md](docs/JSON_FORMAT.md)) |
| **Лицензия** | MIT |

---

## Быстрый старт

```bash
git clone https://github.com/ReDeced/picture-measure.git
cd picture-measure
cargo run --release                 # без аргументов
cargo run --release -- photo.png    # сразу открыть картинку
cargo run --release -- photo.png photo.json   # картинка + готовая разметка
```

Дальше:

1. Перетащите картинку в окно (или впишите путь на боковой панели и нажмите **Открыть**).
2. Кликните — получится **точка**.
3. Зажмите и потяните — получится **прямоугольник**.
4. Зажмите <kbd>Shift</kbd> и потяните — получится **квадрат**.
5. Нажмите <kbd>Ctrl</kbd>+<kbd>S</kbd> — разметка сохранится в JSON.

## Установка

### Готовые сборки

Для Windows, macOS и Linux есть готовые бинарники в
[Releases](https://github.com/ReDeced/picture-measure/releases).

### Linux: `make install`

```bash
git clone https://github.com/ReDeced/picture-measure.git
cd picture-measure

make install                    # в ~/.local, ничего системного не трогает
picture-measure photo.png
```

`make install` собирает программу, ставит бинарник, иконки и файл `.desktop`,
поэтому программа появится в меню приложений. Путь `~/.local/bin` должен быть
в `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Системная установка для всех пользователей:

```bash
sudo make install PREFIX=/usr/local
```

Полезные переменные:

| Переменная | По умолчанию | Значение |
|---|---|---|
| `PREFIX` | `~/.local` | корень установки |
| `BINDIR` | `$(PREFIX)/bin` | куда класть бинарник |
| `DATADIR` | `$(PREFIX)/share` | иконки, `.desktop`, документация |
| `DESTDIR` | — | стадия для сборки пакетов |
| `STRIP` | `1` | вырезать отладочные символы |
| `ARGS` | — | аргументы для `make run` |

Откат: `make uninstall`.

### Linux: пакеты

```bash
make package        # dist/: архив, .deb и .rpm (если есть dpkg-deb и rpmbuild)
sudo apt install ./dist/picture-measure_0.1.0_amd64.deb
```

### Из исходников

Нужен Rust 1.95 или новее. Системных библиотек доустанавливать не нужно:

```bash
cargo install --path .          # тоже ставит в ~/.cargo/bin
```

### Управление

Все цели — `make help`.

## Управление

| Действие | Как |
|---|---|
| Поставить точку | ЛКМ (клик) |
| Нарисовать прямоугольник | ЛКМ (зажать и потянуть) |
| Нарисовать квадрат | ЛКМ + <kbd>Shift</kbd> (потянуть) |
| Переместить картинку | СКМ или ПКМ (потянуть), либо <kbd>Alt</kbd> + ЛКМ |
| Масштаб | Колесо мыши |
| Выбрать фигуру | Инструмент «Выбор» (<kbd>V</kbd>) + клик |
| Добавить к выделению | <kbd>Shift</kbd> + клик |
| Перетащить фигуру | Инструмент «Выбор», тянуть за фигуру |
| Удалить фигуру | Инструмент «Удаление» (<kbd>X</kbd>) + клик, <kbd>Delete</kbd>, ПКМ-меню |
| Сменить инструмент | <kbd>D</kbd> / <kbd>V</kbd> / <kbd>X</kbd> / <kbd>H</kbd> |
| Вписать картинку | <kbd>F</kbd> или кнопка «Вписать» |
| Сохранить / копировать JSON | <kbd>Ctrl</kbd>+<kbd>S</kbd> / <kbd>Ctrl</kbd>+<kbd>C</kbd> |
| Отменить / повторить | <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> |
| Выбрать всё / снять выделение | <kbd>Ctrl</kbd>+<kbd>A</kbd> / <kbd>Esc</kbd> |

Полный список — в меню «Горячие клавиши» внутри программы и в
[docs/USER_GUIDE.md](docs/USER_GUIDE.md).

## Имена и подпапки

По умолчанию каждая фигура получает имя вида `point_1`, `rect_2` — префикс плюс
сквозной номер фигуры. Номера не переиспользуются после удаления, поэтому имена
всегда уникальны. Кнопка **Перенумеровать** снова делает их подряд
(`point_1`, `rect_2`, `point_3` …).

Любой фигуре можно задать своё имя и **подпапку** (она же «класс» или «группа»):

```
eyes/left_eye     точки на левом глазу
faces/nose        нос
rect_7            без папки
```

Подпапка пишется в поле `folder` и используется при раздельном экспорте, когда
каждая фигура сохраняется отдельным файлом:

```
out/
├── eyes/left_eye.json
├── faces/nose.json
└── rect_7.json
```

## Что получается на выходе

```json
{
  "format": "picture-measure",
  "version": 1,
  "image": { "path": "photo.png", "name": "photo.png", "width": 1920, "height": 1080 },
  "settings": { "coordinate_system": "pixel", "origin": "top-left" },
  "folders": ["eyes", "faces"],
  "counts": { "total": 3, "points": 1, "rects": 2 },
  "annotations": [
    { "type": "point", "name": "point_1", "id": 1, "folder": "eyes", "color": "#ff5c5c", "x": 120, "y": 80 },
    { "type": "rect", "name": "nose",   "id": 2, "folder": "faces", "x": 300, "y": 240, "width": 90, "height": 70 },
    { "type": "rect", "name": "rect_3", "id": 3, "x": 10, "y": 20, "width": 100, "height": 100 }
  ]
}
```

Координаты — в пикселях исходной картинки, начало в левом верхнем углу. Формат
описан в [docs/JSON_FORMAT.md](docs/JSON_FORMAT.md), а готовые файлы можно
загружать обратно кнопкой **Открыть** или через аргумент командной строки.

## Разработка

```bash
cargo test              # 190+ тестов, окно не нужно
cargo clippy --all-targets
cargo fmt --check
```

Всё, что можно проверить без окна, вынесено в библиотеку `picture_measure`, а
`src/gui/` — только отрисовка. Подробности в
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

Как участвовать в проекте: [CONTRIBUTING.md](CONTRIBUTING.md) — там же про
ветки, коммиты и pull request.

---

## English

**picture-measure** is a desktop tool for measuring images. Load a picture, place
**points** and **rectangles** with the mouse, give them names and sub-folders, and
export the result as JSON.

- single click → point
- click and drag → rectangle
- <kbd>Shift</kbd> + drag → square
- middle/right drag or <kbd>Alt</kbd> + left drag → pan, mouse wheel → zoom
- figures are named `point_N` / `rect_N` by default, and any figure can be
  renamed and put into a sub-folder

Built with Rust and [egui](https://github.com/emilk/egui). Coordinates are pixels
of the source image with the origin in the top-left corner. See
[docs/JSON_FORMAT.md](docs/JSON_FORMAT.md) for the file format and
[docs/USER_GUIDE.md](docs/USER_GUIDE.md) for the full manual.

## License

MIT — see [LICENSE](LICENSE).