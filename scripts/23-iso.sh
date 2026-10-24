#!/usr/bin/env bash
# Bootable UEFI ISO (build/aios.iso) for VMs such as VirtualBox, with two
# boot entries:
#   Try AiOS      the desktop runs from the CD; changes live in RAM
#   Install AiOS  writes build/aios-install.img's layout to a disk the user
#                 picks (image/iso/installer.sh), which then boots like the
#                 USB image and grows its data partition to fill the disk
# Needs scripts/21-disk.sh first. UEFI only (no BIOS boot).
# ISO_DEFAULT=install makes "Install AiOS" the default entry and ISO_OUT
# names the output; tests/boot/iso_test.py uses both so it needs no key
# timing in the boot menu.
set -euo pipefail
cd "$(dirname "$0")/.."
B=build
W=$B/iso-work
DL=third_party/downloads
INSTALL_IMG=$B/aios-install.img
OUT=${ISO_OUT:-$B/aios.iso}
for f in $B/bzImage $B/rootfs.erofs "$INSTALL_IMG"; do
  [ -f "$f" ] || { echo "$f missing (run scripts/21-disk.sh)" >&2; exit 1; }
done
SDBOOT=/usr/lib/systemd/boot/efi/systemd-bootx64.efi
CMDLINE_COMMON="quiet loglevel=3 printk.time=1 systemd.show_status=0"
# ISO_DEBUG=1: kernel and systemd log on the serial port too (for tests).
[ "${ISO_DEBUG:-0}" = 1 ] && CMDLINE_COMMON="console=tty0 console=ttyS0,115200 printk.time=1"

rm -rf "$W" && mkdir -p "$W/initrd/bin" "$W/cd/aios/install" "$DL"

# busybox (static) for the initramfs, from the distro archive.
deb=$(ls $DL/busybox-static_*_amd64.deb 2>/dev/null | head -1 || true)
if [ -z "$deb" ]; then
  (cd $DL && apt-get download busybox-static >/dev/null)
  deb=$(ls $DL/busybox-static_*_amd64.deb | head -1)
fi
dpkg-deb --fsys-tarfile "$deb" | tar -x -O ./usr/bin/busybox > "$W/initrd/bin/busybox"
chmod 755 "$W/initrd/bin/busybox"
mkdir -p "$W/initrd/dev"
mknod -m 600 "$W/initrd/dev/console" c 5 1
install -m 755 image/iso/init "$W/initrd/init"
install -m 644 image/iso/installer.sh "$W/initrd/installer.sh"
(cd "$W/initrd" && find . | sort | cpio -o -H newc --quiet --owner 0:0) | zstd -q -19 > "$W/initrd.cpio.zst"

# Installer payload: the install image cut at its partition boundaries, so
# the root file system is stored on the CD only once (shared with the live
# session).
read -r P1 P2 P3 < <(sfdisk -J "$INSTALL_IMG" | python3 -c '
import json, sys
p = json.load(sys.stdin)["partitiontable"]["partitions"]
print(*(x["start"] for x in p))')
total=$(stat -c %s "$INSTALL_IMG")
dd if="$INSTALL_IMG" of="$W/cd/aios/install/head.bin" bs=1M count=$((P1 / 2048)) status=none
cp $B/esp-install.img "$W/cd/aios/install/esp.img"
dd if="$INSTALL_IMG" of="$W/cd/aios/install/data.bin" bs=1M skip=$((P3 / 2048)) status=none
cat > "$W/cd/aios/install/layout.env" <<EOF
P1=$P1
P2=$P2
P3=$P3
MIN_BYTES=$((total + 2 * 1024 * 1024 * 1024))
EOF
cp $B/rootfs.erofs "$W/cd/aios/rootfs.erofs"

# Boot image (El Torito, FAT): systemd-boot and one UKI per entry.
osrel() {  # os-release with the menu title
  sed "s/^PRETTY_NAME=.*/PRETTY_NAME=\"$1\"/" $B/rootfs/usr/lib/os-release > "$W/os-release-$2"
}
osrel "Try AiOS" try
osrel "Install AiOS" install
mk_uki() {
  ukify build --linux $B/bzImage --initrd "$W/initrd.cpio.zst" --cmdline "$2" \
    --os-release @"$W/os-release-$1" --uname "$(cat third_party/KERNEL_VERSION)" \
    --output "$W/aios-$1.efi" >/dev/null
}
mk_uki try "aios.live $CMDLINE_COMMON systemd.mask=systemd-repart.service"
mk_uki install "aios.install quiet loglevel=3"
size_kb=$(( ( $(du -k --apparent-size "$W/aios-try.efi" | cut -f1) * 2 + 8192 ) ))
efi=$W/cd/efiboot.img
mkfs.vfat -n AIOS-ISO -C "$efi" "$size_kb" >/dev/null
mmd -i "$efi" ::/EFI ::/EFI/BOOT ::/EFI/Linux ::/loader
mcopy -i "$efi" "$SDBOOT" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$efi" "$W/aios-try.efi" ::/EFI/Linux/aios-1-try.efi
mcopy -i "$efi" "$W/aios-install.efi" ::/EFI/Linux/aios-2-install.efi
default=aios-1-try.efi; [ "${ISO_DEFAULT:-try}" = install ] && default=aios-2-install.efi
printf 'timeout 8\ndefault %s\neditor no\n' "$default" > "$W/loader.conf"
mcopy -i "$efi" "$W/loader.conf" ::/loader/loader.conf

rm -f "$OUT"
xorriso -as mkisofs -quiet -iso-level 3 -V AIOS -R -J \
  -e efiboot.img -no-emul-boot -o "$OUT" "$W/cd"
echo "$OUT: $(du -h --apparent-size "$OUT" | cut -f1)"
