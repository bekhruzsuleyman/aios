# AiOS

An AI-first operating system on the Linux kernel: our own kernel build, a read-only image, a native Rust Wayland desktop shell, and an agent daemon whose every action passes a permission broker.

Status: 0.1 in progress. See `PLAN.md` (the plan), `STATE.md` (where the build is), `CHECKLIST.md` (what is proven, with evidence) and `DECISIONS.md`.

## Building
```
scripts/00-host-check.sh      # what this host can do
scripts/01-install-tools.sh   # host packages, mkosi, Rust
scripts/02-verify-tools.sh
scripts/10-kernel-fetch.sh && scripts/11-kernel-config.sh && scripts/13-kernel-build.sh
./check.sh                    # fmt, clippy, tests, anti-fake guard
```
