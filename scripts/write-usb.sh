#!/usr/bin/env bash
# Write build/aios-usb.img to a USB stick. FOR THE HUMAN ONLY: this erases
# the whole device. The build agent never runs it.
#
#   sudo scripts/write-usb.sh /dev/sdX
#
# Refuses anything that is not a whole, removable/USB disk, anything with a
# mounted partition, and the disk the running system lives on. Prints what
# it will erase and asks you to type the device name to confirm.
set -euo pipefail
cd "$(dirname "$0")/.."
IMG=build/aios-usb.img
die() { echo "write-usb: $*" >&2; exit 1; }

[ $# -eq 1 ] || die "usage: $0 /dev/sdX   (the exact device path of the whole stick)"
DEV=$1
[ "$(id -u)" -eq 0 ] || die "run as root (sudo)"
[ -f "$IMG" ] || die "$IMG not found (build it with scripts/21-disk.sh)"
case "$DEV" in /dev/*) ;; *) die "give the exact device path, e.g. /dev/sdb" ;; esac
[ -b "$DEV" ] || die "$DEV is not a block device"
NAME=$(basename "$(readlink -f "$DEV")")
[ "$(lsblk -dno TYPE "$DEV")" = disk ] || die "$DEV is not a whole disk (give /dev/sdb, not /dev/sdb1)"

# Not the disk of the running system.
for mp in / /boot /boot/efi /home /usr /var; do
  src=$(findmnt -no SOURCE "$mp" 2>/dev/null || true)
  [ -n "$src" ] || continue
  parent=$(lsblk -no PKNAME "$src" 2>/dev/null | head -1 || true)
  [ "$parent" = "$NAME" ] || [ "$(basename "$src")" = "$NAME" ] && die "$DEV holds $mp of the running system"
done
# Nothing on it may be mounted.
if lsblk -nro MOUNTPOINTS "$DEV" 2>/dev/null | grep -q .; then
  lsblk -o NAME,SIZE,MOUNTPOINTS "$DEV" >&2
  die "$DEV has mounted partitions; unmount them first"
fi
# Must be removable or on the USB bus.
RM=$(cat "/sys/block/$NAME/removable" 2>/dev/null || echo 0)
TRAN=$(lsblk -dno TRAN "$DEV" 2>/dev/null || true)
[ "$RM" = 1 ] || [ "$TRAN" = usb ] || die "$DEV is neither removable nor a USB disk (transport: ${TRAN:-unknown})"

IMG_BYTES=$(stat -c %s "$IMG")
DEV_BYTES=$(blockdev --getsize64 "$DEV")
[ "$DEV_BYTES" -ge "$IMG_BYTES" ] || die "$DEV is smaller ($DEV_BYTES bytes) than the image ($IMG_BYTES bytes)"

echo "This will ERASE EVERYTHING on:"
lsblk -o NAME,MODEL,SIZE,TRAN,RM,FSTYPE,LABEL "$DEV"
echo
echo "and write $IMG ($(numfmt --to=iec "$IMG_BYTES")) to it."
read -r -p "Type the device path again to confirm ($DEV): " answer
[ "$answer" = "$DEV" ] || die "not confirmed; nothing written"

dd if="$IMG" of="$DEV" bs=4M conv=fsync oflag=direct status=progress
sync
echo "Done. The data partition grows to fill the stick on first boot."
echo "Boot it with UEFI (Secure Boot off); see docs/HARDWARE_TEST.md."
