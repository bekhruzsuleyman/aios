# Blockers

Things this build host cannot do. Each entry: the command, the error, and the one thing the human must do. Work continues on everything that does not depend on it.

## 1. No KVM on the build host (not fatal)
- Command: `ls -l /dev/kvm`
- Error: `No such file or directory` (cloud container without nested virtualisation).
- Effect: QEMU runs with `-accel tcg`. Everything still boots, but slowly; boot and frame-latency numbers are emulated and far above the KVM budgets.
- Human action (optional): rerun `scripts/22-run-qemu.sh` and `tests/latency/` on a Linux machine with KVM to get real numbers.

## 2. Debian mirrors and kernel.org unreachable (worked around)
- Command: `curl -sS https://deb.debian.org/debian/` and `curl -sS https://cdn.kernel.org/pub/linux/kernel/`
- Error: `CONNECT tunnel failed, response 403` / `Could not resolve host` after 3 retries and all mirrors in plan section 4.4.
- Workaround: kernel source from the GitHub stable mirror (see DECISIONS.md T1); userland from the Ubuntu archive (see DECISIONS.md T3).
- Human action (optional): allow `deb.debian.org` and `cdn.kernel.org` in this environment's network policy to build exactly as the plan specifies (Debian stable userland, verified kernel tarball).

## 3. No internet inside the VM (not fatal)
- Command: QEMU user networking from this container; the container reaches the internet only through an authenticating HTTPS proxy that the guest cannot use.
- Effect: the browser opens the right URLs (checked from the process command line) but the pages do not load in screenshots; the AI tests use the test-only mock server on the host (`10.0.2.2:8081`), as the plan specifies.
- Human action (optional): boot `scripts/22-run-qemu.sh` on a machine with normal internet, or the USB stick on hardware, to see pages load and to connect a real AI endpoint.

## 4. No physical hardware (the plan's human step)
- The USB image cannot be booted on a real machine from here. Follow `docs/HARDWARE_TEST.md`.
