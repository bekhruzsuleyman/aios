# AiOS 0.1: Complete Plan (one self-contained prompt for an AI coding agent)

**How to use this file.** Start a fresh AI coding agent session (one that has a shell, file tools, and internet access). Give it this file (save it as `PLAN.md` in an empty folder) and say:

> Read PLAN.md completely, then execute it from the first unfinished task. Follow the operating contract in section 1. Do not ask me questions or ask me to download anything; decide and log.

Nothing else is needed. This file contains the vision, the design, every decision, the full build procedure (including downloading and building the Linux kernel), the tests, and the roadmap. It assumes no earlier conversation.

---

## A. Vision and requirements (from the human who owns this project)

AiOS is an **AI-first operating system**. Imagine the OS starting up and immediately being ready: it opens a chat (and later voice and video) surface, the user says what they want ("open YouTube and play lofi for late night coding"), and the OS simply does it, on the real system, visibly, with the user in control.

Requirements, in the human's own priorities:

1. **It must be a real OS.** Not a web page, not a simulation. It boots from its own disk image with its own kernel build and its own desktop. (Section 0 defines this precisely and bans the shortcuts.)
2. **Built on the Linux kernel.** We do not write a kernel. We build Linux from source (configured by us, no patches in 0.1), on top of a Debian userland.
3. **Our own desktop manager.** The desktop is our own native shell (Rust, Wayland). In 0.1 it runs on the labwc compositor with a fixed configuration; our own compositor comes later.
4. **Simple for the user, properly engineered underneath.** The user view is minimal: boot, ask, watch it happen, press Stop. No settings maze. Customization happens by talking to the agent. The complexity (permission broker, sandboxing, typed actions) is in the internals where users never see it.
5. **Fixed, like Windows.** One supported desktop, one design language, limited customization. The system image is read-only and updated as a whole; users cannot break it.
6. **Speed is a priority.** Fast boot to a usable desktop, instant UI response, streaming everywhere, and fast actions. Concretely: typed direct actions (not GUI click-through), an intent router that skips the AI for simple commands, streaming of AI tokens and plan steps, persistent connections, parallel execution of independent steps, cheap in-memory permission checks, and a latency tracer in every stage so we optimize from measurements.
7. **Safe by construction.** An agent that can do anything is one prompt injection away from disaster. Everything the AI or the web says is untrusted data. All actions go through a broker enforced by the kernel.
8. **The AI backend is configurable by IP.** The user (or developer) can point AiOS at any OpenAI-compatible endpoint: a cloud API, or a GPU box on the network by `IP:port`, or a local llama.cpp server. No code change needed.
9. **Later (roadmap, not in 0.1):** recognizing the user at boot (face and voice), voice and video chat, our own compositor, immutable A/B updates with verified boot, our own app package format and app manager. See section 15.

### Design reference (the human's mockup, described in words)

The mockup is a calm, light, minimal desktop:
- **Background:** soft pastel gradient (mint at top-left, lavender at bottom-left, peach at right), windows behind are slightly blurred.
- **Top bar:** a thin floating pill centered at the top: a green status dot with the label "AI · NATIVE OS", an "Ask" chip with a keyboard shortcut hint, a status chip, the time, a small weather or temperature item, the date, and a light/dark toggle. Small monospace uppercase labels.
- **Palette card:** a centered, large-radius, softly shadowed translucent card. Top line: the user's request in a large sans typeface with a status dot. Below a hairline divider: a small label "AI · THINKING OUT LOUD" and one italic serif line, for example *"Sure, pulling up YouTube and searching for lofi for late night coding."* Below that, a small label "PLAN" and a numbered list (01 to 04) such as "open the browser", "navigate to youtube.com", "search for ...", "show the results", with a green check on finished steps.
- **Agent presence:** the screen behind shows the actual app windows (a code editor, then a browser with the results). A small caption bubble follows the agent's cursor. A pill near the top shows that the agent is working and has a **Stop** button. A quiet status line at the bottom of the screen says what is happening ("Opening YouTube, searching ...").
- Note: the mockup's "demo running" wording was only for the mockup. The real product says "Working" and must never imply a demo.
- Windows are ordinary app windows with rounded corners and traffic-light-style controls.

The native shell in task T4 should follow this look and layout with software-drawn graphics (no web technology).

---

## 0. What "REAL" means (the most important section)

AiOS is a **Linux-based operating system**: our own build of the Linux kernel, a Debian userland, our own native desktop shell, our own agent daemon and permission system. (A from-scratch kernel is the separate "AiOS kernel" research project and is out of scope here.)

It is real only if **all** of these are true:

1. **It boots by itself.** A disk image with a UEFI bootloader, our own kernel, and a root filesystem starts from firmware and reaches the desktop with no host program involved. No `cargo run` or `npm start` on a host standing in for the OS.
2. **The desktop is native.** The UI is a native Wayland application written in Rust, running under a real compositor. It is not a web page, not Electron, not a webview, not a browser kiosk.
3. **The agent is real.** It calls a real AI endpoint (any `IP:port` or cloud URL set by the user) and executes real actions on the real system: launching real programs, writing real files, running real searches. If no AI is configured, the OS says so honestly and still runs its non-AI commands. It never invents answers or replays scripts.
4. **Permissions are real.** The broker is the only path to actions, enforced by the kernel (Landlock, seccomp, namespaces, separate users), so launched apps cannot approve their own requests.
5. **Everything is verified inside the booted image.** A feature that only works on the host, in a browser tab, or in a test harness does not count.

### 0.1 Banned shortcuts (the agent must never do these)

- No HTML, CSS, or JavaScript for any part of the OS UI. No Electron, Tauri, webview, WebKit shell, or browser kiosk as the shell. (A real browser such as Firefox may exist as an ordinary **app** the agent can launch, like on any desktop. It is never the desktop.)
- No `FakeAgent`, scripted agent, canned responses, or demo mode in any binary that goes into the image. Scripted event sources live only under `tests/` and are excluded from the image.
- No mock AI as a default. The mock AI server exists only for automated tests and is never configured in the shipped image.
- No "the host runs a dev server and the VM opens it".
- No claiming a task is done because unit tests pass on the host. Each task lists its in-image evidence.
- `scripts/check-no-fake.sh` (task T0) enforces most of this. It must run in `./check.sh` and in the image build, and fail the build on any violation.

---

## 1. Operating contract (read twice)

You are the engineer. The human is busy and wants results, not questions.

