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
scripts/kconfig/merge_config.sh -m .config "$ROOT/image/kernel/aios.fragment" "$@"
make -s olddefconfig
"$ROOT/scripts/12-kernel-verify-config.sh" "$@"
