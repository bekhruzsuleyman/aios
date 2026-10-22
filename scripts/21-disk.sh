#!/usr/bin/env bash
# T3b: assemble the bootable GPT disk images without loop mounts.
#   build/aios.img       product image (no serial shell)
#   build/aios-test.img  test image (root shell on ttyS0; never shipped)
#   build/aios-usb.img   product image for a USB stick (T8): same layout; the
#                        data partition grows to fill the stick on first boot
#                        (systemd-repart + x-systemd.growfs)
# Layout: ESP (FAT32, systemd-boot + UKI) | root (erofs, read-only) | data (ext4)
set -euo pipefail
cd "$(dirname "$0")/.."
B=build
SECTOR=512
ROOT_PARTUUID=a1050000-0000-4000-8000-000000000002
DATA_PARTUUID=a1050000-0000-4000-8000-000000000003
ESP_MB=256
DATA_MB=${DATA_MB:-1024}
SDBOOT=/usr/lib/systemd/boot/efi/systemd-bootx64.efi
CMDLINE_BASE="root=PARTUUID=$ROOT_PARTUUID ro rootwait rootfstype=erofs quiet loglevel=3 printk.time=1 systemd.show_status=0"

[ -f $B/bzImage ] || { echo "build/bzImage missing (run scripts/13-kernel-build.sh)" >&2; exit 1; }

# Data partition: user/ and notes/ owned by the desktop user (uid 1000).
rm -rf $B/data-tree && mkdir -p $B/data-tree/user $B/data-tree/notes
chown -R 1000:1000 $B/data-tree/user $B/data-tree/notes
rm -f $B/data.img
mkfs.ext4 -q -L aios-data -U "$DATA_PARTUUID" -d $B/data-tree $B/data.img "${DATA_MB}M"

make_image() {
  local variant=$1 rootfs=$2 cmdline=$3 out=$4
  local uki=$B/aios-$variant.efi esp=$B/esp-$variant.img

  ukify build --linux $B/bzImage --cmdline "$cmdline" --os-release @"$B/rootfs/usr/lib/os-release" \
    --uname "$(cat third_party/KERNEL_VERSION)" --output "$uki" >/dev/null

  rm -f "$esp"
  mkfs.vfat -F 32 -n AIOS-ESP -C "$esp" $((ESP_MB * 1024)) >/dev/null
  mmd -i "$esp" ::/EFI ::/EFI/BOOT ::/EFI/Linux ::/loader
  mcopy -i "$esp" "$SDBOOT" ::/EFI/BOOT/BOOTX64.EFI
  mcopy -i "$esp" "$uki" ::/EFI/Linux/aios.efi
  printf 'timeout 0\ndefault aios.efi\neditor no\n' > $B/loader.conf
  mcopy -i "$esp" $B/loader.conf ::/loader/loader.conf

  # Sizes in sectors, partitions 1 MiB aligned.
  local mib=$((1024 * 1024 / SECTOR))
  local esp_s=$((ESP_MB * mib))
  local root_bytes; root_bytes=$(stat -c %s "$rootfs")
  local root_s=$(( (root_bytes / (1024 * 1024) + 16) * mib ))
  local data_s=$((DATA_MB * mib))
  local p1=$mib p2=$((mib + esp_s)) p3=$((mib + esp_s + root_s))
  local total=$((p3 + data_s + mib))

  rm -f "$out"
  truncate -s $((total * SECTOR)) "$out"
  sfdisk -q "$out" <<SFDISK
label: gpt
unit: sectors
start=$p1, size=$esp_s, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, name="ESP"
start=$p2, size=$root_s, type=4F68BCE3-E8CD-4DB1-96E7-FBCAF984B709, uuid=$ROOT_PARTUUID, name="aios-root", attrs=GUID:60
start=$p3, size=$data_s, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, uuid=$DATA_PARTUUID, name="aios-data"
SFDISK
  dd if="$esp" of="$out" bs=$SECTOR seek=$p1 conv=notrunc,sparse status=none
  dd if="$rootfs" of="$out" bs=$SECTOR seek=$p2 conv=notrunc,sparse status=none
  dd if=$B/data.img of="$out" bs=$SECTOR seek=$p3 conv=notrunc,sparse status=none
  echo "$out: $(du -h --apparent-size "$out" | cut -f1)"
  if [ "$variant" = usb ]; then
    local max=$((14 * 1000 * 1000 * 1000))  # a "16 GB" stick holds ~14.9 GB
    [ "$(stat -c %s "$out")" -lt "$max" ] || { echo "$out is too big for a 16 GB stick" >&2; exit 1; }
  fi
}

make_image product $B/rootfs.erofs "$CMDLINE_BASE" $B/aios.img
make_image usb $B/rootfs.erofs "$CMDLINE_BASE" $B/aios-usb.img
make_image test $B/rootfs-test.erofs "$CMDLINE_BASE console=tty0 console=ttyS0,115200" $B/aios-test.img
sfdisk -d $B/aios-test.img
