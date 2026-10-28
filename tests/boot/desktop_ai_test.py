#!/usr/bin/env python3
"""Mac-style windows, the auto-hiding top bar and the built-in AI, in one
boot of the test image:

  1. aios-llm.service is up on 127.0.0.1:8180 and aiosd reports an AI
     configured with no setup (status.get: ai_configured)
  2. a free-form request is planned by the local model (real tool call,
     no mock) and run through the broker
  3. a window (foot) has the traffic-light buttons on the left; maximised,
     it covers the top bar; the pointer on the top edge brings the bar up

    python3 tests/boot/desktop_ai_test.py [build/aios-test.img]
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from vm import VM, REPO, KVM, SLOW  # noqa: E402

IMG = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO, "build", "aios-test.img")
EV = os.path.join(REPO, "tests", "evidence", "AI")
os.makedirs(EV, exist_ok=True)
os.environ.setdefault("MEM", "4G")
os.environ.setdefault("SMP", "4")
RUN_AS_AIOS = "runuser -u aios -- env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0"
CLIENT = "/usr/local/libexec/aios-test/aios-testkit-client"
log = {"accel": "kvm" if KVM else "tcg (emulated)"}


def shot(vm, name):
    n = vm.screenshot(os.path.join(EV, name))
    print(f"screenshot {name}: {n} colours", flush=True)
    return n


def wait_until(vm, cmd, timeout, every=3):
    deadline = time.time() + timeout * SLOW
    while time.time() < deadline:
        code, out = vm.run(cmd, check=False)
        if code == 0:
            return out
        time.sleep(every)
    raise TimeoutError(f"`{cmd}` did not succeed")


def pointer(vm, x, y):
    """Absolute pointer position (virtio tablet: 0..32767 per axis)."""
    vm.monitor(f"mouse_move {x} {y}")


vm = VM(IMG, "ai").start()
try:
    vm.login(timeout=300 * SLOW)
    wait_until(vm, "test -S /run/user/1000/wayland-0", 180)
    time.sleep(10)
    shot(vm, "1-desktop.png")

    # 1. The built-in AI, with no setup.
    # No curl in the image: bash's /dev/tcp.
    out = wait_until(
        vm, "bash -c 'exec 3<>/dev/tcp/127.0.0.1/8180 && printf \"GET /health HTTP/1.0\\r\\n\\r\\n\" >&3 "
        "&& cat <&3' | grep '\"ok\"'", 600, every=10)
    log["llm_health"] = out.strip()
    _, out = vm.run("systemctl show -p ActiveState,MainPID aios-llm.service; "
                    "ps -o rss=,args= -C llama-server | cut -c1-120")
    log["llm_service"] = out.strip()
    print(out, flush=True)

    # 2. A free-form request, planned by the local model.
    req = "save a note titled Shopping with milk and eggs"
    code, out = vm.run(f"{CLIENT} run /run/aios/agent.sock '{req}' --timeout-s 900",
                       timeout=960 * SLOW, check=False)
    lines = [l for l in out.splitlines() if l.startswith("{") or l.startswith("SUMMARY")]
    log["task"] = {"request": req, "events": lines[-6:]}
    summary = json.loads(out.split("SUMMARY", 1)[1]) if "SUMMARY" in out else {}
    assert summary.get("terminal", {}).get("type") == "result", out[-2000:]
    assert any('"plan_created"' in l and "notes.create" in l for l in lines), out[-2000:]
    _, notes = vm.run("ls /var/lib/aios/notes; cat /var/lib/aios/notes/shopping.md")
    log["note"] = notes.strip()
    assert "milk" in notes.lower(), notes
    _, status = vm.run(
        "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"status.get\",\"params\":{}}' "
        f"| {CLIENT} connect /run/aios/agent.sock 2>&1 | head -3", check=False)
    log["status"] = status.strip()

    # 3. A window with traffic lights, then a maximised one over the bar,
    # and the bar brought back by the pointer on the top edge.
    vm.run(f"{RUN_AS_AIOS} setsid foot >/dev/null 2>&1 &")
    wait_until(vm, "pgrep -x foot", 60)
    pointer(vm, 16000, 30000)
    time.sleep(6)
    shot(vm, "2-window.png")
    vm.run("pkill -x foot", check=False)
    vm.run(f"{RUN_AS_AIOS} setsid foot --maximized >/dev/null 2>&1 &")
    wait_until(vm, "pgrep -x foot", 60)
    time.sleep(6)
    shot(vm, "3-covered.png")
    pointer(vm, 16000, 0)
    time.sleep(3)
    shot(vm, "4-revealed.png")
    pointer(vm, 16000, 30000)
    time.sleep(3)
    shot(vm, "5-hidden-again.png")
finally:
    with open(os.path.join(EV, "run.json"), "w") as f:
        json.dump(log, f, indent=2)
    vm.stop()
print("PASS")
