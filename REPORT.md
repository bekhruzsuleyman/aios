# AiOS 0.1 report

AiOS 0.1 is built and verified in QEMU. Everything in plan sections 2 and 9 (T0 to T8) works inside the booted image, with evidence in `tests/evidence/`. One step is left, and it is yours: booting the USB image on a real machine (`docs/HARDWARE_TEST.md`). T9 (voice) is optional and was skipped.

All timing numbers are **emulated**. This build host has no KVM, so QEMU ran with TCG (a software CPU), and every number is many times slower than it will be on KVM or real hardware.

## What works, and where the evidence is

| Task | What was proven in the VM | Evidence |
|---|---|---|
| T0 | Host tools, repo skeleton, `check.sh`, an anti-fake guard that catches planted HTML, webviews, FakeAgent, node, kiosk browsers | `tests/evidence/T0/` |
| T1 | Linux 6.18.58 LTS built from source, config verified, smoke boot | `tests/evidence/T1/` |
| T2 | Protocol with golden JSON fixtures, tracer, socket client | `tests/evidence/T2/` |
| T3 | UEFI boot of our UKI into labwc on a read-only erofs root, data on ext4 | `tests/evidence/T3/` |
| T4 | Native Rust Wayland shell (layer-shell, tiny-skia, cosmic-text): top bar, palette, plan, approval, setup panel; keys typed with `sendkey` | `tests/evidence/T4/` |
| T5 | `open browser` (asks, then real Epiphany in bubblewrap), `search`, `play`, `note:`, `read`; all with no AI; denials audited | `tests/evidence/T5/` |
| T6 | Setup typed into the shell and saved; multi-step free-form plans executed in parallel; router commands make 0 model requests; cancel in 0.35 ms | `tests/evidence/T6/` |
| T7 | 40/40 injection and sandbox checks inside the VM, Landlock and seccomp on aiosd, a launched app cannot reach the agent sockets, data or the shell | `tests/evidence/T7/` |
| T8 | Hardware kernel with 124 modules and firmware; product image reaches the desktop on NVMe, AHCI and USB; USB image grows to fill a 16 GB stick; Wi-Fi picker connects through real iwd | `tests/evidence/T8/` |

The full checklist, every line ticked except the physical boot, is in `CHECKLIST.md`.

## Latency (emulated, TCG)

| Stage | Median | Budget (KVM) |
|---|---|---|
| Boot to desktop (test image, T3) | 55.5 s compositor ready | about 5 s |
| Boot to desktop (product image, NVMe/AHCI/USB) | 63 to 69 s | about 5 s |
| Keypress to rendered frame | 337 ms | 2 frames |
| Router | 0.18 ms | 300 ms |
| Broker decision | 4.3 ms | 1 ms |
| Stop (cancel to `cancelled`) | 9.2 ms (max 12.3 ms) | |

Details: `tests/latency/RESULTS.md`. The frame, boot and broker numbers miss their budgets because the CPU is emulated; rerun on KVM before drawing conclusions.

## Kernel

Linux **6.18.58** (LTS), from the GitHub stable mirror because kernel.org is blocked here (`DECISIONS.md` T1). Config: `defconfig` + `kvm_guest.config` + `image/kernel/aios.fragment` (74 symbols: erofs, ext4, Landlock, seccomp, namespaces, virtio, DRM) + `image/kernel/hardware.fragment` (96 symbols: built-in AHCI/NVMe/USB storage, HID and early framebuffer; modules for i915, xe, amdgpu, nouveau, iwlwifi, ath, rtw, mt76, Bluetooth, HDA and SOF audio; the crypto iwd needs). `scripts/12-kernel-verify-config.sh` checks all 170.

## What failed along the way, and what changed

- **Launched apps died at once.** systemd gives user sessions `CAP_WAKE_ALARM` as an ambient capability, and bubblewrap refuses to start with any capability set (`bwrap: Unexpected capabilities but not setuid`). The shell now drops all capabilities at startup, and T7 checks it.
- **Epiphany aborted inside the sandbox** without a session bus and without `/sys`. Apps now get a private `dbus-run-session` bus and read-only `/sys`.
- **iwd would not start**: the kernel lacked AF_ALG and keyctl crypto (`iwd: The following options are missing in the kernel: CONFIG_CRYPTO_USER_API_SKCIPHER ...`). Added to `hardware.fragment`.
- **aiosd was killed with SIGSYS** because systemd's syscall filter blocked the Landlock and seccomp setup calls. The unit now allows exactly those.
- **Firefox and Debian:** Debian mirrors are blocked here, so the userland is Ubuntu noble, where Firefox is a snap. The browser is Epiphany (WebKitGTK) instead (`DECISIONS.md` T3).

## Blockers (from `BLOCKERS.md`)

1. No KVM: all timing numbers are emulated.
2. Debian mirrors and kernel.org unreachable: worked around with Ubuntu and the GitHub kernel mirror.
3. No internet inside the VM: browser URLs are correct, but pages do not load in screenshots; AI tests use the test-only mock server.
4. No physical hardware: the USB stick boot is your step.

## Your step: the physical boot (pending)

1. Write `build/aios-usb.img` to a USB stick of 16 GB or more with `scripts/write-usb.sh /dev/sdX` (it refuses system disks, mounted disks and partitions, and asks you to confirm).
2. Boot it in UEFI mode with Secure Boot off.
3. Follow `docs/HARDWARE_TEST.md` for what you should see and how to collect logs if it fails.

To boot it in QEMU instead: `scripts/22-run-qemu.sh build/aios.img`. To connect an AI: press Super+Space, then Alt+S, and fill in the endpoint, model and key (`README.md`, "Connecting an AI").

## Next steps, ranked

1. Boot the USB stick on real hardware and fix whatever the journal shows.
2. Rerun the boot and latency measurements on a KVM host.
3. Try a real model endpoint, and tune the system prompt and tool descriptions on real plans.
4. Build with a Debian userland and Firefox if the mirrors become reachable, as the plan originally specified.
5. T9: voice (push-to-talk, whisper.cpp, Piper).
