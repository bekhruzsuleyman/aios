#!/usr/bin/env bash
# defconfig + kvm_guest.config + our fragment(s). Extra fragments (e.g. the
# hardware one in T8) can be passed as arguments.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
KVER=$(cat third_party/KERNEL_VERSION)
cd "third_party/linux-$KVER"
make -s defconfig
make -s kvm_guest.config
# hardware.fragment (T8) is included when present.
FRAGS=("$ROOT/image/kernel/aios.fragment")
[ -f "$ROOT/image/kernel/hardware.fragment" ] && FRAGS+=("$ROOT/image/kernel/hardware.fragment")
scripts/kconfig/merge_config.sh -m .config "${FRAGS[@]}" "$@"
make -s olddefconfig
"$ROOT/scripts/12-kernel-verify-config.sh" "${FRAGS[@]:1}" "$@"
