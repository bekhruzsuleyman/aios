//! TEST-ONLY: run *as a launched app* (tests/injection bind-mounts it over
//! /usr/bin/foot in the test VM and starts it with `open terminal`). It
//! tries to escape the app sandbox and records what happened in
//! `$HOME/probe.json` (the app's scratch home, readable from outside).

// Raw ptrace/kill are the point of this probe; each block has a SAFETY note.
#![allow(unsafe_code)]

use serde_json::{json, Value};
use std::os::unix::net::UnixStream;

fn try_connect(path: &str) -> Value {
    match UnixStream::connect(path) {
        Ok(_) => json!("CONNECTED"),
        Err(e) => json!(format!("failed: {e}")),
    }
}

fn try_read(path: &str) -> Value {
    match std::fs::read(path) {
        Ok(b) => json!(format!("READ {} bytes", b.len())),
        Err(e) => json!(format!("failed: {e}")),
    }
}

fn try_list(path: &str) -> Value {
    match std::fs::read_dir(path) {
        Ok(d) => json!(d
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>()),
        Err(e) => json!(format!("failed: {e}")),
    }
}

fn try_ptrace(pid: i32) -> Value {
    // SAFETY: PTRACE_ATTACH with null addr/data is a plain syscall; on
    // success we detach immediately. No memory is shared or written.
    let r = unsafe {
        libc::ptrace(
            libc::PTRACE_ATTACH,
            pid,
            std::ptr::null_mut::<libc::c_void>(),
            std::ptr::null_mut::<libc::c_void>(),
        )
    };
    if r == 0 {
        // SAFETY: as above.
        unsafe {
            libc::ptrace(
                libc::PTRACE_DETACH,
                pid,
                std::ptr::null_mut::<libc::c_void>(),
                std::ptr::null_mut::<libc::c_void>(),
            );
        }
        json!("ATTACHED")
    } else {
        json!(format!("failed: {}", std::io::Error::last_os_error()))
    }
}

fn processes() -> Vec<(i32, String)> {
    std::fs::read_dir("/proc")
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let pid: i32 = e.file_name().to_str()?.parse().ok()?;
                    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
                    Some((pid, comm.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn main() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    // The shell's pid outside the sandbox, written by the test beforehand.
    let shell_pid: Option<i32> = std::fs::read_to_string(format!("{home}/shell.pid"))
        .ok()
        .and_then(|s| s.trim().parse().ok());
    let procs = processes();
    let me = std::process::id() as i32;
    let report = json!({
        "uid": std::fs::read_to_string("/proc/self/status").ok()
            .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).map(|l| l.to_string())),
        "connect_agent_sock": try_connect("/run/aios/agent.sock"),
        "connect_approve_sock": try_connect("/run/aios/approve.sock"),
        "connect_launch_sock": try_connect("/run/aios/launch.sock"),
        "list_run_aios": try_list("/run/aios"),
        "list_var_lib_aios": try_list("/var/lib/aios"),
        "read_audit_log": try_read("/var/lib/aios/audit.log"),
        "read_ai_key": try_read("/var/lib/aios/ai.key"),
        "read_notes": try_list("/var/lib/aios/notes"),
        "list_home_aios": try_list("/home/aios"),
        "visible_processes": procs.iter().map(|(p, c)| format!("{p} {c}")).collect::<Vec<_>>(),
        "shell_visible": procs.iter().any(|(_, c)| c == "aios-shell"),
        "ptrace_shell_host_pid": shell_pid.map(try_ptrace),
        "kill0_shell_host_pid": shell_pid.map(|p| {
            // SAFETY: signal 0 only checks whether the pid can be signalled.
            if unsafe { libc::kill(p, 0) } == 0 { json!("SIGNALLABLE") }
            else { json!(format!("failed: {}", std::io::Error::last_os_error())) }
        }),
        "ptrace_other_visible": procs.iter().filter(|(p, _)| *p != me)
            .map(|(p, c)| json!({"pid": p, "comm": c, "ptrace": try_ptrace(*p)}))
            .collect::<Vec<_>>(),
    });
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    println!("{text}");
    let _ = std::fs::write(format!("{home}/probe.json"), text);
}
