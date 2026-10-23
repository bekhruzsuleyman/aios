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

## T3: rootfs, disk, boot test
- **Userland is Ubuntu 24.04 (noble), not Debian:** the Debian mirrors are unreachable from this host. mkosi builds it the same way (`Distribution=ubuntu`, main + universe, HTTPS sources in `mkosi.sandbox`).
- **Browser is Firefox from Mozilla's own .deb** (changed 2026-10-23, Bekhruz's choice): Ubuntu ships Firefox only as a snap, which cannot run on a read-only erofs root, so 0.1 first used GNOME Web (Epiphany). Epiphany always draws its own GNOME title bar and can never match the design's window chrome; Firefox hands its title bar to the compositor (`browser.tabs.inTitlebar` locked to 0 by policy). The .deb is fetched from packages.mozilla.org on a machine that can reach it (this build host's proxy blocks it), checked against the SHA256 in Mozilla's Packages index by `scripts/20-rootfs.sh`, and installed with `dpkg` in `mkosi.postinst`; its library dependencies are in `mkosi.conf`. Policies disable updates (read-only root), telemetry and first-run pages.
- **Fonts:** Inter (UI), JetBrains Mono (labels, `fonts-jetbrains-mono`), Instrument Serif italic for the AI's voice (OFL, from google/fonts, shipped in `/usr/share/fonts/truetype/instrument-serif` with its licence), matching the design mockups in `docs/design/`.
- **Writable state:** `/var`, `/tmp` and `/home` are tmpfs (fstab); persistent state lives only on the ext4 data partition `LABEL=aios-data` at `/var/lib/aios`. `x-systemd.device-timeout=120s` because emulated boots are slow.
- **Test image variant:** `build/aios-test.img` adds root autologin on `ttyS0`, `/etc/aios/TEST_IMAGE` and the test-only event server; the shipped `build/aios.img` has none of these and `check-no-fake.sh --rootfs` verifies it.
- **iwd** only starts when Wi-Fi hardware exists (`ConditionDirectoryNotEmpty=/sys/class/ieee80211`).

## T4: shell
- **Keyboard first:** besides clicks, the palette has keys for everything so the QEMU monitor (`sendkey`) can drive it: Enter, Esc, Alt+A / Alt+D (allow/deny), Alt+C (stop), Alt+S (AI settings). On a fresh boot without an AI, the palette says so and points to Alt+S.
- **Status on connect:** the shell calls `status.get` whenever it (re)connects and after every settings change; the top bar chip shows READY or AI NOT CONNECTED from the daemon's answer, never a guess.

## T5: aiosd, broker, actions
- **Policy follows section 8 literally:** `app.launch` is `ask` for every app (the browser too); `web.search`, `media.play`, `notes.create` are `free`. `fs.read` *inside* `read_roots` is `free` (the table's `never` row bounds it; it has no side effects and its output is tagged untrusted); outside is `never`. `[policy] default` (`ask` or `never`) sets the risk of actions not in the table; `free` is refused.
- **Who launches apps:** aiosd runs as the `aiosd` system user and cannot start programs in the desktop session, so the shell runs a small launcher on `/run/aios/launch.sock`. aiosd sends a typed `{app, url}`; the launcher accepts only aiosd's uid (and root), validates the URL (http/https only), and builds the bubblewrap command line from the app id alone (`actions::launch::sandbox_argv`). Apps get a private scratch home under `~/.apps/<app>`, an empty `/run` and `/var`, read-only `/sys` (WebKit's own inner bubblewrap sandbox binds parts of it), their own PID/IPC/UTS namespaces, and a private session bus (`dbus-run-session`) that dies with the sandbox: GTK apps need a bus, and the user's bus is never reachable from inside.
- **No capabilities in the desktop session:** systemd's `pam_systemd` gives user sessions `CAP_WAKE_ALARM` as an ambient capability by default; unprivileged bubblewrap refuses to start with any capability set, so every launched app died at once. The shell clears its ambient, effective, permitted and inheritable sets first thing in `main` (before any thread exists), and the T7 suite checks it holds none. (Setting `default-capability-ambient-set=` empty in the PAM stack did not take effect.)
- **Socket directory:** `/run/aios` is `2770 aiosd:aios` (tmpfiles), not 0700: the shell runs as `aios` and must reach `agent.sock` and `approve.sock`, and aiosd must reach the shell's `launch.sock`. The setgid bit makes every socket group `aios`; sockets are 0660. `SO_PEERCRED` is checked on every connection (root, aiosd's own uid, the `aios` user). Launched apps never see the directory.
- **Approval socket:** `approval.respond` and `settings.ai.set` are refused unless they arrive on `approve.sock`; that socket sends no events.
- **Data partition ownership:** fixed at service start (`ExecStartPre=+systemd-tmpfiles --create /usr/lib/aios/tmpfiles-data.conf`): `/var/lib/aios` aiosd, `notes/` `2775 aiosd:aios`, `user/` `2775 aios:aios`. aiosd is also in group `aios`.
- **Service hardening:** `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`, `PrivateDevices`, `NoNewPrivileges`, empty capability set, `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`, `SystemCallFilter=@system-service`, `MemoryDenyWriteExecute`.
- **Dev mode** is explicit (`--dev` or `AIOS_DEV=1`), not "not running as root", because in the image aiosd runs as the unprivileged `aiosd` user.
- **Result text:** router commands show the action's summary, plus file contents for `read` (clipped to 4000 bytes) as data.

