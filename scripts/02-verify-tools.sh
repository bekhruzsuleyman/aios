#!/usr/bin/env bash
# Every required host tool answers --version; OVMF firmware is present.
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$PATH:$HOME/.cargo/bin:$PWD/build/venv/bin"
fail=0
check() {
  local name=$1; shift
  local v
  if v=$("$@" 2>&1); then echo "$name: ${v%%$'\n'*}"; else echo "MISSING: $name"; fail=1; fi
}
check gcc gcc --version
check make make --version
check bc bc --version
check flex flex --version
check bison bison --version
check qemu qemu-system-x86_64 --version
check mkfs.erofs sh -c "mkfs.erofs -V 2>&1 | grep -o 'mkfs.erofs [0-9.]*'"
check ukify ukify --version
check mkosi mkosi --version
check cargo cargo --version
check rustc rustc --version
check bwrap bwrap --version
check sfdisk sfdisk --version
check mcopy mcopy --version
check mkfs.ext4 mke2fs -V
OVMF_CODE=$(ls /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd 2>/dev/null | head -1)
SDBOOT=$(ls /usr/lib/systemd/boot/efi/systemd-bootx64.efi 2>/dev/null)
STUB=$(ls /usr/lib/systemd/boot/efi/linuxx64.efi.stub 2>/dev/null)
[ -n "$OVMF_CODE" ] && echo "ovmf: $OVMF_CODE" || { echo "MISSING: OVMF"; fail=1; }
[ -n "$SDBOOT" ] && echo "systemd-boot: $SDBOOT" || { echo "MISSING: systemd-boot EFI binary"; fail=1; }
[ -n "$STUB" ] && echo "uki stub: $STUB" || { echo "MISSING: UKI stub"; fail=1; }
exit $fail
