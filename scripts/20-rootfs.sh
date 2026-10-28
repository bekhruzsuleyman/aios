#!/usr/bin/env bash
# T3a: build the root filesystem with mkosi, then pack it as erofs.
#   build/rootfs/            product tree        -> build/rootfs.erofs
#   build/rootfs-test/       + serial root shell -> build/rootfs-test.erofs
# Our binaries are included when built (cargo build --release).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
export PATH="$PATH:$HOME/.cargo/bin:$ROOT/build/venv/bin"
SRC=image/rootfs
EXTRA=build/rootfs-extra
mkdir -p build

# 1. Files we ship, laid out as they appear in the image.
rm -rf "$EXTRA"
mkdir -p "$EXTRA/etc/aios" "$EXTRA/usr/lib/systemd/system" "$EXTRA/usr/bin"
cp -a "$SRC/etc/." "$EXTRA/etc/"
rm -rf "$EXTRA/etc/labwc"
cp -a "$SRC/etc/labwc" "$EXTRA/etc/aios/labwc"
cp -a "$SRC/usr/." "$EXTRA/usr/"
cp -a "$SRC/units/." "$EXTRA/usr/lib/systemd/system/"
[ -f config/aios.toml ] && cp config/aios.toml "$EXTRA/etc/aios/aios.toml"
# Kernel modules (T8), installed by scripts/13-kernel-build.sh.
KVER=$(cat third_party/KERNEL_VERSION)
if [ -d "build/modules/lib/modules/$KVER" ]; then
  mkdir -p "$EXTRA/usr/lib/modules"
  cp -a "build/modules/lib/modules/$KVER" "$EXTRA/usr/lib/modules/"
  rm -f "$EXTRA/usr/lib/modules/$KVER/build" "$EXTRA/usr/lib/modules/$KVER/source"
  echo "including modules for $KVER"
fi
# Firefox (Mozilla's own .deb; Ubuntu only ships a snap) and the serif for
# the AI's voice. Both are fetched outside this script into
# third_party/downloads/ (see docs/DOWNLOADS.md) and checked here.
DL=third_party/downloads
FFDEB=$(ls "$DL"/firefox_*_amd64*.deb 2>/dev/null | head -n1 || true)
if [ -n "$FFDEB" ]; then
  want=$(awk '/^SHA256:/ {print $2}' "$DL/firefox.Packages.txt")
  have=$(sha256sum "$FFDEB" | cut -d' ' -f1)
  [ "$want" = "$have" ] || { echo "firefox .deb checksum mismatch" >&2; exit 1; }
  mkdir -p "$EXTRA/var/cache/aios-pkgs"
  cp "$FFDEB" "$EXTRA/var/cache/aios-pkgs/firefox.deb"
  echo "including $(basename "$FFDEB")"
else
  echo "warning: no Firefox .deb in $DL; the image will have no browser" >&2
fi
if [ -f "$DL/InstrumentSerif-Italic.ttf" ]; then
  (cd "$DL" && sha256sum -c --ignore-missing --quiet SHA256SUMS)
  F="$EXTRA/usr/share/fonts/truetype/instrument-serif"
  mkdir -p "$F"
  cp "$DL"/InstrumentSerif-*.ttf "$DL/OFL.txt" "$F/"
fi
# labwc with the macOS button order (scripts/16-labwc.sh), over the package's.
if [ -x build/labwc/labwc ]; then
  install -m 0755 build/labwc/labwc "$EXTRA/usr/bin/labwc"
  echo "including patched labwc"
fi
# Built-in AI: llama-server (scripts/15-llama.sh) and the model named in
# $DL/models/DEFAULT (checked against $DL/models/SHA256SUMS; see
# docs/DOWNLOADS.md).
if [ -x build/llama/bin/llama-server ]; then
  mkdir -p "$EXTRA/usr/lib/aios/llama"
  cp -a build/llama/bin build/llama/lib "$EXTRA/usr/lib/aios/llama/"
  echo "including llama-server $(cat build/llama/VERSION)"
fi
if [ -f "$DL/models/DEFAULT" ]; then
  m=$(cat "$DL/models/DEFAULT")
  (cd "$DL/models" && grep " $m\$" SHA256SUMS | sha256sum -c --quiet)
  mkdir -p "$EXTRA/usr/share/aios/models"
  cp "$DL/models/$m" "$EXTRA/usr/share/aios/models/default.gguf"
  echo "$m" > "$EXTRA/usr/share/aios/models/default.name"
  cp "$DL/models/LICENSE.Qwen3-upstream.txt" "$EXTRA/usr/share/aios/models/LICENSE"
  echo "including model $m"
else
  echo "warning: no model in $DL/models; the built-in AI will not start" >&2
fi
for bin in aiosd aios-shell; do
  if [ -x "target/release/$bin" ]; then
    install -m 0755 "target/release/$bin" "$EXTRA/usr/bin/$bin"
    echo "including $bin"
  fi
done

# 2. Root filesystem tree.
mkosi -C "$SRC" --output-dir="$ROOT/build" --cache-dir="$ROOT/build/mkosi-cache" \
  --extra-tree="$ROOT/$EXTRA" --force build

# Binaries built outside the package manager must find all their libraries.
for f in build/rootfs/usr/bin/labwc build/rootfs/usr/lib/aios/llama/bin/*; do
  [ -f "$f" ] || continue
  if chroot build/rootfs ldd "${f#build/rootfs}" 2>&1 | grep "not found"; then
    echo "${f#build/rootfs}: missing libraries in the image (add the package to mkosi.conf)" >&2
    exit 1
  fi
done

# 3. Test variant: same tree plus an autologin root shell on ttyS0 so tests
#    can run commands inside the VM. Never shipped.
rm -rf build/rootfs-test
cp -a build/rootfs build/rootfs-test
DROPIN=build/rootfs-test/etc/systemd/system/serial-getty@ttyS0.service.d
mkdir -p "$DROPIN"
cat > "$DROPIN/autologin.conf" <<'UNIT'
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin root --noclear --keep-baud 115200 %I $TERM
UNIT
mkdir -p build/rootfs-test/etc/systemd/system/getty.target.wants
ln -sf /usr/lib/systemd/system/serial-getty@.service \
  build/rootfs-test/etc/systemd/system/getty.target.wants/serial-getty@ttyS0.service
echo "aios-test" > build/rootfs-test/etc/aios/TEST_IMAGE
# Test-only helpers (tests/testkit) go into the test tree only.
for bin in aios-testkit-events aios-testkit-client aios-sandbox-probe; do
  if [ -x "target/release/$bin" ]; then
    install -D -m 0755 "target/release/$bin" "build/rootfs-test/usr/local/libexec/aios-test/$bin"
  fi
done

# 4. Anti-fake guard on the product tree, then pack both read-only.
scripts/check-no-fake.sh --rootfs build/rootfs
for v in rootfs rootfs-test; do
  rm -f "build/$v.erofs"
  mkfs.erofs -zlz4hc -T0 --quiet "build/$v.erofs" "build/$v/"
  ls -lh "build/$v.erofs"
done
