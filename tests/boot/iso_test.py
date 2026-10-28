#!/usr/bin/env python3
"""The ISO in a VirtualBox-like VM (e1000, AHCI, PS/2
keyboard, USB tablet; virtio display, see VBOX=1 in scripts/22-run-qemu.sh):

  1. "Try Alethic" boots the desktop from the CD
  2. "Install Alethic" writes Alethic to an empty 8 GB disk (disk name and
     "yes" typed as keys)
  3. the installed disk boots to the desktop with the CD removed

The ISO is the product build (no serial shell), so every check is a
screenshot; colour counts tell a drawn desktop from a blank or text screen.

    python3 tests/boot/iso_test.py [build/aios.iso]
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from vm import VM, REPO, SLOW  # noqa: E402

ISO = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO, "build", "aios.iso")
EV = os.path.join(REPO, "tests", "evidence", "ISO")
DISK = os.path.join(REPO, "build", "iso-target.img")
INSTALL_ISO = os.path.join(REPO, "build", "aios-installtest.iso")
os.makedirs(EV, exist_ok=True)
os.environ["VBOX"] = "1"
os.environ["GPU"] = "virtio"  # see 22-run-qemu.sh
os.environ["MEM"] = os.environ.get("MEM", "2G")
DESKTOP_COLOURS = 2000  # the pastel desktop has thousands; text screens < 100


def shot(vm, name):
    n = vm.screenshot(os.path.join(EV, name))
    print(f"screenshot {name}: {n} colours", flush=True)
    return n


def wait_desktop(vm, name, timeout):
    deadline = time.time() + timeout * SLOW
    while time.time() < deadline:
        time.sleep(15)
        if shot(vm, name) > DESKTOP_COLOURS:
            time.sleep(20)  # let the shell finish drawing
            return shot(vm, name)
    raise TimeoutError(f"no desktop within {timeout * SLOW}s (see {name})")


def boot(cd, iso=ISO):
    if cd:
        os.environ["CDROM"] = iso
    else:
        os.environ.pop("CDROM", None)
    return VM(DISK, "iso", snapshot=False, disk_if="ahci").start()


with open(DISK, "wb") as f:
    f.truncate(8 * 1024**3)

# 1. Live session (default entry).
vm = boot(cd=True)
try:
    wait_desktop(vm, "1-live-desktop.png", 300)
finally:
    vm.stop()

# 2. Installer. Picking the menu entry by key is timing-sensitive under
# emulation, so this boot uses the same ISO rebuilt with "Install Alethic" as
# the default entry (ISO_DEFAULT in scripts/23-iso.sh).
subprocess.run([os.path.join(REPO, "scripts", "23-iso.sh")], check=True,
               env=dict(os.environ, ISO_DEFAULT="install", ISO_OUT=INSTALL_ISO))
vm = boot(cd=True, iso=INSTALL_ISO)
try:
    time.sleep(24 * SLOW)  # the installer waits for input
    shot(vm, "3-installer.png")
    vm.type("sda")
    vm.sendkey("ret")
    time.sleep(3)
    vm.type("yes")
    vm.sendkey("ret")
    time.sleep(60 * SLOW)  # it waits for Enter when done
    shot(vm, "4-installed.png")
finally:
    vm.stop()

# 3. Boot the installed disk without the CD.
vm = boot(cd=False)
try:
    wait_desktop(vm, "5-installed-desktop.png", 300)
finally:
    vm.stop()
print("PASS")
