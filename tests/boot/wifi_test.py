#!/usr/bin/env python3
"""T8: the native Wi-Fi picker against real iwd, in the test image.

mac80211_hwsim gives the VM two simulated radios. iwd runs one of them as
an access point (WPA2, "AiOS-Test"); the shell's picker, driven by keys
typed through the QEMU monitor, finds it, asks for the password and
connects the other radio.

    python3 tests/boot/wifi_test.py [build/aios-test.img]
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from vm import VM, REPO, KVM, SLOW  # noqa: E402

EV = os.path.join(REPO, "tests", "evidence", "T8")
SSID, PASSWORD = "AiOS-Test", "correct-horse-42"


def wait_until(vm, cmd, timeout, every=2):
    deadline = time.time() + timeout * SLOW
    while time.time() < deadline:
        code, out = vm.run(cmd, check=False)
        if code == 0:
            return out
        time.sleep(every)
    raise TimeoutError(cmd)


def main():
    image = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO, "build", "aios-test.img")
    os.makedirs(EV, exist_ok=True)
    res = {"accel": "kvm" if KVM else "tcg (emulated)"}
    vm = VM(image, "wifi", log_dir=os.path.join(REPO, "logs")).start()
    try:
        vm.login(timeout=60)
        wait_until(vm, "pgrep -x aios-shell", 120)
        vm.run("modprobe mac80211_hwsim radios=2")
        res["radios"] = vm.run("ls /sys/class/ieee80211", check=False)[1]
        vm.run("systemctl start iwd")
        wait_until(vm, "iwctl device list | grep -q wlan1", 60)
        vm.run("iwctl device wlan1 set-property Mode ap")
        time.sleep(2)
        vm.run(f"iwctl ap wlan1 start {SSID} {PASSWORD}")
        time.sleep(3)
        vm.run("iwctl station wlan0 scan", check=False)
        wait_until(vm, f"iwctl station wlan0 get-networks | grep -q {SSID}", 60)
        res["iwd_sees"] = vm.run("iwctl station wlan0 get-networks", check=False)[1]
        time.sleep(8)  # the shell polls iwd every 5 s
        vm.run("runuser -u aios -- env XDG_RUNTIME_DIR=/run/user/1000 aios-shell --show-palette")
        time.sleep(3)
        vm.sendkey("alt-w")
        time.sleep(8)
        res["list_colours"] = vm.screenshot(os.path.join(EV, "wifi-list.png"))
        vm.sendkey("ret")  # the first (strongest) network asks for its password
        time.sleep(3)
        vm.type(PASSWORD, delay=0.03)
        time.sleep(1)
        res["password_colours"] = vm.screenshot(os.path.join(EV, "wifi-password.png"))
        vm.sendkey("ret")
        out = wait_until(vm, f"iwctl station wlan0 show | grep -E 'Connected network +{SSID}'", 90)
        res["station"] = vm.run("iwctl station wlan0 show", check=False)[1]
        time.sleep(8)
        res["connected_colours"] = vm.screenshot(os.path.join(EV, "wifi-connected.png"))
        res["connected"] = SSID in out
        res["shell_log"] = vm.run("journalctl -b -t aios-shell -o cat --no-pager | tail -n 20", check=False)[1]
    except Exception as e:
        res["error"] = repr(e)
        res["connected"] = False
        try:
            vm.screenshot(os.path.join(EV, "wifi-failure.png"))
        except Exception:
            pass
    finally:
        vm.stop()
        with open(os.path.join(EV, "wifi-results.json"), "w") as f:
            json.dump(res, f, indent=2)
        print(json.dumps(res, indent=2))
    sys.exit(0 if res.get("connected") else 1)


if __name__ == "__main__":
    main()
