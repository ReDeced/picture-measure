#!/bin/sh
# Генерирует PNG-иконки из assets/icon.svg для установки в меню приложений.
#
# Использует ImageMagick (`convert`/`magick`), если он есть, иначе — Python с
# Pillow. Если не найдено ни то, ни другое — рисует иконки напрямую через
# Pillow минимальным кодом, чтобы установка не падала.

set -eu

cd "$(dirname "$0")/.."
OUT="assets/icons"
SVG="assets/icon.svg"
SIZES="16 32 48 128 256"

mkdir -p "$OUT"

render_with_magick() {
    command -v magick >/dev/null 2>&1 && CONVERT=magick ||
        command -v convert >/dev/null 2>&1 && CONVERT=convert ||
        return 1

    for size in $SIZES; do
        "$CONVERT" -background none -density 384 "$SVG" \
            -resize "${size}x${size}" "$OUT/${size}x${size}.png"
    done
    return 0
}

render_with_python() {
    python3 - "$SVG" "$OUT" $SIZES <<'PY'
import sys
from PIL import Image, ImageDraw

svg, out, sizes = sys.argv[1], sys.argv[2], [int(s) for s in sys.argv[3:]]

BG = (30, 32, 40, 255)
FRAME = (64, 196, 255, 255)
GRID = (64, 196, 255, 70)
SQUARE = (255, 214, 51, 255)
SQUARE_FILL = (64, 196, 255, 64)
POINT = (255, 92, 92, 255)

for size in sizes:
    # Рисуем с запасом и уменьшаем: так линии получаются сглаженными.
    s = size * 8
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    u = s / 128.0  # единица из координат исходного SVG

    def box(x, y, w, h):
        return [x * u, y * u, (x + w) * u, (y + h) * u]

    d.rounded_rectangle(box(0, 0, 128, 128), radius=24 * u, fill=BG)

    # Сетка внутри рамки.
    for i in range(16, 113, 16):
        d.line([i * u, 22 * u, i * u, 106 * u], fill=GRID, width=max(1, int(1.5 * u)))
        d.line([14 * u, i * u, 114 * u, i * u], fill=GRID, width=max(1, int(1.5 * u)))

    d.rounded_rectangle(box(14, 22, 100, 84), radius=8 * u, outline=FRAME, width=max(1, int(6 * u)))
    d.rectangle(box(36, 42, 40, 40), fill=SQUARE_FILL, outline=SQUARE, width=max(1, int(5 * u)))

    cx, cy, r, arm = 95 * u, 95 * u, 7 * u, 11 * u
    w = max(1, int(3 * u))
    d.line([cx - arm, cy, cx - r, cy], fill=POINT, width=w)
    d.line([cx + r, cy, cx + arm, cy], fill=POINT, width=w)
    d.line([cx, cy - arm, cx, cy - r], fill=POINT, width=w)
    d.line([cx, cy + r, cx, cy + arm], fill=POINT, width=w)
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=POINT, outline=BG, width=max(1, int(3 * u)))

    img.resize((size, size), Image.LANCZOS).save(f"{out}/{size}x{size}.png")
PY
}

if render_with_magick 2>/dev/null; then
    echo "Иконки собраны ImageMagick: $OUT"
elif render_with_python 2>/dev/null; then
    echo "Иконки собраны Pillow: $OUT"
else
    echo "Не найдены ни ImageMagick, ни Pillow — иконки не собраны." >&2
    echo "Скопируйте assets/icon.svg вручную или поставьте python3-pillow." >&2
    exit 1
fi

ls -1 "$OUT"