1. **Be self-sufficient.** You download, install, clone, configure, build, and run everything yourself. **Never ask the human to download a file, install a package, fetch the Linux kernel, or run a command for you** unless section 1.5 says you are truly blocked. (The one exception is the final test on physical hardware in T8, which only the human can do.)
2. **Do not ask questions. Decide.** Every decision that could have been a question is answered in section 3. If you hit something unlisted, choose the simplest option that matches the goals (real, simple for the user, fast, safe), write one line in `DECISIONS.md`, and continue.
3. **Resume-safe state.** Keep `STATE.md` current: current task, last completed step, next command, open problems. After a context reset, read `STATE.md`, `DECISIONS.md`, `CHECKLIST.md`, and `git log`, then continue. Update it after every task and every long-running command.
4. **Long commands** (kernel build, image build, QEMU boots) run in the background with output in `logs/<name>.log`. Poll the log, set timeouts, and always pass non-interactive flags (`DEBIAN_FRONTEND=noninteractive`, `-y`).
5. **Network failures.** Retry 3 times with backoff, then try alternative mirrors (section 4.4). Only then use the **blocked protocol**: append to `BLOCKERS.md` the exact command, the error, and the one thing the human must do; continue with every task that does not depend on it.
6. **Honesty.** Never fake a test result, never weaken a test to make it pass, never claim something works without running it. Failures go in `REPORT.md` with the log excerpt. Do not describe a stub as a feature.
7. **Scope.** Build only what this plan lists. The **Not in 0.1** list (section 13) is off limits.
8. **Quality gates.** `./check.sh` (fmt, clippy `-D warnings`, tests, `scripts/check-no-fake.sh`) must pass before each commit. One commit per task: `T<n>: <summary>`. No `unsafe` without a justifying comment. No `unwrap()`/`expect()` outside tests and `main`.
9. **Security invariant.** There is no code path from model output, web content, files, or app output to command execution except through the broker. Treat all such text as untrusted data.
10. **Reproducibility.** Every build step is a script in `scripts/` that runs from a clean checkout. No manual steps. Pin chosen versions in `DECISIONS.md`.
11. **Evidence.** For every task, save proof from inside the booted image in `tests/evidence/<task>/`: serial log excerpts, command output from the test shell, and QEMU screenshots (PNG). `CHECKLIST.md` links each check to its evidence file.

### 1.5 When you are truly blocked
Only these count: no network after retries, no `sudo`/root and no container runtime, less than 15 GB free disk, or no CPU virtualization *and* QEMU unavailable. Follow the blocked protocol and continue elsewhere.

---

## 2. Product and definition of done for 0.1

The user boots the machine. A calm, native desktop appears: a thin top bar, an ask palette, and room for apps. The user types (or later speaks) a request; the agent plans it, the broker checks each action, the actions run on the real system, and a native panel shows the plan ticking off with a Stop button. Real apps open in real windows.

0.1 is done when all of these are true, each with evidence in `tests/evidence/`:

1. A kernel built from source by you boots a disk image under UEFI (QEMU/OVMF) straight into the desktop, with no host process involved.
2. The desktop shell is a native Rust Wayland application. `scripts/check-no-fake.sh` passes.
3. Five commands work end to end inside the VM: open an app, web search, play media, create a note, read a file.
4. With a real AI endpoint configured through the first-run setup screen (tests point it at a test server on the host over QEMU networking), a free-form request produces a real plan and real actions.
5. Every action passes the broker and appears in the audit log.
6. The prompt-injection suite passes inside the VM, and launched apps cannot reach the agent's sockets.
7. Stop cancels a running task, with latency measured.
8. The tracer prints per-stage latency timelines, and `tests/latency/RESULTS.md` holds the numbers.
9. A hardware-ready image is produced (T8) with a README for writing it to a USB stick. (The human performs the physical-hardware boot.)
10. `REPORT.md` is written (section 14).

---

## 3. Decisions already made for you (do not ask)

| Topic | Decision |
|---|---|
| Host OS | Debian/Ubuntu-based Linux (native, VM, or WSL2). On macOS or Windows without WSL2, use a Debian container (Docker or Podman) as the build host; QEMU then runs with software emulation (no KVM). |
| Language | Rust stable for everything we write, including the shell. |
| Kernel | Newest **longterm** release from kernel.org, no patches, defconfig + `kvm_guest.config` + our fragments (section 6). |
| Boot | UEFI, systemd-boot, Unified Kernel Image built with `ukify`, **no initramfs** (storage and filesystem drivers built in). |
| Root FS | Debian stable base made with `mkosi`, packed as read-only **erofs**; user data on a separate ext4 partition. |
| Init | systemd (trimmed). |
| Compositor | **labwc** (wlroots, stacking, tiny, themeable) with a locked-down fixed configuration, shipped by us. It gives real windows for real apps today. Our own Smithay compositor replaces it in a later version. |
| Shell (the desktop UI) | Native Rust Wayland **layer-shell** client: `smithay-client-toolkit` + `tiny-skia` (software rendering, works without GPU) + `cosmic-text` for text. Top bar, palette, plan panel, approval dialog, Stop button, first-run AI setup. Style follows the user's mockup: light translucent surfaces, large radius, soft shadow, calm spacing, small monospace labels. |
| Shell to daemon | Unix socket, JSON-RPC 2.0 requests plus newline-delimited JSON events. No HTTP, no WebSocket. |
| AI backend | OpenAI-compatible endpoint (`IP:port` or cloud URL), entered in the first-run setup screen and stored in `/var/lib/aios/ai.toml` (key in `/var/lib/aios/ai.key`, mode 0600). Default: **not configured**, and the shell shows "AI not connected" while the router commands still work. |
| Web search | Provider trait: `http_json` (SearXNG-compatible URL from config) and `browser_fallback` (default: opens the browser on `https://duckduckgo.com/?q=<urlencoded>`). `fixture` provider exists only in tests. |
| Media | `media.play` opens `https://www.youtube.com/results?search_query=<urlencoded>` in the real browser. No scraping. |
| Apps shipped | Browser: `firefox-esr`; editor: `mousepad`; terminal: `foot` (launch needs approval); file manager: none in 0.1. |
| Launched apps | Run under `bubblewrap` as the same desktop user, with `/run/aios` and the data partition hidden, so they can never reach the agent sockets or other apps' data. |
| Fonts | `fonts-inter` (fallback `fonts-dejavu-core`) plus a monospace font. |
| Voice | Optional task T9, whisper.cpp and Piper. |
| Hardware | QEMU first. T8 produces a USB-bootable image with a generic hardware profile; the human does the physical test. |

---

## 4. Host bootstrap (task T0)

### 4.1 Detect and check
`scripts/00-host-check.sh` records in `logs/host.txt`: OS and version, CPU count, RAM, free disk (need at least 25 GB, absolute minimum 15 GB), `/dev/kvm` access, passwordless `sudo`, Docker/Podman, and reachability of `kernel.org`, `github.com`, `crates.io`, `deb.debian.org`.
If KVM is missing use `-accel tcg`, multiply boot timeouts by 10, and mark latency numbers as emulated.

### 4.2 Install tools (`scripts/01-install-tools.sh`, idempotent, non-interactive)
```
sudo DEBIAN_FRONTEND=noninteractive apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
  build-essential bc bison flex libssl-dev libelf-dev libncurses-dev dwarves \
  cpio kmod xz-utils zstd git curl ca-certificates gnupg python3 python3-pip \
  python3-venv python3-pil pkg-config clang lld cmake ninja-build \
  qemu-system-x86 qemu-utils ovmf mtools dosfstools e2fsprogs \
  erofs-utils squashfs-tools cryptsetup-bin systemd-boot systemd-ukify \
  debootstrap mkosi libseccomp-dev libasound2-dev \
  libwayland-dev wayland-protocols libxkbcommon-dev libfontconfig1-dev \
  libfreetype-dev bubblewrap netpbm ccache jq socat expect netcat-openbsd
```
If a package name does not exist in this release, find the equivalent with `apt-cache search`, install it, and log it in `DECISIONS.md`. If `systemd-ukify` or `mkosi` is too old, install a current one (`pip install --user --break-system-packages mkosi`, or the `ukify` script from the systemd source).

