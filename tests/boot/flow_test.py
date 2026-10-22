#!/usr/bin/env python3
"""T4/T5/T6 acceptance in the booted test image, in one boot.

  T4  the shell: top bar, palette, a plan ticking and an approval dialog
      (driven by the test-only event server inside the test image), the
      setup panel, and keys typed through the QEMU monitor.
  T5  the five router commands against the real aiosd, with screenshots
      and audit log lines; a denied read is audited too.
  T6  the AI endpoint entered in the setup panel (typed through the
      monitor), free-form multi-step plans against tests/testkit/mock-ai on
      the host, no model request for router commands, and cancellation.

    python3 tests/boot/flow_test.py [build/aios-test.img] [--only T4,T5,T6]
"""
import json
import os
import re
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from vm import VM, REPO, KVM, SLOW  # noqa: E402

EV = {t: os.path.join(REPO, "tests", "evidence", t) for t in ("T4", "T5", "T6")}
RUN_AS_AIOS = "runuser -u aios -- env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0"
TESTKIT = "/usr/local/libexec/aios-test/aios-testkit-events"
results = {"accel": "kvm" if KVM else "tcg (emulated)"}
notes = {t: [] for t in EV}


def note(task, text):
    print(f"[{task}] {text}", flush=True)
    notes[task].append(text)


def shot(vm, task, name):
    path = os.path.join(EV[task], name)
    colours = vm.screenshot(path)
    note(task, f"screenshot {name}: {colours} colours")
    return colours


def wait_until(vm, cmd, timeout, every=2):
    deadline = time.time() + timeout * SLOW
    while time.time() < deadline:
        code, out = vm.run(cmd, check=False)
        if code == 0:
            return out
        time.sleep(every)
    raise TimeoutError(f"`{cmd}` did not succeed")


def palette(vm):
    """Toggle the palette (Super+Space, the labwc keybind)."""
    vm.sendkey("meta_l-spc")
    time.sleep(1.5 * SLOW / 2)


def fresh_palette(vm):
    """Open the palette, or re-create it so it has keyboard focus again
    (after an app window took it). Super+Space is a toggle, so the tests
    use the shell's `--show-palette` control command here."""
    vm.run("runuser -u aios -- env XDG_RUNTIME_DIR=/run/user/1000 aios-shell --show-palette")
    time.sleep(3)


def ask(vm, text):
    vm.type(text, delay=0.03)
    time.sleep(1)
    vm.sendkey("ret")


def audit_tail(vm, n):
    return vm.run(f"tail -n {n} /var/lib/aios/audit.log", check=False)[1]


def kill_browser(vm):
    vm.run("pkill -f epiphany >/dev/null 2>&1; pkill -x bwrap >/dev/null 2>&1; sleep 2; true", check=False)


def t4(vm):
    os.makedirs(EV["T4"], exist_ok=True)
    shot(vm, "T4", "topbar.png")
    # Swap the real daemon for the test-only event server (same sockets).
    vm.run("systemctl stop aiosd")
    vm.run(f"(umask 0007; runuser -u aiosd -- {TESTKIT} /run/aios/agent.sock /run/aios/approve.sock 2500 "
           ">/tmp/testkit.log 2>&1 &)")
    wait_until(vm, "test -S /run/aios/agent.sock && test -S /run/aios/approve.sock", 30)
    time.sleep(4)  # the shell reconnects with a backoff of at most 2 s
    palette(vm)
    vm.type("plan an evening", delay=0.03)
    time.sleep(2)
    shot(vm, "T4", "palette-typed.png")
    vm.sendkey("ret")
    time.sleep(4)
    shot(vm, "T4", "plan-ticking.png")
    # The testkit's third step needs approval (2.5 s per step).
    time.sleep(8)
    shot(vm, "T4", "approval.png")
    vm.sendkey("alt-a")
    time.sleep(12)
    shot(vm, "T4", "result.png")
    vm.sendkey("alt-s")
    time.sleep(3)
    shot(vm, "T4", "setup-panel.png")
    vm.sendkey("esc")
    time.sleep(1)
    vm.sendkey("esc")  # hide the palette
    note("T4", "testkit log:\n" + vm.run("cat /tmp/testkit.log", check=False)[1])
    vm.run("pkill -x aios-testkit-ev; sleep 1; rm -f /run/aios/agent.sock /run/aios/approve.sock; "
           "systemctl start aiosd", check=False)
    wait_until(vm, "systemctl is-active aiosd && test -S /run/aios/agent.sock", 60)
    time.sleep(4)


