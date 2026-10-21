#!/usr/bin/env bash
# Rewrite the data partition of a disk image (or block device) with a fresh
# empty one: all notes, user files, settings and the audit log are erased.
#   scripts/factory-reset.sh build/aios.img
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET=${1:?usage: factory-reset.sh <image-or-device>}
DATA_PARTUUID=a1050000-0000-4000-8000-000000000003
line=$(sfdisk -d "$TARGET" | grep -i "uuid=$DATA_PARTUUID") || { echo "no AiOS data partition on $TARGET" >&2; exit 1; }
start=$(echo "$line" | sed -E 's/.*start= *([0-9]+).*/\1/')
size=$(echo "$line" | sed -E 's/.*size= *([0-9]+).*/\1/')
echo "erasing AiOS data partition on $TARGET (sectors $start+$size)"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/tree/user" "$tmp/tree/notes"
chown -R 1000:1000 "$tmp/tree/user" "$tmp/tree/notes"
mkfs.ext4 -q -L aios-data -U "$DATA_PARTUUID" -d "$tmp/tree" "$tmp/data.img" $((size / 2))K
dd if="$tmp/data.img" of="$TARGET" bs=512 seek="$start" conv=notrunc status=none
echo "done"
