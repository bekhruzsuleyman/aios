//! Kernel-enforced limits on aiosd itself, applied before any thread or
//! socket exists, so every action the broker runs is inside them:
//!
//! - Landlock: read-only `/usr`, `/etc`, `/proc`, `/sys` and the read
//!   roots; read-write only the data and socket directories. Everything
//!   else (other users' homes, `/root`, `/tmp`, the rest of `/var`) is
//!   invisible to the process.
//! - seccomp: aiosd never starts programs or debugs processes, so
//!   `execve`, `ptrace`, namespace and mount syscalls fail with EPERM.
//!   Model output can therefore never become a command, even through a bug.

use crate::config::Config;
use landlock::{
    Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus,
    ABI,
};
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, SeccompRule, TargetArch};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// "fully_enforced", "partially_enforced" or "not_enforced" (kernel
    /// without Landlock).
    pub landlock: &'static str,
    pub seccomp: bool,
}

/// Syscalls aiosd must never make.
const DENIED_SYSCALLS: &[i64] = &[
    libc::SYS_execve,
    libc::SYS_execveat,
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_mount,
    libc::SYS_umount2,
    libc::SYS_pivot_root,
    libc::SYS_chroot,
    libc::SYS_setns,
    libc::SYS_unshare,
    libc::SYS_bpf,
    libc::SYS_init_module,
    libc::SYS_finit_module,
    libc::SYS_delete_module,
    libc::SYS_kexec_load,
    libc::SYS_reboot,
    libc::SYS_swapon,
    libc::SYS_swapoff,
];

fn dirs(cfg: &Config) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let p = &cfg.paths;
    let mut ro: Vec<PathBuf> = ["/usr", "/etc", "/proc", "/sys", "/lib", "/lib64", "/bin"]
        .iter()
        .map(PathBuf::from)
        .collect();
    ro.extend(p.read_roots.iter().cloned());
    let parent = |f: &Path| f.parent().map(Path::to_path_buf);
    let rw: Vec<PathBuf> = [
        Some(p.notes.clone()),
        parent(&p.audit),
        parent(&p.ai_toml),
        parent(&p.ai_key),
        parent(&p.socket),
        parent(&p.approve),
        parent(&p.launch),
    ]
    .into_iter()
    .flatten()
    .collect();
    (ro, rw)
}

fn landlock(cfg: &Config) -> Result<&'static str, String> {
    let abi = ABI::V3;
    let (ro, rw) = dirs(cfg);
    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .and_then(|r| r.create())
        .map_err(|e| format!("landlock: {e}"))?;
    for (paths, access) in [
        (ro, AccessFs::from_read(abi)),
        (rw, AccessFs::from_all(abi)),
    ] {
        for path in paths {
            // Missing optional paths (e.g. /lib64) are simply not granted.
            let Ok(fd) = PathFd::new(&path) else {
                continue;
            };
            ruleset = ruleset
                .add_rule(PathBeneath::new(fd, access))
                .map_err(|e| format!("landlock rule {}: {e}", path.display()))?;
        }
    }
    let status = ruleset
        .restrict_self()
        .map_err(|e| format!("landlock: {e}"))?;
    Ok(match status.ruleset {
        RulesetStatus::FullyEnforced => "fully_enforced",
        RulesetStatus::PartiallyEnforced => "partially_enforced",
        RulesetStatus::NotEnforced => "not_enforced",
    })
}

fn seccomp() -> Result<(), String> {
    let rules: BTreeMap<i64, Vec<SeccompRule>> =
        DENIED_SYSCALLS.iter().map(|s| (*s, Vec::new())).collect();
    let arch: TargetArch = std::env::consts::ARCH
        .try_into()
        .map_err(|e| format!("seccomp: {e:?}"))?;
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EPERM as u32),
        arch,
    )
    .map_err(|e| format!("seccomp: {e}"))?;
    let prog: BpfProgram = filter.try_into().map_err(|e| format!("seccomp: {e}"))?;
    // All threads (there should be only one yet); sets no_new_privs.
    seccompiler::apply_filter_all_threads(&prog).map_err(|e| format!("seccomp: {e}"))
}

/// Apply both. Must run before the tokio runtime starts.
pub fn apply(cfg: &Config) -> Result<Status, String> {
    let landlock = landlock(cfg)?;
    seccomp()?;
    Ok(Status {
        landlock,
        seccomp: true,
    })
}

/// `aiosd --check-sandbox`: apply the sandbox, then try what it forbids.
/// Prints one JSON object; exits non-zero if anything was allowed.
pub fn check(cfg: &Config) -> Result<(), String> {
    let status = apply(cfg)?;
    let res = |r: std::io::Result<()>| match r {
        Ok(()) => "ALLOWED".to_string(),
        Err(e) => format!("denied: {e}"),
    };
    let outside = std::fs::read_dir("/")
        .map(|_| ())
        .and_then(|_| std::fs::read_dir("/root").map(|_| ()));
    let exec = std::process::Command::new("/usr/bin/true")
        .status()
        .map(|_| ());
    let notes_ok = std::fs::read_dir(&cfg.paths.notes).map(|_| ());
    let report = serde_json::json!({
        "landlock": status.landlock,
        "seccomp": status.seccomp,
        "read_outside_allowed_dirs": res(outside),
        "exec_usr_bin_true": res(exec),
        "read_notes_dir": res(notes_ok),
    });
    println!("{report}");
    let leaked = [
        &report["read_outside_allowed_dirs"],
        &report["exec_usr_bin_true"],
    ]
    .iter()
    .any(|v| **v == "ALLOWED");
    if leaked {
        return Err("the sandbox let a forbidden operation through".into());
    }
    Ok(())
}
