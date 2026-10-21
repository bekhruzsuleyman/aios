# State

- **Current task:** T1 (kernel build running in background, log `logs/kernel-build.log`); T2 next.
- **Last completed step:** T0 committed (host checks, tools, skeleton, guard).
- **Next command:** `tail logs/kernel-build.log`; when done, `file build/bzImage` and the QEMU smoke test (`scripts/14-kernel-smoke.sh`).
- **Open problems:** no KVM (TCG only); Debian and kernel.org blocked (see BLOCKERS.md).
