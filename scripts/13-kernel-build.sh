#!/usr/bin/env bash
# Build bzImage (log: logs/kernel-build.log when run via the background wrapper).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
KVER=$(cat third_party/KERNEL_VERSION)
mkdir -p build
cd "third_party/linux-$KVER"
if command -v ccache >/dev/null; then export CC="ccache gcc"; fi
make -j"$(nproc)" CC="${CC:-gcc}" bzImage modules
cp arch/x86/boot/bzImage "$ROOT/build/bzImage"
# Modules (T8) for the root filesystem: build/modules/lib/modules/<kver>.
rm -rf "$ROOT/build/modules"
make -s INSTALL_MOD_PATH="$ROOT/build/modules" INSTALL_MOD_STRIP=1 modules_install
du -sh "$ROOT/build/modules/lib/modules/"*
file "$ROOT/build/bzImage"
