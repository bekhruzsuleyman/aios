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
