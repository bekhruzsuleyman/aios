# Checklist

Acceptance checks per task with the command used and a link to evidence.

## T0: Host bootstrap, repo skeleton, anti-fake guard
- [x] `scripts/02-verify-tools.sh` exits 0. Evidence: [tests/evidence/T0/verify-tools.txt](tests/evidence/T0/verify-tools.txt)
- [x] `./check.sh` passes (fmt, clippy, tests, guard, guard self-test). Evidence: [tests/evidence/T0/check.txt](tests/evidence/T0/check.txt)
- [x] Guard proven: planting an HTML file, `package.json`, a webview crate, `FakeAgent`, `node`, a testkit file, or a `--kiosk` browser in a temp copy makes it fail. Command: `tests/guard/test-check-no-fake.sh`. Evidence: [tests/evidence/T0/guard-selftest.txt](tests/evidence/T0/guard-selftest.txt)

## T2: protocol, tracer, client
- [x] Round-trip tests for every message plus golden JSON fixtures. Command: `cargo test -p protocol` (6 unit + 4 golden). Evidence: [tests/evidence/T2/tests.txt](tests/evidence/T2/tests.txt)
- [x] Tracer test prints a three-stage timeline. Command: `cargo test -p tracer -- --nocapture three_stage_timeline`. Evidence: [tests/evidence/T2/timeline.txt](tests/evidence/T2/timeline.txt)
- [x] Client: request/response/event delivery, reconnect after the server goes away, approvals on the approval socket. Command: `cargo test -p client`

## T1: Kernel
- [x] `build/bzImage` exists: Linux 6.18.58 x86 boot executable. Evidence: [tests/evidence/T1/bzImage.txt](tests/evidence/T1/bzImage.txt)
- [x] `scripts/12-kernel-verify-config.sh` exits 0 (74 symbols). Evidence: [tests/evidence/T1/verify-config.txt](tests/evidence/T1/verify-config.txt)
- [x] QEMU smoke test reaches `Kernel panic - not syncing: VFS: Unable to mount root fs` (TCG, 7.7 s). Command: `scripts/14-kernel-smoke.sh`. Evidence: [tests/evidence/T1/smoke-boot.txt](tests/evidence/T1/smoke-boot.txt)

## T3: Root filesystem, disk image, QEMU boot test
- [x] Image boots under UEFI (OVMF) with our UKI and kernel into labwc. Command: `python3 tests/boot/boot_test.py`. Evidence: [tests/evidence/T3/serial-boot.log](tests/evidence/T3/serial-boot.log), [results.json](tests/evidence/T3/results.json)
- [x] In-VM checks pass: kernel 6.18.58, `aios-session` active, labwc running, `/` is read-only erofs, `/var/lib/aios` is ext4 on the data partition, notes owned by the user, `/usr` not writable. Evidence: [tests/evidence/T3/in-vm-checks.txt](tests/evidence/T3/in-vm-checks.txt)
- [x] Screenshots are not blank: [boot.png](tests/evidence/T3/boot.png) (3117 colours: pastel backdrop and the shell top bar), [window.png](tests/evidence/T3/window.png) (3591 colours: a foot terminal window on the desktop).
- [x] Boot timeline recorded (emulated): [tests/latency/RESULTS.md](tests/latency/RESULTS.md)

## T4: Native desktop shell
- [x] Shell autostarts under labwc in the booted image (journal `aios-shell: started`). Command: `python3 tests/boot/flow_test.py`. Evidence: [tests/evidence/T4/flow-notes.txt](tests/evidence/T4/flow-notes.txt)
- [x] Screenshots: [top bar](tests/evidence/T4/topbar.png), [palette with typed text](tests/evidence/T4/palette-typed.png) (typed with QEMU `sendkey`), [plan ticking](tests/evidence/T4/plan-ticking.png) and [approval dialog](tests/evidence/T4/approval.png) driven by the test-only event server, [result](tests/evidence/T4/result.png), [setup panel](tests/evidence/T4/setup-panel.png).
- [x] Keypress-to-frame traced in the VM (emulated numbers: [tests/latency/RESULTS.md](tests/latency/RESULTS.md)); 2-frame budget not checkable without KVM.
- [x] `check-no-fake.sh` passes (part of `./check.sh`).

