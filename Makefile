# picture-measure — сборка, проверки и установка
#
#   make            собрать релизную версию
#   make run        собрать и запустить
#   make test       прогнать все тесты
#   make check      fmt + clippy + тесты (то же, что в CI)
#   make install    поставить в ~/.local
#   make uninstall  убрать из ~/.local
#
# Переменные, которые можно переопределить:
#   PREFIX=$(HOME)/.local        префикс установки
#   BINDIR=$(PREFIX)/bin         куда класть бинарник
#   DATADIR=$(PREFIX)/share     куда класть иконки и .desktop
#   STRIP=0                     не вырезать отладочные символы
#   ARGS="photo.png"            аргументы для make run
#
# Системная установка (для всех пользователей):
#   sudo make install PREFIX=/usr/local

CARGO       ?= cargo
CARGO_FLAGS ?=
BINARY      := picture-measure
TARGET_DIR  ?= target/release
BIN         := $(TARGET_DIR)/$(BINARY)

PREFIX      ?= $(HOME)/.local
DESTDIR     ?=
BINDIR      ?= $(PREFIX)/bin
DATADIR     ?= $(PREFIX)/share
ICONDIR     ?= $(DATADIR)/icons/hicolor
APPDIR      ?= $(DATADIR)/applications
DOCDIR      ?= $(DATADIR)/doc/picture-measure

# Полосатое «не вставляйте сюда» по умолчанию.
ifneq ($(strip $(DESTDIR)),)
  space :=
else
  space := /NOP
endif

# Отладочные символы вырезаем: готовая программа весит ~2 МБ вместо ~400 МБ.
ifeq ($(origin STRIP),undefined)
  STRIP := 1
endif

ifeq ($(strip $(RUSTFLAGS)),)
RUSTFLAGS := $(if $(filter 1,$(STRIP)),-C strip=symbols,)
endif

.PHONY: all build release debug run test test-verbose check fmt fmt-check clippy lint \
        doc desktop-check install uninstall icons source \
        dist package appimage clean clean-all help list

## all: собрать релизную версию (по умолчанию)
all: build

## build: собрать релизную версию
build release:
		$(CARGO) build --release $(CARGO_FLAGS)

## debug: собрать отладочную версию
debug:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build $(CARGO_FLAGS)

## run: собрать и запустить программу
run: build
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) run --release $(CARGO_FLAGS) -- $(ARGS)

## test: прогнать все тесты
test:
	$(CARGO) test --all-targets $(CARGO_FLAGS)

## test-verbose: прогнать тесты с выводом имён тестов
test-verbose:
	$(CARGO) test --all-targets -- --nocapture

## fmt: отформатировать код
fmt:
	$(CARGO) fmt --all

## fmt-check: проверить форматирование (как в CI)
fmt-check:
	$(CARGO) fmt --all -- --check

## clippy: прогнать clippy
clippy lint:
	$(CARGO) clippy --all-targets --all-features $(CARGO_FLAGS)

## check: всё, что проверяет CI
check: fmt-check clippy test desktop-check

## desktop-check: проверить файл .desktop
desktop-check:
	@if command -v desktop-file-validate >/dev/null 2>&1; then \
	    desktop-file-validate assets/$(BINARY).desktop && echo "  .desktop в порядке"; \
	else \
	    echo "  desktop-file-validate не найден — проверка пропущена"; \
	fi

## doc: собрать документацию
doc:
	$(CARGO) doc --no-deps --open

## icons: перегенерировать PNG-иконки из assets/icon.svg
icons:
	@scripts/make-icons.sh

## install: установить в $(BINDIR), иконки и .desktop — в $(DATADIR)
install: build icons
	@echo "  Установка в $(DESTDIR)$(PREFIX)"
	install -Dm755 $(BIN) $(DESTDIR)$(BINDIR)/$(BINARY)
	install -Dm644 assets/icon.svg $(DESTDIR)$(ICONDIR)/scalable/apps/$(BINARY).svg
	install -Dm644 assets/icons/16x16.png   $(DESTDIR)$(ICONDIR)/16x16/apps/$(BINARY).png
	install -Dm644 assets/icons/32x32.png   $(DESTDIR)$(ICONDIR)/32x32/apps/$(BINARY).png
	install -Dm644 assets/icons/48x48.png   $(DESTDIR)$(ICONDIR)/48x48/apps/$(BINARY).png
	install -Dm644 assets/icons/128x128.png $(DESTDIR)$(ICONDIR)/128x128/apps/$(BINARY).png
	install -Dm644 assets/icons/256x256.png $(DESTDIR)$(ICONDIR)/256x256/apps/$(BINARY).png
	install -Dm644 assets/$(BINARY).desktop  $(DESTDIR)$(APPDIR)/$(BINARY).desktop
	install -Dm644 README.md                  $(DESTDIR)$(DOCDIR)/README.md
	install -Dm644 LICENSE                    $(DESTDIR)$(DOCDIR)/LICENSE
	install -Dm644 docs/JSON_FORMAT.md        $(DESTDIR)$(DOCDIR)/JSON_FORMAT.md
	install -Dm644 docs/USER_GUIDE.md         $(DESTDIR)$(DOCDIR)/USER_GUIDE.md
	@echo
	@echo "  Готово. Запуск:  $(BINARY)"
	@if [ -z "$(DESTDIR)" ] && [ "$(PREFIX)" = "$(HOME)/.local" ]; then \
	    echo "  Если $(BINDIR) не в PATH, добавьте строку:"; \
	    echo "      export PATH=\"\$$HOME/.local/bin:\$$PATH\""; \
	    echo "  Чтобы пункт появился в меню приложений:"; \
	    echo "      update-desktop-database $(APPDIR) 2>/dev/null || true"; \
	fi

## uninstall: убрать программу
uninstall:
	@echo "  Удаление из $(DESTDIR)$(PREFIX)"
	rm -f  $(DESTDIR)$(BINDIR)/$(BINARY)
	rm -f  $(DESTDIR)$(APPDIR)/$(BINARY).desktop
	rm -rf $(DESTDIR)$(ICONDIR)/*/apps/$(BINARY).png
	rm -f  $(DESTDIR)$(ICONDIR)/scalable/apps/$(BINARY).svg
	rm -rf $(DESTDIR)$(DOCDIR)
	@command -v update-desktop-database >/dev/null 2>&1 && \
	    update-desktop-database $(DESTDIR)$(APPDIR) 2>/dev/null || true
	@echo "  Готово"

## source: собрать тарбол с исходниками (требование GPL к бинарникам)
source:
	@mkdir -p dist
	@git archive --format=tar.gz --prefix=picture-measure-$(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)/ \
	    -o dist/picture-measure-$(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)-source.tar.gz HEAD \
	    || echo "Нет коммитов: соберите тарбол из рабочей копии вручную"
	@ls -lh dist/*-source.tar.gz 2>/dev/null || true

## package: собрать .deb, .rpm и архив в dist/
package dist: build
	@scripts/package.sh

## appimage: собрать один запускаемый файл в dist/
appimage: build
	@scripts/make-appimage.sh

## clean: удалить артефакты сборки
clean:
	$(CARGO) clean
	rm -rf dist

## clean-all: clean + удалить сгенерированные иконки
clean-all: clean
	rm -rf assets/icons

## help: показать список целей
help list:
	@echo "picture-measure — доступные цели:"
	@echo
	@sed -n 's/^## //p' $(MAKEFILE_LIST) | awk -F': ' '{printf "  \033[1m%-14s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "Переменные: PREFIX=$(PREFIX)  BINDIR=$(BINDIR)  DATADIR=$(DATADIR)"