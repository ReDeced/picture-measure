# Работа с ветками и pull request

Подробности о стиле коммитов и оформлении PR — в
[CONTRIBUTING.md](../CONTRIBUTING.md). Здесь только про ветки.

## Схема веток

Проект держит одну долгоживущую ветку и короткоживущие ветки задач.

```text
        ┌── feat/square-drag ──┐
main ───┼── fix/zoom-anchor ───┼──► main ──► tag v0.2.0
  ▲     └── docs/json-format ─┘        │
  │                                  ▼
  └────── hotfix/crash-on-zoom ─── release/v0.2.1
```

| Ветка | Время жизни | Что попадает |
|---|---|---|
| `main` | бесконечно | только проверенный код, всегда собирается |
| `feat/*` `fix/*` `docs/*` `refactor/*` `chore/*` | до слиятия PR | работа по задаче |
| `hotfix/*` | до слияния | срочное исправление для релиза |
| `release/*` | от подготовки релиза до публикации | только исправления, отложенные с прошлого релиза |

Отдельная ветка `develop` не используется: при таком размере проекта она только
добавляет работы по синхронизации.

## Когда нужна ветка

Отдельная ветка обязательна, если выполняется хотя бы одно условие:

- работа дольше одного дня;
- затрагиваются два и более модуля;
- есть риск сломать существующее поведение;
- хочется показать идею до того, как она готова.

Мелкие правки — опечатка в тексте, исправление ссылки, однострочный багфикс —
можно отправлять прямо в `main` отдельным PR.

## Имена веток

```
<тип>/<короткое-описание>
```

| Префикс | Для чего |
|---|---|
| `feat/` | новая функциональность |
| `fix/` | исправление ошибки |
| `docs/` | документация |
| `refactor/` | перестройка без изменения поведения |
| `perf/` | ускорение |
| `test/` | тесты |
| `chore/` | зависимости, CI, разное |
| `hotfix/` | срочное исправление в релиз |

Пишем латиницей, через косую черту, без пробелов и кириллицы — по имени ветки
потом ищут через `git log --grep` и в списках PR.

Примеры:

```text
feat/shift-square
feat/split-export
fix/zoom-keeps-cursor-point
fix/lost-release-cancels-drag
docs/json-format-examples
refactor/move-logic-into-session
chore/bump-egui
```

## Создание ветки

```bash
# Актуальный main
git fetch upstream
git checkout main
git pull --ff-only upstream main

# Новая ветка от него
git checkout -b feat/shift-square

# …работа…

git add -A
git commit
git push -u origin feat/shift-square
```

Держите ветку в актуальном состоянии относительно `main`:

```bash
git fetch upstream
git rebase upstream/main
```

> `rebase` предпочтительнее `merge`: история остаётся линейной, а в PR нет
> искусственного merge-коммита. Используйте `merge`, только если вас попросили.

## Pull request

1. **Откройте PR** из своей ветки в `main` основного репозитория:
   ```
   https://github.com/ReDeced/picture-measure/compare/main?...head=ReDeced:feat/shift-square
   ```
2. **Заполните описание** по шаблону: что изменилось, как проверить, что
   изменилось в документации.
3. **Свяжите задачу**: `Closes #12` в теле коммита или в описании PR. Issue
   закроется автоматически после слияния.
4. **Дождитесь CI**: форматирование, clippy, тесты на Linux/Windows/macOS,
   сборка под четыре целевые платформы, проверка на минимальной версии Rust.
5. **Отвечайте на замечания** и обновляйте PR новыми коммитами или
   `--amend`. Сила push не важна — историю никто не переписывает поверх
   master-ветки.
6. **Смержите**, когда всё зелёное: squash-коммит для длинной ветки, обычный
   merge для короткой из двух-трёх коммитов.

### Из чего PR не собирают

- Не собирается CI.
- Новое поведение без теста.
- Изменение формата JSON без обновления `docs/JSON_FORMAT.md` и `CHANGELOG.md`.
- Новую зависимость, не обсуждённую в issue.
- Массовое переформатирование файла, не связанное с задачей.
- Несколько несвязанных изменений в одном PR.

## Релиз

```bash
git checkout main
git pull --ff-only upstream main

# Обновить версию в Cargo.toml и раздел в CHANGELOG.md,
# отправить это отдельным PR и дождаться зелёного CI.

git tag -a v0.2.0 -m "v0.2.0 — <одной строкой о главном>"
git push upstream v0.2.0
```

Тег `v*` запускает `.github/workflows/release.yml`, который собирает бинарники
под пять целевых платформ, считает SHA-256 и создаёт релиз на GitHub.

### Горячее исправление

```bash
git checkout -b hotfix/crash-on-save v0.2.0
# исправить, добавить тест
git push -u origin hotfix/crash-on-save
```

Дальше **два** PR:

1. в `main` — чтобы исправление не потерялось;
2. в `release/v0.2.1` — чтобы попасть в ближайший релиз.

```bash
git checkout -b release/v0.2.1 v0.2.0
git cherry-pick <коммит-исправления>
```

После публикации релиза ветку `release/*` удаляют.

### Откат

```bash
git revert <коммит>      # безопасный откат: история сохраняется
```

`git revert`, а не `reset --hard`: опубликованную историю не переписывают.

## Быстрая шпаргалка

```bash
# Новая задача
git fetch upstream && git checkout main && git pull --ff-only
git checkout -b fix/short-description
# … работа …
make check
git commit -am "fix(область): описание"
git push -u origin fix/short-description
# → открыть PR в ReDeced/picture-measure

# Обновить ветку перед отправкой
git fetch upstream && git rebase upstream/main
git push --force-with-lease

# Посмотреть, что готовится
git log --oneline --graph upstream/main..HEAD
git diff upstream/main...HEAD --stat

# Откатить изменение
git revert <коммит>
```