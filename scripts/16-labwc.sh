#!/usr/bin/env bash
# labwc 0.7.1 (Ubuntu noble's source) with image/labwc/*.patch applied:
# macOS window button order (close, minimise, maximise on the left).
# 0.7.1 has no setting for the order. The result, build/labwc/labwc,
# replaces the packaged /usr/bin/labwc in the image (scripts/20-rootfs.sh);
# the package still brings its libraries.
# Needs the build dependencies of Ubuntu's labwc source package (libwlroots-dev,
# meson, librsvg2-dev, ...).
set -euo pipefail
cd "$(dirname "$0")/.."
V=0.7.1
SHA=1810ec55e287708e7a3cd44c726aa887db02480704db82b3d0bd550a6c4bfb76
D=third_party/labwc-src
TAR=$D/labwc_$V.orig.tar.gz
mkdir -p "$D"
[ -f "$TAR" ] || curl -sSfL -o "$TAR" "http://archive.ubuntu.com/ubuntu/pool/universe/l/labwc/labwc_$V.orig.tar.gz"
echo "$SHA  $TAR" | sha256sum -c --quiet
W=build/labwc-src
rm -rf "$W" && mkdir -p "$W"
tar -xf "$TAR" -C "$W" --strip-components=1
for p in image/labwc/*.patch; do patch -d "$W" -p1 --quiet < "$p"; done
# Same hardening flags as the Ubuntu package (dpkg-buildflags hardening=+all).
export DEB_BUILD_MAINT_OPTIONS=hardening=+all
eval "$(dpkg-buildflags --export=sh)"
meson setup "$W/build" "$W" --buildtype=plain --prefix=/usr -Dman-pages=disabled >/dev/null
ninja -C "$W/build" >/dev/null
mkdir -p build/labwc
install -m 755 "$W/build/labwc" build/labwc/labwc
strip build/labwc/labwc
echo "build/labwc/labwc: labwc $V + $(ls image/labwc/*.patch | wc -l) patch(es)"