Rust:
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal -c rustfmt,clippy
. "$HOME/.cargo/env"
```

### 4.3 Verify
`scripts/02-verify-tools.sh` runs `--version` for gcc, make, bc, flex, bison, qemu-system-x86_64, mkfs.erofs, ukify, mkosi, cargo, rustc, bwrap, and checks `ls /usr/share/OVMF/`. It must exit 0. Record versions in `DECISIONS.md`.

### 4.4 Mirrors if a download fails
Kernel: `cdn.kernel.org`, `mirrors.edge.kernel.org`, `git.kernel.org`. Debian: `deb.debian.org`, `ftp.debian.org`. Rust: `static.rust-lang.org`. Crates: sparse protocol (`CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse`).

---

## 5. Repo layout (create exactly this)

```
aios/
  PLAN.md  STATE.md  DECISIONS.md  CHECKLIST.md  BLOCKERS.md  REPORT.md  README.md
  check.sh
  Cargo.toml                        # workspace
  config/                           # default system config shipped in the image
  scripts/                          # numbered build and run scripts, check-no-fake.sh
  logs/  third_party/  build/       # gitignored
  crates/
    protocol/  tracer/  client/     # shared types, timeline, socket client library
    broker/  actions/  aiosd/       # permission layer, real actions, the daemon
    shell/                          # native Wayland layer-shell desktop UI
  image/
    kernel/aios.fragment            # config fragment (QEMU)
    kernel/hardware.fragment        # extra config for real hardware (T8)
    rootfs/                         # mkosi config and files copied into the image
      etc/labwc/  etc/pam.d/  units/  usr/share/aios/theme/
  tests/
    testkit/                        # TEST-ONLY crate: scripted events, mock AI server (never in the image)
    evidence/                       # screenshots, logs, outputs per task
    boot/  injection/  latency/
```
`.gitignore`: `target/`, `logs/`, `third_party/`, `build/`.

### `scripts/check-no-fake.sh` (task T0 creates it, later tasks keep it passing)
Fails if any of these is true: a `.html`, `.js`, `.css`, or `package.json` file exists outside `tests/` and `third_party/`; the workspace depends on `tauri`, `wry`, `webkit2gtk`, `electron`, or any webview crate in non-test crates; any non-test source contains `FakeAgent`, `MockAgent`, `scripted`, or `demo_mode`; `tests/testkit` appears in the image rootfs; the built image rootfs contains `node`, `electron`, or a browser started with `--kiosk` anywhere in `etc/` or `units/`.

### Default config (`/etc/aios/aios.toml`; user overrides in `/var/lib/aios/ai.toml`)
```toml
[paths]
socket     = "/run/aios/agent.sock"
approve    = "/run/aios/approve.sock"   # reachable only by the shell
audit      = "/var/lib/aios/audit.log"
notes      = "/var/lib/aios/notes"
read_roots = ["/var/lib/aios/user"]

[search]
provider = "browser_fallback"            # "browser_fallback" | "http_json"
url      = ""

