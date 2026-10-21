#!/usr/bin/env python3
"""T3 boot test: boot the test image under UEFI and check, inside the VM,
that our kernel, the read-only erofs root, the data partition and the labwc
session are all there. Saves evidence to tests/evidence/T3/.

    python3 tests/boot/boot_test.py [build/aios-test.img]
"""
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from vm import VM, REPO, KVM  # noqa: E402

EVID = os.path.join(REPO, "tests", "evidence", "T3")


def main():
    image = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO, "build", "aios-test.img")
    kver = open(os.path.join(REPO, "third_party", "KERNEL_VERSION")).read().strip()
    os.makedirs(EVID, exist_ok=True)
    vm = VM(image, "boot", log_dir=EVID).start()
    results = {"accel": "kvm" if KVM else "tcg (emulated)"}
    try:
        vm.login(timeout=60)
        results["host_seconds_to_serial_shell"] = round(time.monotonic() - vm.t0, 1)
        checks = {
            "uname -r": lambda o: o == kver,
            "systemctl is-active aios-session": lambda o: o == "active",
            "pgrep -x labwc": lambda o: o.isdigit(),
            "findmnt -n -o FSTYPE,OPTIONS /": lambda o: o.startswith("erofs") and "ro" in o.split()[1].split(","),
            "findmnt -n -o SOURCE,FSTYPE /var/lib/aios": lambda o: "ext4" in o,
            "stat -c %U /var/lib/aios/notes": lambda o: o == "aios",
            "touch /usr/x 2>&1; echo rc=$?": lambda o: "Read-only file system" in o,
        }
        # labwc may still be starting under emulation.
        for _ in range(60):
            if vm.run("pgrep -x labwc", check=False)[0] == 0:
                break
            time.sleep(2)
        out_lines = []
        ok = True
        for cmd, pred in checks.items():
            code, out = vm.run(cmd, check=False)
            passed = pred(out)
            ok &= passed
            out_lines.append(f"$ {cmd}\n{out}\n=> {'PASS' if passed else 'FAIL'}\n")
        with open(os.path.join(EVID, "in-vm-checks.txt"), "w") as f:
            f.write("\n".join(out_lines))
        print("\n".join(out_lines))

        # Boot timeline from inside the VM.
        sa = vm.run("systemd-analyze 2>&1 | head -1", check=False)[1]
        ready = vm.run("journalctl -b -t aios-session -o cat | grep -m1 'compositor ready'", check=False)[1]
        shell_up = vm.run("journalctl -b -o short-monotonic | grep -m1 'aios-shell: started'", check=False)[1]
        results["systemd_analyze"] = sa
        m = re.search(r"uptime=([\d.]+)", ready)
        results["compositor_ready_uptime_s"] = float(m.group(1)) if m else None
        m = re.search(r"\[\s*([\d.]+)\]", shell_up)
        results["shell_started_uptime_s"] = float(m.group(1)) if m else None

        time.sleep(5)
        colours = vm.screenshot(os.path.join(EVID, "boot.png"))
        results["boot_png_colours"] = colours

        # A real app window on the real desktop: foot, started as the desktop user.
        vm.run("cd /home/aios && runuser -u aios -- env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 "
               "foot --title 'AiOS test window' sh -c 'uname -a; echo; echo hello from foot; sleep 600' >/dev/null 2>&1 &")
        for _ in range(60):
            if vm.run("pgrep -x foot", check=False)[0] == 0:
                break
            time.sleep(2)
        time.sleep(15 if not KVM else 3)
        results["window_png_colours"] = vm.screenshot(os.path.join(EVID, "window.png"))
        results["foot_running"] = vm.run("pgrep -x foot", check=False)[0] == 0
        ok &= results["foot_running"] and colours > 16
        results["pass"] = bool(ok)
    finally:
        vm.stop()
    with open(os.path.join(EVID, "results.json"), "w") as f:
        json.dump(results, f, indent=2)
    print(json.dumps(results, indent=2))
    sys.exit(0 if results.get("pass") else 1)


if __name__ == "__main__":
    main()