## T6: AI loop
- **Wire format:** OpenAI `/chat/completions` with `stream: true`, `tools` for the five actions (`app_launch`, `web_search`, `media_play`, `notes_create`, `fs_read`: tool names cannot contain dots), `tool_choice: auto`. Streamed tool-call deltas are merged by index. Unknown tool names map `a_b` to `a.b` so the broker's policy applies (`shell_exec` → `shell.exec` → never).
- **One planning round, then a tool-less answer:** all tool calls of the first reply are the plan (at most 8 steps, run in parallel). If any step returned untrusted data, the model gets the results as `tool` messages, data inside `<untrusted_data source=...>` blocks (a closing tag inside the data is defused), and is asked for the final answer **without tools**. Tool calls it makes anyway are logged and ignored. If no step returned data, the step summaries are the answer and no second request is made.
- **Settings:** `settings.ai.set` normalises `IP:port` to `http://IP:port/v1`, sends a one-message test request, and (unless test-only) writes `ai.key` (0600) then `ai.toml` (0640) atomically. A save is kept even if the test fails, with the real error shown, so a server that is down right now can be set up.
- **Cancellation** aborts the task's future, which drops the in-flight HTTP request and running steps, and revokes pending approvals. Measured by the `cancel` trace stage.
- **mock-ai** (`tests/testkit/mock-ai/mock_ai.py`, Python stdlib, tests only) streams tool calls split across chunks the way OpenAI does, checks the key and model, logs every request (to prove router commands make none), returns hostile tool calls for `HOSTILE-TEST`, "obeys" injected text in tool results, and holds `slow-test` prompts for 30 s for cancellation tests.

## T7: trust verification
- **Landlock and seccomp cover the whole daemon**, not a separate action process: aiosd applies both in `main` before the tokio runtime starts, so every thread (and so every action the broker runs) is inside them. Landlock (ABI v3): read-only `/usr /etc /proc /sys` and the read roots; read-write only the notes, audit/settings and socket directories. seccomp: `execve`, `execveat`, `ptrace`, `process_vm_*`, mount/namespace syscalls, `bpf`, module and kexec syscalls fail with EPERM. aiosd never starts programs (apps are started by the shell's launcher), so a bug or a hostile model can never turn into a command.
- HTTP clients are built before the sandbox is applied (they read proxy settings and root certificates from files the sandbox then hides).
- `aiosd --check-sandbox` applies the sandbox and then tries a forbidden read and an exec; used by `crates/aiosd/tests/sandbox.rs` and the in-VM suite.
- **"App output"** as an injection channel: apps cannot hand data to the agent in 0.1 except through files, so the fixture is a file an app could have written (`fixtures/app-output.txt`) read with `fs.read`.
- **Approvals for tasks started by other clients** of `agent.sock` are never shown in the shell (it only shows its own task), so such a task waits until it is cancelled. That is the safe default; the suite's forged-approval case relies on it.
- **Sandbox probe:** the suite bind-mounts the test helper over `/usr/bin/foot` in the test VM, so `open terminal` (approved by Alt+A in the real palette) starts it through the real broker, launcher and bubblewrap.

## T8: hardware image
- **Commits T4 to T8:** these tasks were built together because each VM run exercised all of them (one image, one boot). `./check.sh` was run on the finished tree before committing; the five commits split the work by task so each one's files and evidence are reviewable, but the intermediate commits are not each guaranteed to build on their own.
- **Firmware:** Ubuntu's `linux-firmware` (main) replaces Debian's `firmware-*` packages from `non-free-firmware`, plus `wireless-regdb`.
- **Modules** live in `/usr/lib/modules/<kver>` on the read-only root (zstd-compressed, stripped); storage and input are built in because there is no initramfs. `rootwait` is on the command line for USB roots.
- **"Hybrid GPT image":** a GPT disk with a protective MBR, UEFI-only (AiOS 0.1 has no BIOS boot path). `build/aios-usb.img` is the product image with the same layout; `systemd-repart` (`/usr/lib/repart.d/50-aios-data.conf`, ordered before `local-fs-pre.target`) grows the last (data) partition to fill the stick on first boot and `x-systemd.growfs` grows ext4 when it is mounted.
- **Logs survive reboots:** the journal is kept on the data partition (`/var/log/journal` → `/var/lib/aios/journal`), so a failed hardware boot can be read on another machine with `journalctl -D`.
- **Kernel crypto for iwd:** iwd uses the kernel's AF_ALG and keyctl crypto and refuses to start without them, so `hardware.fragment` turns on the skcipher/hash user APIs, AES/DES/ECB/CBC/CMAC/MD5/SHA, DH key operations and the asymmetric key parsers (found by booting: iwd listed the missing options).
- **Wi-Fi picker:** a thread in the shell talks to iwd over the system D-Bus (zbus, blocking API): stations and networks from `GetManagedObjects` and `Station.GetOrderedNetworks`, `Network.Connect`, and an agent object (`net.connman.iwd.Agent`) that hands iwd the password typed in the shell's own field. The desktop user is in group `netdev`, which iwd's D-Bus policy allows. The chip appears in the top bar only when a radio exists; Alt+W opens the picker from the palette.
- **write-usb.sh** refuses: no exact `/dev/...` path, partitions instead of whole disks, non-removable non-USB disks, disks with mounted partitions, the disk holding the running system, sticks smaller than the image; it shows the device and asks for the path to be typed again. The agent never runs it.