[policy]
default = "ask"
```
`ai.toml` (written by the first-run screen): `endpoint = "http://IP:PORT/v1"`, `model = "..."`. The key goes in `ai.key`.
Dev mode: when not running as root, replace `/var/lib/aios` with `./build/dev-data` and `/run/aios` with `$XDG_RUNTIME_DIR/aios` automatically (for unit tests only; acceptance happens in the image).

---

## 6. The Linux kernel (task T1): download, configure, build

You build the kernel yourself. Do not ask the human for it.

### 6.1 Resolve the version and download (`scripts/10-kernel-fetch.sh`)
```
mkdir -p third_party && cd third_party
curl -fsSL https://www.kernel.org/releases.json -o releases.json
KVER=$(python3 - <<'EOF'
import json
r = json.load(open("releases.json"))["releases"]
lts = [x for x in r if x["moniker"] == "longterm" and not x.get("iseol")]
lts.sort(key=lambda x: tuple(int(p) for p in x["version"].split(".") if p.isdigit()), reverse=True)
print((lts or [x for x in r if x["moniker"] == "stable"])[0]["version"])
EOF
)
MAJOR=${KVER%%.*}
BASE=https://cdn.kernel.org/pub/linux/kernel/v${MAJOR}.x
curl -fLO $BASE/linux-$KVER.tar.xz
curl -fLO $BASE/sha256sums.asc
grep " linux-$KVER.tar.xz" sha256sums.asc | sha256sum -c -
tar -xf linux-$KVER.tar.xz
echo $KVER > KERNEL_VERSION
```
Record `KVER` and the date in `DECISIONS.md`. A hash mismatch is fatal: re-download once, then use the blocked protocol.

### 6.2 Configure (`scripts/11-kernel-config.sh`)
```
cd third_party/linux-$KVER
make defconfig
make kvm_guest.config
scripts/kconfig/merge_config.sh -m .config ../../image/kernel/aios.fragment
make olddefconfig
```
Then run `scripts/12-kernel-verify-config.sh`, which greps `.config` and fails if any required symbol from the fragment is not `=y` (or not the requested value). Print the list of failures; fix the fragment (some symbols have changed names or dependencies between versions: use `make menuconfig`-free fixes, e.g. find the symbol with `grep -r "config NAME" --include=Kconfig*` and enable its dependencies).

### 6.3 `image/kernel/aios.fragment`
```
CONFIG_PRINTK_TIME=y
CONFIG_EFI=y
CONFIG_EFI_STUB=y
CONFIG_DEVTMPFS=y
CONFIG_DEVTMPFS_MOUNT=y
CONFIG_TMPFS=y
CONFIG_TMPFS_POSIX_ACL=y
CONFIG_TMPFS_XATTR=y
CONFIG_PROC_FS=y
CONFIG_SYSFS=y
CONFIG_CGROUPS=y
CONFIG_CGROUP_BPF=y
CONFIG_MEMCG=y
CONFIG_CGROUP_PIDS=y
CONFIG_CGROUP_CPUACCT=y
CONFIG_CGROUP_SCHED=y
CONFIG_CGROUP_FREEZER=y
CONFIG_NAMESPACES=y
CONFIG_USER_NS=y
CONFIG_PID_NS=y
CONFIG_NET_NS=y
CONFIG_UTS_NS=y
CONFIG_IPC_NS=y
CONFIG_SECCOMP=y
CONFIG_SECCOMP_FILTER=y
CONFIG_SECURITY=y
CONFIG_SECURITY_LANDLOCK=y
CONFIG_LSM="landlock,lockdown,yama,integrity"
CONFIG_FHANDLE=y
CONFIG_SIGNALFD=y
CONFIG_TIMERFD=y
CONFIG_EPOLL=y
CONFIG_EVENTFD=y
CONFIG_INOTIFY_USER=y
CONFIG_AUTOFS_FS=y
CONFIG_NET=y
CONFIG_UNIX=y
CONFIG_INET=y
CONFIG_IPV6=y
CONFIG_PACKET=y
CONFIG_CRYPTO_USER_API_HASH=y
CONFIG_CRYPTO_HMAC=y
CONFIG_CRYPTO_SHA256=y
CONFIG_VIRTIO=y
CONFIG_VIRTIO_PCI=y
CONFIG_VIRTIO_BLK=y
CONFIG_VIRTIO_NET=y
CONFIG_VIRTIO_CONSOLE=y
CONFIG_VIRTIO_INPUT=y
CONFIG_HW_RANDOM_VIRTIO=y
CONFIG_BLK_DEV_NVME=y
CONFIG_EXT4_FS=y
CONFIG_VFAT_FS=y
CONFIG_FAT_FS=y
CONFIG_NLS_CODEPAGE_437=y
CONFIG_NLS_ISO8859_1=y
CONFIG_NLS_UTF8=y
CONFIG_EROFS_FS=y
CONFIG_SQUASHFS=y
CONFIG_OVERLAY_FS=y
CONFIG_BLK_DEV_DM=y
CONFIG_DM_VERITY=y
CONFIG_DRM=y
CONFIG_DRM_VIRTIO_GPU=y
CONFIG_DRM_BOCHS=y
CONFIG_DRM_FBDEV_EMULATION=y
CONFIG_INPUT=y
CONFIG_INPUT_EVDEV=y
CONFIG_SND=y
CONFIG_SND_HDA_INTEL=y
CONFIG_SND_HDA_GENERIC=y
CONFIG_SERIAL_8250=y
CONFIG_SERIAL_8250_CONSOLE=y
```
If the Debian userland (systemd, labwc, PipeWire) later complains about a missing kernel feature, add the symbol here, note it in `DECISIONS.md`, and rebuild. Modules stay enabled in the config; the QEMU image ships none because everything it needs is built in. T8 adds `image/kernel/hardware.fragment` (applied the same way) and installs modules and firmware for real hardware.

### 6.4 Build (`scripts/13-kernel-build.sh`, background, log to `logs/kernel-build.log`)
```
cd third_party/linux-$KVER
make -j"$(nproc)" bzImage
cp arch/x86/boot/bzImage ../../build/bzImage
```
Expect 10 to 60 minutes. Use `ccache` (`export CC="ccache gcc"`) if present. Success check: `file build/bzImage` reports a Linux kernel x86 boot executable. Quick smoke test before building the image:
```
qemu-system-x86_64 -accel kvm -m 1G -nographic -kernel build/bzImage -append "console=ttyS0 panic=1" -no-reboot
```
It must reach the "Kernel panic - not syncing: VFS: Unable to mount root fs" line (no root given), which proves the kernel boots. Record the boot log in `logs/kernel-smoke.log`.

---

## 7. Protocol (implemented in `crates/protocol`)

Every message has `ts_ns` (monotonic ns), `task_id` (uuid string), `seq` (u64 increasing per task).

**Requests (JSON-RPC 2.0, client to aiosd)**
- `task.start { text }` returns `{ task_id }`
- `task.cancel { task_id }` returns `{ ok }`
- `approval.respond { task_id, approval_id, allow }` returns `{ ok }`

**Events (aiosd to client, one JSON object per line)**
```
plan_created     { steps: [{ id, title, action, args }] }
step_started     { step_id }
step_done        { step_id, summary }
step_failed      { step_id, reason }
approval_needed  { approval_id, step_id, action, args, risk }
result           { summary }
error            { message }
cancelled        { }
trace            { stage, dt_ns }
```
Provide serde types with round-trip tests for every message and a golden JSON fixture file per message under `crates/protocol/tests/`.

---

## 8. Risk policy (`crates/broker`)

| Level | Behaviour | Actions |
|---|---|---|
| `free` | runs without asking | `web.search`, `media.play`, `notes.create` |
| `ask` | emits `approval_needed`, waits for `approval.respond` | `app.launch`, anything unlisted |
| `never` | always denied, always logged | `fs.read` outside `read_roots`; `fs.write` outside the notes dir; any shell execution; any network call except through `web.search`/`media.play` |

Rules:
- The broker is the only caller of `crates/actions`. `aiosd` sends an `ActionRequest`; the broker returns `Allowed`, `NeedsApproval`, or `Denied`.
- Decisions are in-memory lookups: no disk, no network in the decision path.
- Audit log: one JSON line per decision and per result: `ts, task_id, step_id, action, args, decision, outcome`.
- **Untrusted content:** text from the web, files, tool results, and apps is tagged `untrusted`, given to the model only inside a delimited data block, and can never create or approve a grant. Approvals come only from `approval.respond` on `approve.sock`, which only the shell can reach (launched apps are sandboxed away from it).
- Cancel revokes pending approvals and aborts running steps.

---

## 9. Tasks

Order matters: **the real, bootable system comes first, and every later feature is verified inside it.** Mark acceptance checks in `CHECKLIST.md` with links to evidence before moving on. Update `STATE.md` as you go.

### T0: Host bootstrap, repo skeleton, anti-fake guard
Do section 4, create the layout in section 5, an empty Cargo workspace, `check.sh` (`cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && scripts/check-no-fake.sh`), `.gitignore`, and write `scripts/check-no-fake.sh` as specified.
**Accept:** `scripts/02-verify-tools.sh` exits 0; `./check.sh` passes; the guard script is proven to work by a test that plants a forbidden file in a temp copy and sees the script fail.

### T1: Kernel
Section 6 in full. Start the background build as early as possible and keep working while it compiles.
**Accept:** `build/bzImage` exists, version recorded, the QEMU smoke test reaches the expected panic line, `scripts/12-kernel-verify-config.sh` exits 0. Evidence: `tests/evidence/T1/`.

### T2: `protocol`, `tracer`, `client`
- All types from section 7 with serde, round-trip tests, and golden JSON fixtures.
- `tracer`: `Tracer::stage("name")` guard, `timeline()` table (stage, ms, cumulative ms), `trace` events.
- `client`: a Rust library other crates use to talk to `aiosd` over the Unix socket (requests plus event stream, reconnect with backoff).
**Accept:** tests pass; a tracer test prints a three-stage timeline.

### T3: Walking skeleton: a real image boots to a real desktop (the first milestone that matters)
Goal: prove the real OS exists before building features. No agent yet.

**T3a Rootfs (`scripts/20-rootfs.sh`)**
- `mkosi`, `Format=directory`, `Distribution=debian`, current stable, packages: `systemd systemd-sysv dbus libpam-systemd udev seatd labwc foot firefox-esr mousepad bubblewrap iwd pipewire wireplumber pipewire-pulse fonts-inter fonts-dejavu-core ca-certificates`.
- Users: `aios` (uid 1000, groups `video,input,render,audio`) runs the desktop; `aiosd` (system user) runs the daemon.
- **Session:** `aios-session.service` runs labwc on tty1 using the standard kiosk-session pattern: `PAMName=aios-session` (create `/etc/pam.d/aios-session` including `pam_systemd`), `TTYPath=/dev/tty1`, `StandardInput=tty-fail`, `UtmpIdentifier=tty1`, `User=aios`, `ExecStart=/usr/bin/labwc -C /etc/aios/labwc -s /usr/libexec/aios/session-start`. labwc `autostart` starts `aios-shell`. The session must not wait for the network.
- **labwc config (`image/rootfs/etc/labwc/`)**: fixed and locked down. One workspace; no root menu (empty `menu.xml`); server-side title bars with a single theme (`aios`) matching the shell style; keybinds only `Super+Space` (runs `aios-shell --toggle-palette`), `Alt+F4` (close window), `Super+Tab` (next window). No other bindings, no user-editable settings UI.
- **Read-only root**: erofs root, tmpfs for `/run`, `/tmp`, `/var`; persistent data partition mounted at `/var/lib/aios`; machine-id set at build time. Resolve ordering with `systemd-tmpfiles` and mount dependencies; record the final approach in `DECISIONS.md`.
- Mask unneeded units (`apt-daily*`, `man-db`, `e2scrub*`, getty on tty2 to tty6).
- Pack with `mkfs.erofs -zlz4hc build/rootfs.erofs build/rootfs/`.
- Run `scripts/check-no-fake.sh` against the built rootfs.

**T3b Disk (`scripts/21-disk.sh`, no loop mounts, no root)**
- `build/esp.img` (FAT32, 256 MB) built with `mtools`: `EFI/BOOT/BOOTX64.EFI` (systemd-boot), `EFI/Linux/aios.efi` (UKI), `loader/loader.conf` with `timeout 0`.
- UKI: `ukify build --linux build/bzImage --cmdline "root=PARTUUID=<root-partuuid> ro rootfstype=erofs quiet loglevel=3 printk.time=1 systemd.show_status=0" --output build/aios.efi` (no initrd).
- `build/data.img` (ext4, 1 GB, label `aios-data`) with `user/` and `notes/` pre-created and owned by uid 1000.
- Assemble `build/aios.img` with `sfdisk` (GPT: ESP, root with fixed PARTUUID, data) and `dd` at offsets.
- Two variants from the same script: **product** (no serial shell) and **test** (`AIOS_TEST_IMAGE=1`: adds an autologin root debug shell on `ttyS0` so tests can run commands inside the VM). The test variant is never shipped.
- `scripts/factory-reset.sh` rewrites the data partition.

**T3c Run and prove (`scripts/22-run-qemu.sh`, `tests/boot/`)**
```
qemu-system-x86_64 -accel kvm -m 3G -smp 2 -machine q35 \
  -drive if=pflash,format=raw,readonly=on,file=<OVMF_CODE> \
  -drive if=pflash,format=raw,file=build/OVMF_VARS.fd \
  -drive file=build/aios.img,format=raw,if=virtio \
  -device virtio-gpu-pci -device virtio-keyboard-pci -device virtio-tablet-pci \
  -nic user,model=virtio-net-pci \
  -monitor unix:build/qemu-mon.sock,server,nowait \
  -serial mon:stdio -display none      # use -display gtk for a human to watch
