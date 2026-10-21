#!/usr/bin/env bash
# Install host build tools. Idempotent and non-interactive.
# Package names follow the plan (section 4.2); substitutions for this host
# release are listed in DECISIONS.md.
set -euo pipefail
SUDO=""
[ "$(id -u)" -ne 0 ] && SUDO="sudo"
export DEBIAN_FRONTEND=noninteractive

PKGS=(
  build-essential bc bison flex libssl-dev libelf-dev libncurses-dev dwarves
  cpio kmod xz-utils zstd git curl ca-certificates gnupg python3 python3-pip
  python3-venv python3-pil pkg-config clang lld cmake ninja-build
  qemu-system-x86 qemu-utils ovmf mtools dosfstools e2fsprogs fdisk
  erofs-utils squashfs-tools cryptsetup-bin systemd-boot-efi systemd-ukify
  debootstrap libseccomp-dev libasound2-dev
  libwayland-dev wayland-protocols libxkbcommon-dev libfontconfig1-dev
  libfreetype-dev bubblewrap netpbm ccache jq socat expect netcat-openbsd
  ubuntu-keyring apt zstd
)

for i in 1 2 3; do
  $SUDO apt-get update -y && break
  sleep $((i * 5))
done
missing=()
for p in "${PKGS[@]}"; do
  dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "install ok installed" || missing+=("$p")
done
if [ "${#missing[@]}" -gt 0 ]; then
  $SUDO apt-get install -y --no-install-recommends "${missing[@]}"
fi

# mkosi: the distro package is too old for erofs/directory output; use a
# pinned release in a venv under build/.
MKOSI_VERSION=${MKOSI_VERSION:-25.3}
VENV="$(cd "$(dirname "$0")/.." && pwd)/build/venv"
if [ ! -x "$VENV/bin/mkosi" ]; then
  python3 -m venv "$VENV"
  # mkosi is not published on PyPI; install the tagged release from GitHub.
  "$VENV/bin/pip" install -q "git+https://github.com/systemd/mkosi@v$MKOSI_VERSION"
fi

# Rust (stable) if not present.
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal -c rustfmt,clippy
fi
echo "tools installed"
