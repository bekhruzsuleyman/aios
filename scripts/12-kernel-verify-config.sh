#!/usr/bin/env bash
# Fail if any symbol requested by a fragment did not end up with the
# requested value in .config (Kconfig silently drops unmet dependencies).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
KVER=$(cat third_party/KERNEL_VERSION)
CONFIG="third_party/linux-$KVER/.config"
fail=0
for frag in "$ROOT/image/kernel/aios.fragment" "$@"; do
  while IFS= read -r line; do
    case "$line" in ''|'#'*) continue ;; esac
    if ! grep -qxF "$line" "$CONFIG"; then
      sym=${line%%=*}
      have=$(grep -E "^(# )?$sym[= ]" "$CONFIG" || echo "$sym missing")
      echo "MISMATCH: want '$line', have '$have'"
      fail=1
    fi
  done < "$frag"
done
[ $fail -eq 0 ] && echo "kernel config OK ($(cat "$ROOT/image/kernel/aios.fragment" "$@" | grep -cv -e "^#" -e "^$") symbols checked)"
exit $fail