```
(Find real OVMF file names with `ls /usr/share/OVMF`; copy VARS into `build/` first.)
- `tests/boot/boot_test.py` boots the **test** image, waits for the serial shell, then asserts inside the VM: `uname -r` equals our kernel version, `pgrep labwc` and `systemctl is-active aios-session` succeed, `findmnt /` shows erofs read-only, the data partition is mounted at `/var/lib/aios`.
- Take a screenshot with the QEMU monitor (`screendump build/shot.ppm`, convert to PNG with `pnmtopng` or Pillow) and save it to `tests/evidence/T3/boot.png`. Verify the screenshot is not blank (more than N distinct colours) and visually describe it in the evidence notes.
- Launch a real app inside the VM (`foot` through labwc, driven from the serial shell with `WAYLAND_DISPLAY` set) and screenshot a real window on the real desktop.
**Accept:** the image boots under UEFI with our kernel into a labwc desktop; boot test passes; screenshots exist; boot timeline recorded in `tests/latency/RESULTS.md`.

### T4: Native desktop shell (Rust, Wayland layer-shell)
Build `crates/shell`, a native Wayland client. No web technology.
- Stack: `smithay-client-toolkit` (layer-shell, seat, shm), `tiny-skia` for drawing into `wl_shm` buffers, `cosmic-text` for text. Hand-built minimal widgets (label, button, text field with caret and backspace, list, panel). Do not add a widget framework unless this stack fails twice; if so log the switch in `DECISIONS.md`.
- Surfaces: **top bar** (layer `top`, anchored top, exclusive zone, status dot, "Ask" chip with the shortcut hint, clock); **palette panel** (layer `overlay`, centered, keyboard-interactive on demand, rounded and softly shadowed, containing the input field, the plan list with check ticks, the status line, a Stop button, an approval dialog, and the result view); **first-run setup panel** (AI endpoint `IP:port`, model name, API key, Test button).
- Visual style follows the user's mockup: light translucent panels, large radius, calm spacing, one sans typeface plus small monospace labels, no settings screens. Draw the soft blur as a pre-blurred static backdrop, not live blur.
- Behaviour: `Super+Space` and clicking the top bar open the palette (through `aios-shell --toggle-palette` talking to the running shell over a private socket); Enter submits through `client`; plan and result states render from events; reconnect state when the daemon is away; "AI not connected" state when unconfigured.
- Frame budget: keypress to rendered frame under 2 frames at 60 Hz in the VM on KVM; log it with the tracer.
- Test-only support: `tests/testkit` provides a **scripted event server** (clearly labelled test-only, excluded from the image) so the shell can be tested in the VM before `aiosd` exists.
**Accept (in the booted image):** the shell autostarts under labwc; screenshots show the top bar, the open palette, a plan ticking (driven by the test-only event server running on the host over the network or inside the test image), the approval dialog, and the setup panel; keyboard input typed through the QEMU monitor (`sendkey`) reaches the text field; `check-no-fake.sh` still passes.

### T5: `aiosd`, `broker`, `actions` (real system actions)
- `aiosd` runs as a system service as user `aiosd`. Sockets: `/run/aios/agent.sock` (task start/cancel from the shell) and `/run/aios/approve.sock` (approvals; reachable only by the shell). Check `SO_PEERCRED`. The directory is mode 0700 for the sockets' owner and hidden from launched apps.
- `broker` implements section 8 and the audit log. `actions` implements, with real effects:
  - `app.launch { name }`: allow-list (`browser`→firefox-esr, `editor`→mousepad, `terminal`→foot [ask]). Spawn with `bubblewrap` (new namespaces, no `/run/aios`, no data partition except the app's own scratch dir), the Wayland socket bound in, running as the desktop user. No raw command strings anywhere.
  - `web.search { query }`: `browser_fallback` opens `https://duckduckgo.com/?q=<urlencoded>` through `app.launch`; `http_json` calls a configured SearXNG-style URL and returns titles and URLs tagged `untrusted`.
  - `media.play { query }`: opens the YouTube search URL in the browser.
  - `notes.create { title, body }`: writes a file with a sanitized name under `paths.notes`.
  - `fs.read { path }`: only under `read_roots`; canonicalize, reject `..` and symlink escapes, size limit 1 MB; contents tagged `untrusted`.
