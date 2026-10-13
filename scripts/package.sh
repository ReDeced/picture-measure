#!/bin/sh
# Собирает дистрибутивы в dist/: архив с бинарником, .deb и .rpm.
#
# Нужны fakeroot/dpkg-deb и rpmbuild; если их нет, .deb/.rpm пропускаются, а
# обычный архив собирается всегда.

set -eu

cd "$(dirname "$0")/.."

BINARY=picture-measure
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TARGET_DIR=target/release
DIST=dist

if [ ! -x "$TARGET_DIR/$BINARY" ]; then
    echo "Сначала выполните: make build" >&2
    exit 1
fi

rm -rf "$DIST"
mkdir -p "$DIST/$BINARY-$VERSION"
cp "$TARGET_DIR/$BINARY" "$DIST/$BINARY-$VERSION/"
cp README.md LICENSE "$DIST/$BINARY-$VERSION/"
mkdir -p "$DIST/$BINARY-$VERSION/docs"
cp docs/*.md "$DIST/$BINARY-$VERSION/docs/"
cp assets/icon.svg "$DIST/$BINARY-$VERSION/"

if command -v zip >/dev/null 2>&1; then
    (cd "$DIST" && zip -qr "$BINARY-$VERSION-linux-x86_64.zip" "$BINARY-$VERSION")
else
    tar -czf "$DIST/$BINARY-$VERSION-linux-x86_64.tar.gz" -C "$DIST" "$BINARY-$VERSION"
fi

# --- .deb -------------------------------------------------------------------
if command -v dpkg-deb >/dev/null 2>&1; then
    PKG="$DIST/pkg"
    rm -rf "$PKG"
    mkdir -p "$PKG/DEBIAN" \
             "$PKG/usr/bin" \
             "$PKG/usr/share/applications" \
             "$PKG/usr/share/doc/picture-measure" \
             "$PKG/usr/share/icons/hicolor/scalable/apps" \
             "$PKG/usr/share/icons/hicolor/256x256/apps" \
             "$PKG/usr/share/icons/hicolor/128x128/apps" \
             "$PKG/usr/share/icons/hicolor/48x48/apps" \
             "$PKG/usr/share/icons/hicolor/32x32/apps" \
             "$PKG/usr/share/icons/hicolor/16x16/apps"

    cp "$TARGET_DIR/$BINARY" "$PKG/usr/bin/"
    cp "assets/$BINARY.desktop" "$PKG/usr/share/applications/"
    cp "assets/icon.svg" "$PKG/usr/share/icons/hicolor/scalable/apps/$BINARY.svg"
    for s in 16 32 48 128 256; do
        [ -f "assets/icons/${s}x${s}.png" ] &&
            cp "assets/icons/${s}x${s}.png" "$PKG/usr/share/icons/hicolor/${s}x${s}/apps/$BINARY.png"
    done
    cp README.md LICENSE "$PKG/usr/share/doc/picture-measure/"
    cp docs/*.md "$PKG/usr/share/doc/picture-measure/"

    ARCH=$(dpkg --print-architecture 2>/dev/null || echo amd64)
    cat > "$PKG/DEBIAN/control" <<EOF
Package: picture-measure
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: ReDeced <github.com/ReDeced>
Installed-Size: $(du -ks "$PKG" | cut -f1)
Depends: libc6, libgl1
Description: Измерение изображений точками и прямоугольниками
 Позволяет расставлять на картинке точки и прямоугольники мышью, называть их,
 раскладывать по подпапкам и выгружать разметку в JSON.
EOF

    dpkg-deb --build --root-owner-group "$PKG" "$DIST/$BINARY_${VERSION}_${ARCH}.deb" >/dev/null
    echo "  Собран .deb"
else
    echo "  dpkg-deb не найден — .deb пропущен"
fi

# --- .rpm -------------------------------------------------------------------
if command -v rpmbuild >/dev/null 2>&1; then
    TOP=$(rpm --eval '%{_topdir}' 2>/dev/null || echo ~/.rpmmacros)
    mkdir -p "$TOP/SOURCES"
    cp "$TARGET_DIR/$BINARY" "$TOP/SOURCES/"
    tar -czf "$TOP/SOURCES/${BINARY}-${VERSION}.tar.gz" -C "$DIST" "$BINARY-$VERSION"

    cat > "$TOP/SPECS/picture-measure.spec" <<EOF
Name:           picture-measure
Version:        $VERSION
Release:        1%{?dist}
Summary:        Измерение изображений точками и прямоугольниками
License:        MIT
URL:            https://github.com/ReDeced/picture-measure
Source0:        %{name}-%{version}.tar.gz
Requires:       libgl
%description
Позволяет расставлять на картинке точки и прямоугольники мышью, называть их,
раскладывать по подпапкам и выгружать разметку в JSON.

%prep
%setup -q

%install
mkdir -p %{buildroot}%{_bindir}
install -m 0755 ${BINARY} %{buildroot}%{_bindir}/${BINARY}
mkdir -p %{buildroot}%{_datadir}/applications
install -m 0644 ${BINARY}.desktop %{buildroot}%{_datadir}/applications/${BINARY}.desktop
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
install -m 0644 icon.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/${BINARY}.svg

%files
%{_bindir}/${BINARY}
%{_datadir}/applications/${BINARY}.desktop
%{_datadir}/icons/hicolor/scalable/apps/${BINARY}.svg
EOF

    if rpmbuild -bb "$TOP/SPECS/picture-measure.spec" >/dev/null 2>&1; then
        find "$TOP/RPMS" -name '*.rpm' -exec cp {} "$DIST/" \; 2>/dev/null || true
        echo "  Собран .rpm"
    else
        echo "  rpmbuild не смог собрать пакет — .rpm пропущен"
    fi
else
    echo "  rpmbuild не найден — .rpm пропущен"
fi

echo
echo "Готово, dist/:"
ls -lh "$DIST" | grep -v '^total'