## T5: aiosd, broker, actions
- [x] `open browser` asks first, then a real Epiphany window starts inside bubblewrap (own PID namespace, no `/run/aios`, no data partition). Evidence: [approval](tests/evidence/T5/open-browser-approval.png), [browser](tests/evidence/T5/open-browser.png), process and namespace checks plus audit lines in [flow-notes.txt](tests/evidence/T5/flow-notes.txt)
- [x] `search rust language` opens `https://duckduckgo.com/?q=rust+language` ([search.png](tests/evidence/T5/search.png)); `play lofi beats` opens the YouTube search ([play.png](tests/evidence/T5/play.png)). The VM has no internet, so the pages themselves do not load (BLOCKERS.md 3); the process command lines with the URLs are in flow-notes.txt.
- [x] `note: buy milk and eggs` writes `/var/lib/aios/notes/buy-milk-and-eggs.md` ([note.png](tests/evidence/T5/note.png)); `read todo.txt` shows the file ([read.png](tests/evidence/T5/read.png)); `read /etc/shadow` is refused and audited ([read-denied.png](tests/evidence/T5/read-denied.png)).
- [x] All five router commands work with no AI configured (the top bar shows "AI not connected" in every screenshot).
- [x] Broker unit tests cover every row of section 8, traversal, symlink escape, huge reads, unknown actions. Command: `cargo test -p broker -p actions`.

## T6: AI agent loop and first-run setup
- [x] Endpoint, model and key entered in the setup panel by `sendkey`, tested and saved (`ai.key` 0600, `ai.toml` 0640). Evidence: [not-connected.png](tests/evidence/T6/not-connected.png), [setup-saved.png](tests/evidence/T6/setup-saved.png), [flow-notes.txt](tests/evidence/T6/flow-notes.txt)
- [x] Free-form "plan my weekend" runs a 3-step plan in parallel for real (note written, search and music opened) ([weekend-plan.png](tests/evidence/T6/weekend-plan.png)); "summarize my todo list" reads the file and answers from it ([todo-summary.png](tests/evidence/T6/todo-summary.png)).
- [x] Router commands make no model request: 0 requests vs 3 for free-form, from the mock AI's log ([mock-ai-requests.jsonl](tests/evidence/T6/mock-ai-requests.jsonl), [flow-results.json](tests/evidence/T6/flow-results.json)).
- [x] Cancellation measured: 0.35 ms to the `cancelled` event; the in-flight model request is dropped ([cancelled.png](tests/evidence/T6/cancelled.png)).
- [x] README documents connecting a real endpoint.

## T7: Trust verification inside the image
- [x] Suite passes inside the VM, 40 of 40 checks. Command: `python3 tests/injection/run_in_vm.py`. Evidence: [suite.txt](tests/evidence/T7/suite.txt), [results.json](tests/evidence/T7/results.json), [audit.log](tests/evidence/T7/audit.log)
- [x] Hostile instructions in search results, a read file, app output and the AI's own tool calls: no `never` action executed, no read outside the roots, the only approval is the one typed into the shell, every denial audited, tool calls made after untrusted data are ignored.
- [x] Sandbox proof from a helper launched through the real `app.launch` path: cannot connect to any `/run/aios` socket, read `/var/lib/aios`, see or ptrace or signal the shell ([probe.json](tests/evidence/T7/probe.json), [terminal-approval.png](tests/evidence/T7/terminal-approval.png)).
- [x] Landlock enforced and seccomp filter active on the running aiosd; a forbidden read and `execve` fail ([sandbox-check.json](tests/evidence/T7/sandbox-check.json)).
- [x] Stop latency recorded: median 9.2 ms, max 12.3 ms ([tests/latency/RESULTS.md](tests/latency/RESULTS.md)).

## T8: Hardware-ready image
- [x] Kernel rebuilt with `hardware.fragment` (170 symbols verified), 124 modules installed into the root filesystem, firmware added. Evidence: [kernel.txt](tests/evidence/T8/kernel.txt)
- [x] Product image reaches the desktop on NVMe, AHCI and USB storage (xHCI): [nvme](tests/evidence/T8/desktop-nvme.png), [ahci](tests/evidence/T8/desktop-ahci.png), [usb](tests/evidence/T8/desktop-usb.png). Command: `python3 tests/boot/hw_variants.py`. Evidence: [results.json](tests/evidence/T8/results.json)
- [x] `build/aios-usb.img` (2.4 GB) boots from a 16 GB USB stick and grows its data partition from 1 GiB to 13.7 GiB on first boot, ext4 included ([desktop-usb-stick.png](tests/evidence/T8/desktop-usb-stick.png), results.json `usb_stick`).
- [x] Native Wi-Fi picker against real iwd with simulated radios: lists "AiOS-Test", takes the password in the shell's own field, connects. Command: `python3 tests/boot/wifi_test.py`. Evidence: [list](tests/evidence/T8/wifi-list.png), [password](tests/evidence/T8/wifi-password.png), [connected](tests/evidence/T8/wifi-connected.png), [wifi-results.json](tests/evidence/T8/wifi-results.json)
- [x] `scripts/write-usb.sh` and [docs/HARDWARE_TEST.md](docs/HARDWARE_TEST.md) exist; the script was never run by the agent.
- [ ] Physical hardware boot: the human's step (pending).