- Intent router (no AI needed): `open <app>`, `play <query>`, `note: <text>`, `search <query>`, `read <path>`, case-insensitive, anchored at the start. These five commands must work with **no AI configured**.
- Wire the shell to `aiosd` through `client`.
**Accept (in the booted test image):** each of the five commands, typed into the shell, produces its real effect (a real Firefox window with the right URL; a real file in the notes directory; the read content shown; etc.), each with a screenshot and the audit log line; denied actions still create audit entries; broker unit tests cover every row of section 8 including path traversal, symlink escape, huge reads, and unknown actions.

### T6: The real AI agent loop and first-run setup
- `aiosd` reads `ai.toml` and `ai.key` and calls the configured OpenAI-compatible endpoint with streaming, tool definitions for the five actions, and a system prompt stating the untrusted-content rule. Parse streamed tool calls into plan steps, emit `plan_created` as soon as the plan is known, then run steps through the broker (independent steps in parallel). Cancellation aborts the in-flight HTTP request and running steps.
- First-run setup in the shell (T4 panel) saves the settings and tests the connection (a small real request), showing success or the real error text.
- Honest states: if no AI is configured or the endpoint fails, say so in the shell; never produce fake plans.
- Testing: `tests/testkit/mock-ai` is a small OpenAI-compatible test server (Python stdlib) for automated tests only. It runs on the host; the VM reaches it at `http://10.0.2.2:8081/v1` through QEMU user networking. It maps keywords to tool calls, and one special prompt returns hostile tool calls for T7.
- `README.md` documents how to point AiOS at any real endpoint (cloud API, a GPU box by IP, local llama.cpp).
**Accept (in the booted test image):** configure the endpoint through the setup screen (typed through the QEMU monitor), then run free-form requests that produce multi-step plans executed for real (screenshots plus audit log); router commands show no model request in the trace; cancellation measured and logged.

