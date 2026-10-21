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