def t5(vm):
    os.makedirs(EV["T5"], exist_ok=True)
    vm.run("printf 'buy milk\\ncall mom\\nwrite the report\\n' > /var/lib/aios/user/todo.txt; "
           "chown aios:aios /var/lib/aios/user/todo.txt")
    shot(vm, "T5", "ready.png")

    # 1. open browser: asks first (app.launch is `ask`), then a sandboxed browser.
    fresh_palette(vm)
    ask(vm, "open browser")
    time.sleep(8)
    shot(vm, "T5", "open-browser-approval.png")
    vm.sendkey("alt-a")
    out = wait_until(vm, "pgrep -a -x epiphany", 120)
    note("T5", "browser process: " + out)
    note("T5", "browser runs inside bwrap: " + vm.run(
        "p=$(pgrep -o -x epiphany); ps -o pid,ppid,user,args -p $p; "
        "ls /proc/$p/root/run/ ; echo pidns=$(readlink /proc/$p/ns/pid) host=$(readlink /proc/1/ns/pid); "
        "test -e /proc/$p/root/run/aios && echo 'SEES /run/aios' || echo 'no /run/aios inside'; "
        "test -e /proc/$p/root/var/lib/aios/audit.log && echo 'SEES data' || echo 'no data partition inside'",
        check=False)[1])
    time.sleep(40 if not KVM else 5)
    shot(vm, "T5", "open-browser.png")
    note("T5", "audit:\n" + audit_tail(vm, 3))
    kill_browser(vm)

    # 2. search: browser on DuckDuckGo with the query.
    fresh_palette(vm)
    ask(vm, "search rust language")
    out = wait_until(vm, "pgrep -a -x epiphany", 120)
    note("T5", "search browser: " + out)
    time.sleep(40 if not KVM else 5)
    shot(vm, "T5", "search.png")
    note("T5", "audit:\n" + audit_tail(vm, 2))
    kill_browser(vm)

    # 3. play: browser on a YouTube search.
    fresh_palette(vm)
    ask(vm, "play lofi beats")
    out = wait_until(vm, "pgrep -a -x epiphany", 120)
    note("T5", "play browser: " + out)
    time.sleep(40 if not KVM else 5)
    shot(vm, "T5", "play.png")
    note("T5", "audit:\n" + audit_tail(vm, 2))
    kill_browser(vm)

    # 4. note: a real file in the notes directory.
    fresh_palette(vm)
    ask(vm, "note: buy milk and eggs")
    out = wait_until(vm, "ls -l /var/lib/aios/notes/buy-milk-and-eggs.md && cat /var/lib/aios/notes/buy-milk-and-eggs.md", 60)
    note("T5", "note file:\n" + out)
    time.sleep(3)
    shot(vm, "T5", "note.png")
    note("T5", "audit:\n" + audit_tail(vm, 2))

    # 5. read: the content is shown in the palette.
    ask(vm, "read todo.txt")
    time.sleep(8)
    shot(vm, "T5", "read.png")
    note("T5", "audit:\n" + audit_tail(vm, 2))

    # Denied: outside read_roots, still audited.
    ask(vm, "read /etc/shadow")
    time.sleep(8)
    shot(vm, "T5", "read-denied.png")
    a = audit_tail(vm, 1)
    note("T5", "denied audit:\n" + a)
    results["t5_denied_audited"] = '"decision":"denied"' in a and "/etc/shadow" in a
    vm.sendkey("esc")


def start_mock_ai():
    log = os.path.join(EV["T6"], "mock-ai-requests.jsonl")
    if os.path.exists(log):
        os.unlink(log)
    proc = subprocess.Popen([sys.executable, os.path.join(REPO, "tests", "testkit", "mock-ai", "mock_ai.py"),
                             "--host", "0.0.0.0", "--port", "8081", "--log", log],
                            stdout=subprocess.PIPE, stderr=open(os.path.join(REPO, "logs", "mock-ai.log"), "w"))
    if not proc.stdout.readline():
        raise RuntimeError("mock-ai did not start (is port 8081 taken?)")
    return proc, log


def requests(log):
    try:
        return sum(1 for _ in open(log))
    except FileNotFoundError:
        return 0


