#!/usr/bin/env python3
"""T8: boot the *product* image on the storage controllers real machines
use (NVMe, AHCI/SATA, USB mass storage on xHCI), and boot the USB image on
a 16 GB "stick" to check that the data partition grows on first boot.

The product image has no shell, so "reached the desktop" is judged from
screenshots: the pastel desktop with the shell's top bar has thousands of
colours, firmware and console screens a few hundred at most. The growth
check reads the partition table and the ext4 superblock of the disk file
after the VM powered off.

    python3 tests/boot/hw_variants.py
"""
import json
import os
import shutil
import struct
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from vm import VM, REPO, KVM, SLOW  # noqa: E402

EV = os.path.join(REPO, "tests", "evidence", "T8")
DESKTOP_COLOURS = 2000


def wait_desktop(vm, name, timeout):
    """Poll screenshots until the desktop is up; save the last one."""
    t0 = time.monotonic()
    png = os.path.join(EV, f"{name}.png")
    colours = 0
    while time.monotonic() - t0 < timeout * SLOW:
        time.sleep(5)
        try:
            colours = vm.screenshot(png)
        except Exception:  # the display may not exist yet
            continue
        if colours > DESKTOP_COLOURS:
            time.sleep(10)  # let the top bar finish drawing
            colours = vm.screenshot(png)
            return True, round(time.monotonic() - t0, 1), colours
    return False, round(time.monotonic() - t0, 1), colours


def ext4_blocks(img, offset):
    with open(img, "rb") as f:
        f.seek(offset + 1024)
        sb = f.read(1024)
    lo, = struct.unpack_from("<I", sb, 4)
    log_bs, = struct.unpack_from("<I", sb, 24)
    hi, = struct.unpack_from("<I", sb, 0x150)
    magic, = struct.unpack_from("<H", sb, 0x38)
    if magic != 0xEF53:
        return None
    return ((hi << 32) | lo) * (1024 << log_bs)


def ext4_after_fsck(img, offset, size):
    """Copy the data partition out (sparse), let e2fsck replay its journal,
    then read the size from the superblock."""
    part = img + ".data"
    subprocess.run(["dd", f"if={img}", f"of={part}", "bs=4M", "iflag=skip_bytes,count_bytes",
                    f"skip={offset}", f"count={size}", "conv=sparse", "status=none"], check=True)
    try:
        fsck = subprocess.run(["e2fsck", "-fy", part], capture_output=True, text=True)
        if fsck.returncode >= 4:
            return None
        return ext4_blocks(part, 0)
    finally:
        os.unlink(part)


def partitions(img):
    out = subprocess.run(["sfdisk", "-J", img], capture_output=True, text=True, check=True).stdout
    return json.loads(out)["partitiontable"]["partitions"]


def main():
    os.makedirs(EV, exist_ok=True)
    res = {"accel": "kvm" if KVM else "tcg (emulated)", "variants": {}}
    image = os.path.join(REPO, "build", "aios.img")
    for iface in ("nvme", "ahci", "usb"):
        vm = VM(image, f"hw-{iface}", disk_if=iface).start()
        try:
            ok, secs, colours = wait_desktop(vm, f"desktop-{iface}", 30)
        finally:
            vm.stop()
        res["variants"][iface] = {"desktop": ok, "seconds": secs, "colours": colours}
        print(iface, res["variants"][iface], flush=True)

    # The USB image on a 16 GB stick (sparse file), booted for real (no snapshot).
    usb = os.path.join(REPO, "build", "aios-usb.img")
    stick = os.path.join(REPO, "build", "usb-stick-16g.img")
    shutil.copyfile(usb, stick)
    before = partitions(stick)
    os.truncate(stick, 16 * 1000 ** 3)
    subprocess.run(["sfdisk", "--relocate", "gpt-bak-std", stick], check=True, capture_output=True)
    vm = VM(stick, "hw-usb-stick", disk_if="usb", snapshot=False).start()
    try:
        ok, secs, colours = wait_desktop(vm, "desktop-usb-stick", 30)
        time.sleep(30)
        vm.monitor("system_powerdown")
        try:
            vm.proc.wait(180)
            res["usb_stick_clean_poweroff"] = True
        except subprocess.TimeoutExpired:
            # Ended by killing QEMU; the e2fsck below replays the journal.
            res["usb_stick_clean_poweroff"] = False
    finally:
        vm.stop()
    after = partitions(stick)
    sector = 512
    data_before, data_after = before[2]["size"] * sector, after[2]["size"] * sector
    fs_after = ext4_after_fsck(stick, after[2]["start"] * sector, after[2]["size"] * sector)
    res["usb_stick"] = {
        "desktop": ok, "seconds": secs, "colours": colours,
        "data_partition_bytes_before": data_before, "data_partition_bytes_after": data_after,
        "ext4_bytes_after": fs_after,
        "grew": data_after > data_before * 4 and fs_after is not None and fs_after > data_before * 4,
    }
    os.unlink(stick)
    res["pass"] = all(v["desktop"] for v in res["variants"].values()) and res["usb_stick"]["grew"]
    with open(os.path.join(EV, "results.json"), "w") as f:
        json.dump(res, f, indent=2)
    print(json.dumps(res, indent=2))
    sys.exit(0 if res["pass"] else 1)


if __name__ == "__main__":
    main()
