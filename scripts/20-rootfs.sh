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
for bin in aiosd aios-shell; do
  if [ -x "target/release/$bin" ]; then
    install -m 0755 "target/release/$bin" "$EXTRA/usr/bin/$bin"
    echo "including $bin"
  fi
done

# 2. Root filesystem tree.
mkosi -C "$SRC" --output-dir="$ROOT/build" --cache-dir="$ROOT/build/mkosi-cache" \
  --extra-tree="$ROOT/$EXTRA" --force build

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
