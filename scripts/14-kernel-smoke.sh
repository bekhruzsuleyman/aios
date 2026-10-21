#!/usr/bin/env bash
# Boot the bare kernel with no root: it must reach the expected
# "VFS: Unable to mount root fs" panic. Log: logs/kernel-smoke.log
set -uo pipefail
cd "$(dirname "$0")/.."
ACCEL=tcg; [ -w /dev/kvm ] && ACCEL=kvm
timeout "${SMOKE_TIMEOUT:-600}" qemu-system-x86_64 -accel $ACCEL -m 1G -nographic \
  -kernel build/bzImage -append "console=ttyS0 panic=1" -no-reboot > logs/kernel-smoke.log 2>&1
if grep -q "Kernel panic - not syncing: VFS: Unable to mount root fs" logs/kernel-smoke.log; then
  echo "smoke OK: $(grep -ao -m1 'Linux version [^ ]*' logs/kernel-smoke.log)"
else
  echo "smoke FAILED"; tail -20 logs/kernel-smoke.log; exit 1
fi
