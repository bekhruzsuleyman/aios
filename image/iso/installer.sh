# Sourced by /init when the "Install Alethic" boot entry was picked. Writes the
# same layout as build/aios-usb.img (ESP | erofs root | data) to a disk; the
# data partition grows to fill the disk on first boot (systemd-repart).
# Nothing is written until the user types the disk's name and "yes".
. /cd/aios/install/layout.env   # P1 P2 P3 (start sectors, 1 MiB aligned), MIN_BYTES

exec < /dev/console > /dev/console 2>&1
clear
echo "================  Install Alethic  ================"
echo
echo "This writes Alethic to a whole disk. Everything on that disk is erased."
echo

cdbase=$(basename "$found")
disks=
for d in /sys/block/*; do
  name=$(basename "$d")
  case "$name" in loop*|ram*|sr*|zram*|"$cdbase") continue ;; esac
  sectors=$(cat "$d/size")
  [ "$((sectors * 512))" -ge "$MIN_BYTES" ] || continue
  model=$(cat "$d/device/model" 2>/dev/null | sed 's/ *$//')
  echo "  $name    $((sectors / 2048 / 1024)) GB    ${model:-disk}"
  disks="$disks $name"
done
echo

[ -n "$disks" ] || fail "no disk of at least $((MIN_BYTES / 1024 / 1024 / 1024)) GB was found."

while :; do
  printf "Type the disk to install to (e.g. %s), or 'off' to power off: " $(echo $disks | cut -d' ' -f1)
  read -r target
  [ "$target" = off ] && poweroff -f
  case " $disks " in *" $target "*) break ;; esac
  echo "  '$target' is not one of the disks listed above."
done

echo
echo "ALL DATA ON /dev/$target WILL BE ERASED."
printf "Type yes to continue: "
read -r answer
[ "$answer" = yes ] || fail "cancelled; nothing was written."

dev=/dev/$target
echo
echo "Writing Alethic to $dev ..."
step() { echo "  $1"; dd if="$2" of="$dev" bs=1M seek="$3" conv=fsync 2>/dev/null || fail "writing $1 failed."; }
# Clear old signatures at the start of the disk first.
dd if=/dev/zero of="$dev" bs=1M count=8 conv=fsync 2>/dev/null
step "partition table" /cd/aios/install/head.bin 0
step "boot partition"  /cd/aios/install/esp.img  $((P1 / 2048))
step "system (about 1 GB, takes a minute)" /cd/aios/rootfs.erofs $((P2 / 2048))
step "data partition"  /cd/aios/install/data.bin $((P3 / 2048))
sync

echo
echo "Done. Alethic is installed on $dev."
echo "Remove the CD/ISO (in VirtualBox: Devices > Optical Drives > Remove disk),"
printf "then press Enter to restart. "
read -r _
reboot -f