def t6(vm):
    os.makedirs(EV["T6"], exist_ok=True)
    mock, log = start_mock_ai()
    try:
        vm.run("rm -f /var/lib/aios/ai.toml /var/lib/aios/ai.key")
        fresh_palette(vm)
        time.sleep(2)
        shot(vm, "T6", "not-connected.png")
        vm.sendkey("alt-s")
        time.sleep(2)
        vm.type("10.0.2.2:8081", delay=0.03)
        vm.sendkey("tab")
        vm.type("test-model", delay=0.03)
        vm.sendkey("tab")
        vm.type("sk-test", delay=0.03)
        vm.sendkey("ret")  # Save on the last field
        wait_until(vm, "test -f /var/lib/aios/ai.toml", 60)
        time.sleep(4)
        shot(vm, "T6", "setup-saved.png")
        note("T6", "stored settings:\n" + vm.run(
            "cat /var/lib/aios/ai.toml; ls -l /var/lib/aios/ai.toml /var/lib/aios/ai.key", check=False)[1])
        vm.sendkey("esc")
        time.sleep(1)

        n0 = requests(log)
        ask(vm, "plan my weekend and put on some relaxing music")
        time.sleep(15)
        shot(vm, "T6", "weekend-plan.png")
        out = wait_until(vm, "ls /var/lib/aios/notes/weekend-plan.md", 60)
        note("T6", "weekend note: " + out)
        note("T6", "audit:\n" + audit_tail(vm, 6))
        kill_browser(vm)

        fresh_palette(vm)
        ask(vm, "summarize my todo list")
        time.sleep(15)
        shot(vm, "T6", "todo-summary.png")
        note("T6", "audit:\n" + audit_tail(vm, 2))

        n1 = requests(log)
        ask(vm, "note: router check")
        wait_until(vm, "ls /var/lib/aios/notes/router-check.md", 60)
        time.sleep(2)
        n2 = requests(log)
        results["t6_model_requests_free_form"] = n1 - n0
        results["t6_model_requests_router"] = n2 - n1
        note("T6", f"model requests: free-form {n1 - n0}, router command {n2 - n1}")

        # Cancellation: the slow prompt holds the model request for 30 s.
        ask(vm, "slow-test plan my weekend")
        time.sleep(5)
        shot(vm, "T6", "cancel-before.png")
        vm.sendkey("alt-c")
        time.sleep(4)
        shot(vm, "T6", "cancelled.png")
        tl = vm.run("journalctl -u aiosd -o cat --no-pager | grep -A12 'timeline' | tail -n 14", check=False)[1]
        note("T6", "aiosd timeline after cancel:\n" + tl)
        m = re.search(r"^cancel\s+([\d.]+)", tl, re.M)
        results["t6_cancel_ms"] = float(m.group(1)) if m else None
        note("T6", "aiosd journal:\n" + vm.run("journalctl -u aiosd -o cat --no-pager | tail -n 30", check=False)[1])
    finally:
        mock.terminate()


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = next((a.split("=", 1)[1] for a in sys.argv[1:] if a.startswith("--only=")), "T4,T5,T6").split(",")
    image = args[0] if args else os.path.join(REPO, "build", "aios-test.img")
    for d in EV.values():
        os.makedirs(d, exist_ok=True)
    vm = VM(image, "flow", log_dir=os.path.join(REPO, "logs")).start()
    ok = True
    try:
        vm.login(timeout=60)
        wait_until(vm, "systemctl is-active aiosd", 120)
        wait_until(vm, "pgrep -x aios-shell", 120)
        time.sleep(10)
        for name, fn in (("T4", t4), ("T5", t5), ("T6", t6)):
            if name not in only:
                continue
            try:
                fn(vm)
            except Exception as e:  # keep going; record the failure honestly
                ok = False
                note(name, f"FAILED: {e!r}")
                shot(vm, name, "failure.png")
        results["shell_journal"] = vm.run("journalctl -b -t aios-shell -o cat --no-pager | tail -n 30", check=False)[1]
    finally:
        vm.stop()
        for t, lines in notes.items():
            if t in only:
                with open(os.path.join(EV[t], "flow-notes.txt"), "w") as f:
                    f.write("\n\n".join(lines) + "\n")
        results["pass"] = ok
        with open(os.path.join(REPO, "logs", "flow-results.json"), "w") as f:
            json.dump(results, f, indent=2)
        print(json.dumps(results, indent=2))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
