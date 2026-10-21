#!/usr/bin/env bash
# Boot an AiOS disk image under UEFI (OVMF) in QEMU.
#   scripts/22-run-qemu.sh [image]       default build/aios.img
# Env: DISPLAY_MODE=none|gtk (default none), SERIAL=stdio|<file>|unix:<path>
#      ACCEL=kvm|tcg (default: kvm when /dev/kvm is usable)
#      DISK_IF=virtio|nvme|ahci|usb (T8 hardware variants), MEM (default 3G)
#      SNAPSHOT=1 to discard disk writes
set -euo pipefail
cd "$(dirname "$0")/.."
IMG=${1:-build/aios.img}
ACCEL=${ACCEL:-$([ -w /dev/kvm ] && echo kvm || echo tcg)}
OVMF_CODE=$(ls /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd 2>/dev/null | head -1 || true)
OVMF_VARS_SRC=$(ls /usr/share/OVMF/OVMF_VARS_4M.fd /usr/share/OVMF/OVMF_VARS.fd 2>/dev/null | head -1 || true)
VARS=build/OVMF_VARS-$(basename "$IMG" .img).fd
[ -f "$VARS" ] || cp "$OVMF_VARS_SRC" "$VARS"

case "${SERIAL:-stdio}" in
  stdio) SER=(-serial mon:stdio) ;;
  unix:*) SER=(-serial "unix:${SERIAL#unix:},server,nowait") ;;
  *) SER=(-serial "file:$SERIAL") ;;
esac
SNAP=(); [ "${SNAPSHOT:-0}" = 1 ] && SNAP=(-snapshot)
case "${DISK_IF:-virtio}" in
  virtio) DISK=(-drive "file=$IMG,format=raw,if=virtio") ;;
  nvme)   DISK=(-drive "file=$IMG,format=raw,if=none,id=d0" -device nvme,drive=d0,serial=aios0) ;;
  ahci)   DISK=(-drive "file=$IMG,format=raw,if=none,id=d0" -device ahci,id=ahci -device ide-hd,drive=d0,bus=ahci.0) ;;
  usb)    DISK=(-drive "file=$IMG,format=raw,if=none,id=d0" -device qemu-xhci,id=xhci -device usb-storage,drive=d0,bus=xhci.0) ;;
esac
CPU=(); [ "$ACCEL" = kvm ] && CPU=(-cpu host)

exec qemu-system-x86_64 -accel "$ACCEL" "${CPU[@]}" -m "${MEM:-3G}" -smp "${SMP:-2}" -machine q35 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
  -drive "if=pflash,format=raw,file=$VARS" \
  "${DISK[@]}" "${SNAP[@]}" \
  -device virtio-gpu-pci -device virtio-keyboard-pci -device virtio-tablet-pci \
  -nic "user,model=virtio-net-pci${HOSTFWD:-}" \
  -monitor "unix:${MONITOR:-build/qemu-mon.sock},server,nowait" \
  "${SER[@]}" -display "${DISPLAY_MODE:-none}"
