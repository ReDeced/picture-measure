#!/bin/sh
# Собирает AppImage: один файл, который запускается на любом дистрибутиве Linux.
#
# Почему внутри образа нет библиотек
# ---------------------------------
# Первый вариант клал внутрь X11, Wayland и OpenGL из Ubuntu 20.04 — и падал
# при создании окна. Стек был такой:
#
#     glXChooseFBConfig (Mesa 20.04 из образа)
#       -> swrast_dri.so        (Mesa из образа)
#       -> libEGL.so.1          (Mesa из образа)
#       -> libEGL_nvidia.so.0   (драйвер из СИСТЕМЫ)
#       -> libgbm.so.1          (Mesa из образа)  <- вызов по адресу 0x0
#
# Драйвер видеокарты всегда остаётся системным: положить его в AppImage
# нельзя, он привязан к конкретной версии ядра и карты. А значит, любая
# библиотека GL/EGL/gbm внутри образа гарантированно столкнётся с системной.
#
# Программа — чистый Rust, и всё, что ей нужно от системы, — это glibc, X11,
# xkbcommon и OpenGL. Всё это есть на любом рабочем столе Linux. Поэтому
# AppImage содержит только саму программу, а AppRun запускает её через
# системный загрузчик: так несовместимости просто не возникает.
#
# Что это значит на практике: образ не «переносит» видеодрайвер и системные
# библиотеки, он даёт неизменную версию программы в одном файле. Если нужна
# полностью автономная сборка, её делают в контейнере с Ubuntu 20.04 —
# тогда glibc внутри образа совпадает с glibc собранных библиотек.
#
# Запуск:
#   ./scripts/make-appimage.sh
#
# Переменные окружения:
#   CARGO_BUILD_JOBS  параллельность cargo (по умолчанию 3 — сборка тяжёлая)
#   APPIMAGETOOL      путь к appimagetool; если не задан, качается в dist/

set -eu

cd "$(dirname "$0")/.."

APP=picture-measure
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DIST=dist
APPDIR="$DIST/AppDir"
BINARY="target/release/$APP"
OUT="$DIST/$APP-$VERSION-x86_64.AppImage"

# --- сборка бинарника -------------------------------------------------------

if [ ! -x "$BINARY" ]; then
    echo "Собираю…" >&2
    CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-3} cargo build --release
fi

# --- appimagetool -----------------------------------------------------------

if [ -z "${APPIMAGETOOL:-}" ]; then
    APPIMAGETOOL="$PWD/$DIST/appimagetool"
    if [ ! -x "$APPIMAGETOOL" ]; then
        echo "Скачиваю appimagetool…" >&2
        curl -sSL -o "$APPIMAGETOOL" \
            "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
        chmod +x "$APPIMAGETOOL"
    fi
else
    # Ниже скрипт уходит в другой каталог, поэтому путь должен быть абсолютным.
    APPIMAGETOOL=$(cd "$(dirname "$APPIMAGETOOL")" && pwd)/$(basename "$APPIMAGETOOL")
fi
if [ ! -x "$APPIMAGETOOL" ]; then
    echo "Нет appimagetool: $APPIMAGETOOL" >&2
    exit 1
fi

# --- каркас образа ----------------------------------------------------------

rm -rf "$APPDIR"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/doc/$APP" \
         "$APPDIR/usr/share/icons/hicolor"

cp "$BINARY" "$APPDIR/usr/bin/$APP"
cp LICENSE README.md "$APPDIR/usr/share/doc/$APP/"
if [ -d docs ]; then
    cp docs/*.md "$APPDIR/usr/share/doc/$APP/"
fi

for size in 16 32 48 128 256; do
    mkdir -p "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps"
    if [ -f "assets/icons/${size}x${size}.png" ]; then
        cp "assets/icons/${size}x${size}.png" \
           "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps/$APP.png"
        ln -sf "usr/share/icons/hicolor/${size}x${size}/apps/$APP.png" "$APPDIR/$APP.png"
    fi
done

cp "assets/$APP.desktop" "$APPDIR/$APP.desktop"
ln -sf "$APP.png" "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
# Точка входа AppImage.
#
# Runtime AppImage несёт свою glibc (2.31), и если запустить программу через
# неё, системные библиотеки графики соберутся под неё же. Поэтому бинарник
# стартует через СИСТЕМНЫЙ загрузчик: glibc, X11 и видеодрайвер берутся у
# системы и всегда согласованы между собой.
HERE=$(dirname "$(readlink -f "$0")")
APP="$HERE/usr/bin/picture-measure"

for loader in /lib64/ld-linux-x86-64.so.2 /lib/ld-linux-x86-64.so.2; do
    if [ -x "$loader" ]; then
        exec "$loader" "$APP" "$@"
    fi
done

# Загрузчик не нашёлся — запускаем как есть, авось glibc внутри образа
# подойдёт (например, на очень старой системе, где и библиотеки старые).
exec "$APP" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

# --- сборка -------------------------------------------------------------------

echo "Собираю AppImage…" >&2
rm -f "$OUT"
"$APPIMAGETOOL" --no-appstream "$APPDIR" "$OUT" >&2

echo
echo "Готово: $OUT"
ls -lh "$OUT" | awk '{print "  размер:", $5}'