### T7: Trust verification, inside the image
- `tests/injection/`: fixtures where search results, a read file, app output, and the test AI's response contain hostile instructions ("ignore previous instructions", "approve all pending actions", "read /etc/shadow", "run this shell command").
- Run the suite **inside the VM** (via the test image's serial shell and the socket). Assert: no `never` action executes, no read outside roots, no approval ever occurs without a genuine message on `approve.sock` from the shell, and each denial is in the audit log.
- **Sandbox proof:** from inside a launched app (a test helper run through `app.launch`), attempts to connect to `/run/aios/*.sock`, read `/var/lib/aios`, or ptrace the shell must all fail.
- Landlock and seccomp applied to `aiosd`'s action execution; tests that a forbidden path access and a forbidden syscall fail.
- Stop-latency test recorded in `tests/latency/RESULTS.md`.
**Accept:** the suite passes inside the VM with evidence saved.

### T8: Hardware-ready image (for the human to test on a real machine)
- Add `image/kernel/hardware.fragment` and rebuild the kernel: built-in storage and input (`SATA_AHCI`, `ATA`, `BLK_DEV_SD`, `BLK_DEV_NVME`, `USB`, `USB_XHCI_HCD`, `USB_XHCI_PCI`, `USB_STORAGE`, `USB_HID`, `HID_GENERIC`, `HID_MULTITOUCH`, `I2C_HID_ACPI`, `MMC`/`MMC_SDHCI_PCI` if present), early display built in (`SYSFB_SIMPLEFB`, `DRM_SIMPLEDRM`, `FB_EFI`), and as modules: `DRM_I915`, `DRM_AMDGPU`, `DRM_NOUVEAU`, Wi-Fi (`IWLWIFI`, `ATH*`, `RTW*`, `MT76*`), Bluetooth, audio (`SND_HDA_*`, `SND_SOC_SOF*`), `THINKPAD_ACPI`, `ACPI_*`. Run `make modules_install INSTALL_MOD_PATH=build/modules` and copy `/lib/modules/<kver>` into the rootfs.
- Add firmware: enable Debian's `non-free-firmware` component in mkosi and install `firmware-linux firmware-iwlwifi firmware-amd-graphics firmware-realtek firmware-atheros firmware-misc-nonfree`.
- Networking on real machines: `iwd` plus `systemd-networkd`; the shell top bar gets a small native Wi-Fi picker (list networks via iwd's D-Bus API, enter a password in a native field).
- Output `build/aios-usb.img` (hybrid GPT image, sized to fit a 16 GB stick, with a bigger data partition that grows on first boot via `systemd-repart`), plus `scripts/write-usb.sh <device>` which **refuses** to write to a mounted or system disk, requires the exact device path, and prints what it will erase. The agent must never run `write-usb.sh`.
- Verify what can be verified without hardware: boot the product image in QEMU with `-device nvme`, `-device ahci`, `-device usb-ehci`/`qemu-xhci` USB storage variants, and confirm it reaches the desktop each time.
- Write `docs/HARDWARE_TEST.md` (for the human): how to write the stick, how to boot it (UEFI, disable Secure Boot for 0.1), what should appear, how to collect logs (`journalctl -b`, `dmesg`) if it fails.
**Accept:** the image boots in each QEMU hardware variant with screenshots; the USB image and docs exist. The final physical-hardware boot is the human's step and is reported as pending in `REPORT.md`.

### T9 (optional, skip if anything above is incomplete): Voice
Push-to-talk key handled by the native shell; audio capture through PipeWire; streaming STT with whisper.cpp (clone the current official repo, build with cmake, download a small English model with its script); TTS with Piper (find the current official release and a small English voice; record exact URLs in `DECISIONS.md`); listening and speaking states in the native shell; per-stage latency in the tracer.
**Accept:** a recorded WAV fixture is transcribed correctly inside the VM test image; the shell shows the states; latencies recorded.

---

## 10. Testing, evidence, and measurement rules

- **In-image evidence is mandatory.** For each task, `tests/evidence/<task>/` holds serial log excerpts, command outputs from the test shell, and PNG screenshots from QEMU's `screendump`. `CHECKLIST.md` links to them.
- Screenshot helper: send `screendump build/shot.ppm` to the monitor socket (`socat - UNIX-CONNECT:build/qemu-mon.sock`), convert with `pnmtopng`, and assert non-blank. Type into the VM with the monitor's `sendkey` or `sendkey` sequences from a helper script.
- Tracer timestamps everywhere. Required stages: keypress to rendered frame, command to `plan_created`, broker decision, action duration, boot (kernel start, init start, `aiosd` ready, shell visible).
- Budgets to aim at (adjust with data, report misses honestly): keypress to frame within 2 frames; router overhead under 300 ms; broker decision under 1 ms; QEMU boot to desktop around 5 s with KVM.
- Do not optimize without a measurement.

## 11. Troubleshooting playbook (try before declaring a blocker)

- **Kernel build fails for missing headers or tools:** install the missing `-dev` package. If `dwarves`/`pahole` errors appear, set `CONFIG_DEBUG_INFO_BTF=n`.
- **Panic "unable to mount root":** check `root=PARTUUID=` matches `sfdisk`, `EROFS_FS` and `VIRTIO_BLK` are `=y`, run `scripts/12-kernel-verify-config.sh`.
- **No UEFI boot:** confirm OVMF paths, `EFI/BOOT/BOOTX64.EFI` on the ESP, and the UKI under `EFI/Linux/`.
- **systemd hangs:** boot with `systemd.log_level=debug console=ttyS0`, read for missing kernel features, add symbols to the fragment.
- **labwc fails to start:** check `seatd`/logind, that `aios` is in `video,input,render`, that `/dev/dri/card0` exists (QEMU needs `virtio-gpu-pci`), and set `WLR_RENDERER=pixman` for software rendering in the VM.
- **Shell shows nothing:** run `aios-shell` from the test shell with `WAYLAND_DISPLAY=wayland-0` and read its stderr; confirm the compositor supports `wlr-layer-shell` (labwc does).
- **Firefox fails in the sandbox:** add only the specific bind mounts it needs (Wayland socket, fonts, its own profile dir); never bind `/run/aios` or the data partition.
- **Read-only root errors:** find the path that wants to write (`strace`, `journalctl`) and give it a tmpfs or the data partition; do not make the root writable.
- **No KVM:** use `-accel tcg`, enlarge timeouts, mark latency as emulated.
- **Out of disk:** `make clean` the kernel tree after copying `bzImage`; prune `target/`.

## 12. Parallelism hint

Start T1's kernel build in the background right after T0. Write T2 and the shell skeleton (T4's drawing and widget code) while it compiles. T3 needs the kernel; T5 onward need T3 and T4.

## 13. Not in 0.1 (do not build)

Our own Smithay compositor (labwc is used for now), A/B updates, Secure Boot, dm-verity enforcement, face or voice recognition, video, multi-user, third-party apps and packaging (Appendix A), local LLM, kernel patches, anything web-based as UI.

## 14. Final report (`REPORT.md`)

What works and where the evidence is, the ticked `CHECKLIST.md`, the latency table (KVM or emulated), the kernel version and config deltas, anything that failed or was cut (with log excerpts), `BLOCKERS.md` contents, the pending human step (physical hardware boot), and a ranked list of next steps. Then print a short summary for the human: how to boot it in QEMU (`scripts/22-run-qemu.sh`), how to write the USB stick (`docs/HARDWARE_TEST.md`), and how to connect the AI (first-run setup screen).

---

## 15. Roadmap after 0.1 (informational: do not build any of this in 0.1)

| Version | Goal |
|---|---|
| 0.1 | This document: real boot, our kernel build, native shell, real agent, broker, five real actions, trust tests, hardware-ready image |
| 0.2 | App manifest and sandbox runner; apps expose typed actions to the agent (Appendix A, tasks A1 to A4) |
| 0.3 | `.aiapp` package format with signing; local install and uninstall; agent-driven install flow (Appendix A, B1 to B4) |
| 0.4 | Signed catalog, verified downloads, atomic updates with rollback, agent catalog search (Appendix A, C1 to C4) |
| 0.5+ | Own compositor on Smithay (replacing labwc); A/B image updates, dm-verity and Secure Boot; user recognition at boot (face and voice); voice and video chat; local small model for the fast path |

**Kernel changes policy.** Most "AI-first" features need no kernel change. Sort any idea into: (1) configuration only (strip drivers, enable Landlock, seccomp, cgroups; fastest boot); (2) loadable modules or eBPF (per-agent accounting, syscall-level enforcement of broker rules); (3) real kernel patches (a fork, which means rebasing on every release). Do (1) and (2) first; do (3) only when a measurement shows it is the only way.

**Executable and package formats.** Linux already runs ELF programs, so AiOS does not invent a new executable format. What it will add later is a *package* format (`.aiapp`: a signed read-only image plus a manifest declaring permissions and agent-callable actions) and an app manager that is driven by the agent. There is no dependency solver: apps are self-contained bundles. Running Windows `.exe` files (through Wine) is optional and far off; macOS `.dmg` apps cannot run on Linux. Installers that put AiOS on other machines (like a USB writer for Windows or macOS) are a separate, later tool.

---

# Appendix A: Plan for 0.2 to 0.4 (apps and the app manager), start only after 0.1 is done

Only start this after every 0.1 acceptance check is ticked and REPORT.md exists. The operating contract in section 1 and the decisions in section 3 apply here too: never ask the human to download or run anything, decide and log, keep STATE.md current. The former ASK items are resolved by these defaults: ship no third-party publisher keys except a locally generated dev key, host the catalog as a local file served by a test HTTP server, and keep developer mode (unsigned apps with a visible warning) off by default but available behind a config flag; log these choices in DECISIONS.md.

### 0. Goal

Let third-party apps exist without breaking the simple user view or the security model.

- **0.2:** an app manifest and a sandbox runner. Apps can expose typed actions to the agent.
- **0.3:** the `.aiapp` package format with signing, local install and uninstall.
- **0.4:** a signed catalog, downloading, atomic updates with automatic rollback.

The user never types package commands. The agent searches, explains permissions in plain language, asks approval, and installs. A developer CLI exists but is secondary.

### 1. Design decisions (fixed)

- **No dependency solver.** Apps are self-contained bundles. At most one shared runtime is allowed (a versioned base) and it is referenced by exact hash.
- **No custom crypto.** Use ed25519 signatures (`ed25519-dalek`) and SHA-256 (`sha2`). Keys are stored and handled as raw files; do not invent schemes.
- **No install scripts.** Installing means verify, place, mount, register. Nothing from the package is executed at install time.
- **Permissions in the manifest map to broker risk levels.** An app can never grant itself anything; the broker and the user decide.
- **Actions are declared, typed, and discoverable.** The agent only knows about actions that appear in a verified manifest.
- **Apps are untrusted code.** Everything an app returns is tagged `untrusted` in the broker (same rule as web content).

### 2. New repo layout (add to the existing workspace)

```
crates/
  manifest/      # manifest types, parser, validation, JSON schema
  sandbox/       # builds and applies a sandbox from a manifest
  appd/          # app host: starts apps, brokers their action sockets
  aiapp/         # package format: pack, verify, unpack (library)
  pkgd/          # installer/updater service (privileged, minimal)
  catalog/       # catalog types, signature check, fetcher (0.4)
tools/
  aiapp-cli/     # developer CLI: pack, sign, verify, install, uninstall, list
examples/
  hello-app/     # sample app used by tests
tests/
  apps/          # sandbox escape tests, malformed package tests
```

### 3. Manifest spec (`manifest.toml`, task A1 implements and validates it)

```toml
[app]
id        = "org.example.notes"        # reverse-DNS, lowercase, validated
name      = "Notes"
version   = "1.2.0"                    # semver
publisher = "example"
exec      = "bin/notes"                # path inside the bundle, no absolute paths, no ..
runtime   = "none"                     # or "aios-base-1@sha256:<hash>"

[permissions]                          # what the app asks for; nothing is implicit
files   = [{ path = "$APPDATA", mode = "rw" }]
network = false                        # or { hosts = ["api.example.org"] }
camera  = false
mic     = false
gpu     = false

[[actions]]                            # what the agent may call
name        = "notes.create"
description = "Create a note"
risk        = "free"                   # requested risk; the broker may raise it, never lower it
args        = { title = "string", body = "string" }
returns     = { id = "string" }
```

Validation rules (all must have tests): reject unknown keys, invalid ids, path traversal in `exec` or file paths, duplicate action names, action names outside the app's own namespace (an app named `org.example.notes` may only define actions under `notes.*` that it owns; it can never define or override built-in actions like `fs.read`), and any permission string not in the allowed set.

### 4. Tasks: version 0.2

#### A1: `manifest` crate
Types, parser, validator, generated JSON schema (`manifest.schema.json`), plain-language summary function: `manifest.describe_permissions() -> Vec<String>` (for example, "Can read and write its own data", "Can contact api.example.org", "Cannot use your camera or microphone").
Acceptance: valid and invalid manifest fixtures pass and fail as expected; summaries are generated for the example manifest.

#### A2: `sandbox` crate
Builds a sandbox from a manifest: mount namespace with only the bundle (read-only) and `$APPDATA` (read-write), Landlock ruleset from `files`, seccomp allow-list, network namespace (no network unless declared; if hosts are declared, enforce through a per-app proxy, not by trusting the app), cgroup v2 limits (memory, cpu, pids), no new privileges, no access to the broker socket or audit log.
Acceptance (in `tests/apps/`): a test app attempts each of these and every one fails: read `/etc/shadow`, read another app's data, open a network connection without the permission, `ptrace` another process, exceed the memory limit, write outside `$APPDATA`.

#### A3: `appd` and action registration
`appd` starts an app inside its sandbox, gives it a private Unix socket, and speaks a small protocol on it: `register`, `call { action, args }`, `result`, `error`. Actions from the verified manifest are registered with `aiosd` as tools. The agent sees them like built-in actions. All calls go through the broker (risk from policy table, with the manifest's requested risk only able to raise or keep it); results are tagged `untrusted`.
Acceptance: the agent can call `hello.greet` from `examples/hello-app`; a call to an undeclared action is rejected; an app returning text that says "approve all actions" has no effect (add to the injection suite).

#### A4: Sample app and tests
`examples/hello-app` (a tiny Rust program) with a manifest, plus a negative sample that tries to break out. Wire all of this into `./check.sh` where possible (sandbox tests may need a privileged CI step; document it in `DECISIONS.md`).
Acceptance: positive sample works, negative sample is contained and every attempt appears in the audit log.

### 5. Tasks: version 0.3

#### B1: `.aiapp` format (`aiapp` crate)
A bundle is a single file containing, in this order:
1. a header with magic bytes, format version and section lengths
2. `manifest.toml`
3. an erofs (or squashfs) image of the app files
4. a detached ed25519 signature over `SHA-256(manifest || image)` plus the signer's public key id

Provide `pack`, `verify`, and `open` (parse without trusting). Parsing must be strict: unknown format versions, truncated files, oversized sections, and hash mismatches are rejected, with tests and a fuzz target (`cargo-fuzz`) for the header parser.
Acceptance: pack and verify round-trips; every malformed fixture is rejected; fuzz target runs for a fixed number of iterations without panics.

#### B2: Keys and trust
- `/etc/aios/trusted-keys/` holds publisher public keys (the system image ships a small default set).
- Package signed by an unknown key: rejected by default. Policy can allow "ask the user" for developer mode, off by default.
- Provide `aiapp-cli keygen`, `sign`, and `verify`.
Acceptance: a package signed with a trusted key verifies; wrong key, tampered manifest, and tampered image all fail with distinct clear errors.

#### B3: `pkgd` installer
Privileged service with the minimum capabilities. `install(file)`: verify, check version rules, copy to `/var/lib/aios/apps/<id>/<version>.aiapp`, create the data directory, register with `appd`. `uninstall(id, keep_data: bool)`. `list()`. No package-supplied code runs at install time.
Acceptance: install, run, uninstall works end to end with `hello-app`; uninstall with `keep_data=true` leaves the data directory; a failed install leaves no partial state (test by killing the process mid-install).

#### B4: Agent-driven install flow
Add built-in action `app.install { source }` (risk `ask`). The approval prompt shows `describe_permissions()` output and the publisher, not raw manifest text. The agent may suggest an install but can never approve it.
Acceptance: asking "install hello" produces an approval prompt with plain-language permissions; denying it installs nothing; approving it installs and the new action becomes callable in the same session.

### 6. Tasks: version 0.4

#### C1: Catalog format (`catalog` crate)
`catalog.json`: list of apps with id, name, description, versions, SHA-256 of each `.aiapp`, download URLs, requested permissions, and an expiry timestamp; the whole file is signed by the catalog key. Include a revocation list (revoked package hashes and revoked keys).
Acceptance: signature and expiry checks tested; an expired or tampered catalog is rejected; a revoked hash blocks install and flags an installed app.

#### C2: Fetcher
Download with HTTPS, resumable, SHA-256 verified against the catalog before the file is handed to `pkgd`. The fetcher never writes outside its staging directory.
Acceptance: interrupted downloads resume; a corrupted download is rejected; a mismatched hash never reaches `pkgd`.

#### C3: Atomic updates with rollback
Install the new version next to the old one, switch a single pointer atomically, run a health check (the app starts and registers its actions within a timeout), and roll back automatically on failure. Keep the previous version until the new one has run successfully once.
Acceptance: a deliberately broken update rolls back without user action; the audit log records both attempts.

#### C4: Agent catalog search
Built-in action `app.search { query }` (risk `free`) over the cached catalog. Results are tagged `untrusted` and show the publisher and requested permissions.
Acceptance: "I need a PDF editor" finds catalog entries; the agent never installs without the `app.install` approval flow.

### 7. Security tests to add (must all pass before the 0.4 report)

- Sandbox escape suite (A2) runs on every build.
- Malformed package suite and fuzz target (B1).
- Downgrade test: installing an older version of an installed app is rejected unless explicitly allowed.
- Confused-deputy test: an app cannot make the agent call actions the app was not granted.
- Name-squatting test: an app cannot register actions in another app's or the built-in namespace.
- Prompt-injection test through app results and catalog descriptions.

### 8. Not in 0.2 to 0.4 (do not build)

Public publishing portal, payments, delta updates, multiple shared runtimes, Wine or Windows compatibility, macOS compatibility, GUI app store, multi-user installs, remote attestation.

### 9. Decisions already made (formerly ASK items)

- Trusted keys: ship only a locally generated dev key; the catalog signing key is also a generated dev key kept in `build/keys/` (gitignored). The human replaces both before any public release.
- Catalog location: a local file plus a test HTTP server on 127.0.0.1 for 0.4; the URL is a config field.
- Developer mode: exists behind a config flag, off by default, shows a visible warning when on.

### 10. Final report

Write `REPORT-0.4.md`: what works, ticked CHECKLIST, results of the security suite, install and update timings, anything cut or failing, and a ranked list of next steps. Report any sandbox test that fails honestly; do not weaken a test to make it pass.
