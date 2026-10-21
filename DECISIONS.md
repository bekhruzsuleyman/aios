# Decisions

One line per decision that the plan left open or that this build host forced. Newest at the bottom of each section.

## T0: host and tooling
- **Restart.** The branch first held work for an earlier plan (HTML shell, scripted agent). Bekhruz asked to start over with this plan; the T0 commit removes all of it. The old commits remain in history because a force push was not permitted from this session.
- **Build host:** Ubuntu 24.04.5 cloud container, 4 CPUs, 15 GiB RAM, ~28 GB free, root, **no /dev/kvm** → QEMU runs with `-accel tcg`, boot timeouts ×10, all latency numbers are marked *emulated* (see BLOCKERS.md).
- **Network policy of this host:** kernel.org, deb.debian.org, ftp.debian.org, security.debian.org, mozilla.org are unreachable (proxy 403 / DNS). Reachable: GitHub, crates.io, PyPI, archive.ubuntu.com. Consequences are listed in T1 and T3 below and in BLOCKERS.md.
- **Package substitutions (section 4.2):** `systemd-boot` → `systemd-boot-efi` (Ubuntu splits the EFI binaries); `mkosi` from the distro is too old → mkosi **v25.3** installed from its GitHub tag into `build/venv` (mkosi is not on PyPI); added `fdisk` (sfdisk) explicitly.
- **Tool versions:** gcc 13.3.0, GNU make 4.3, QEMU 8.2.2, mkfs.erofs 1.7.1, ukify 255.4, mkosi 25.3, cargo/rustc 1.97.0, bubblewrap 0.9.0, mtools 4.0.43, e2fsprogs 1.47.0, OVMF `/usr/share/OVMF/OVMF_CODE_4M.fd`.
- **Guard self-test** lives in `tests/guard/test-check-no-fake.sh` and runs from `check.sh`.
- **Lints:** `unwrap()`/`expect()` are denied workspace-wide by clippy (`unwrap_used`, `expect_used`) and allowed in tests via `clippy.toml`.

## T1: kernel
- **Version: Linux 6.18.58** (2026-10-21), newest point release of the newest longterm series (6.18). kernel.org's `releases.json` is unreachable here, so `scripts/10-kernel-fetch.sh` falls back to the pinned LTS series (`LTS_SERIES=6.18`) and takes the newest tag from the stable-tree mirror `github.com/gregkh/linux`.
- **Source integrity:** without kernel.org there is no `sha256sums.asc`; the source is a shallow git clone of tag `v6.18.58`, commit `2926d88bb7fc4608fd942453d929c39e4e5ec006`, recorded in `third_party/KERNEL_SOURCE`. When kernel.org is reachable the script uses the tarball and sha256 check as the plan specifies.
- **Fragment addition:** `CONFIG_BPF_SYSCALL=y`, because `CONFIG_CGROUP_BPF` depends on it and was otherwise dropped by olddefconfig.

## T2: protocol, tracer, client
- **Requests vs. envelope fields:** `ts_ns`/`task_id`/`seq` are on every event; requests are plain JSON-RPC 2.0 because `task.start` has no task yet. Responses and events share a connection; clients tell them apart by the `jsonrpc` key (`protocol::ServerMsg`, untagged).
- **Event tag:** `"type": "<snake_case>"` flattened next to the envelope fields. Golden fixtures: `crates/protocol/tests/golden/*.json` (15 files, one per message kind).
- **Unknown methods** (e.g. `shell.exec`) fail to deserialise: `Call` is a closed enum.
- **Monotonic clock:** `tracer::now_ns()` = ns since a process-wide `Instant` epoch; comparable within one process.
- **client** uses std threads and blocking sockets (no async runtime) so the Wayland shell can embed it; it reconnects with backoff 100 ms → 2 s and delivers `Connected`/`Disconnected`/`Response`/`Event` to a sink callback. `approval.respond` always goes to the approval socket on a short-lived connection.
- **Build time:** ~25 min for `bzImage` on 4 vCPUs with ccache cold. `scripts/14-kernel-smoke.sh` added for the smoke boot (plan 6.4).
- **Active LSMs** at boot: `capability,landlock` (the requested `lockdown`, `yama` are not built by defconfig; `CONFIG_LSM` lists them harmlessly).
