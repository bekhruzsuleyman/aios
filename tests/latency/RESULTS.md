# Latency results

All numbers on this build host are **emulated**: there is no `/dev/kvm`, so
QEMU runs with TCG (see BLOCKERS.md). They show where time goes, not what
real hardware or KVM will do.

## Boot timeline (T3)

Command: `python3 tests/boot/boot_test.py` (test image, `SNAPSHOT=1`).
Evidence: [tests/evidence/T3/results.json](../evidence/T3/results.json).

| Stage | Value | Source |
|---|---|---|
| Kernel | 6.442 s | `systemd-analyze` |
| Userspace to `default.target` | 38.335 s | `systemd-analyze` |
| Total (systemd) | 44.777 s | `systemd-analyze` |
| labwc compositor ready | 55.5 s uptime | `aios: compositor ready` journal line |
| aios-shell started | 57.24 s uptime | shell journal line |
| Host wall clock to serial root shell | 66.2 s | `tests/boot/vm.py` |

Accelerator: tcg (emulated), 4 vCPU host, 2 GiB guest.

## Shell, router, broker and AI loop (T4 to T6)

Command: `python3 tests/boot/flow_test.py` (test image, TCG). Stage times
come from the tracer lines in the VM journal; evidence:
[tests/evidence/T5/flow-notes.txt](../evidence/T5/flow-notes.txt),
[tests/evidence/T6/flow-notes.txt](../evidence/T6/flow-notes.txt),
[tests/evidence/T6/flow-results.json](../evidence/T6/flow-results.json).

| Stage | Samples | Median | Max | Budget (KVM) |
|---|---|---|---|---|
| Keypress to rendered frame (`key_to_frame`) | 30 | 336.7 ms (p90 491 ms, min 18 ms) | 920 ms | 2 frames (33 ms) |
| Router (`router`) | 5 | 0.18 ms | 1.65 ms | 300 ms |
| Broker decision (`broker.decide`) | 6 | 4.3 ms | 17.8 ms | 1 ms |
| Model first byte (mock AI on the host, `ai.first_byte`) | 4 | 17.6 ms | 98.7 ms | n/a |
| Cancel to `cancelled` event (`cancel`, T6) | 1 | 0.35 ms | | |

All of these miss or barely meet their budgets only because the CPU is
emulated: under TCG every frame is drawn by a software-emulated CPU into a
software-emulated framebuffer. They need a KVM rerun before they mean
anything about real speed (BLOCKERS.md 1).

Router commands made **0** model requests; the three free-form requests made
3 (two planning rounds and one tool-less answer round), counted from the mock
AI's request log.

## Stop latency (T7)

Command: `python3 tests/injection/run_in_vm.py` (test image, TCG). The client
sends `task.cancel` while the model request is in flight and measures until
the `cancelled` event arrives. Evidence:
[tests/evidence/T7/results.json](../evidence/T7/results.json).

| Run | 1 | 2 | 3 | 4 | 5 | Median | Max |
|---|---|---|---|---|---|---|---|
| Stop latency (ms) | 4.60 | 9.16 | 11.24 | 3.86 | 12.31 | 9.16 | 12.31 |

## Hardware variants (T8)

Command: `python3 tests/boot/hw_variants.py` (product image, TCG). Seconds
from QEMU start until a screenshot shows the desktop (over 2000 colours).
Evidence: [tests/evidence/T8/results.json](../evidence/T8/results.json).

| Disk controller | Desktop | Seconds |
|---|---|---|
| NVMe | yes | 68.8 |
| AHCI (SATA) | yes | 63.0 |
| USB storage on xHCI | yes | 68.9 |
| USB image on a 16 GB stick (first boot, grows data) | yes | 68.8 |
