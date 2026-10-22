//! The session's app launcher. aiosd runs as its own user and cannot start
//! programs in the desktop session, so after the broker allows a launch it
//! sends a typed `LaunchRequest` here. Only aiosd (and root) may connect.
//! The command line comes from `actions::launch::sandbox_argv`, built from
//! the app id alone; every app runs inside bubblewrap.

use actions::launch::{sandbox_argv, validate, LaunchReply, LaunchRequest, SessionPaths};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Launcher {
    pub socket: PathBuf,
    pub runtime_dir: PathBuf,
    pub wayland_display: String,
    pub home: PathBuf,
    pub allowed_uids: Vec<u32>,
}

impl Launcher {
    pub fn from_env(socket: PathBuf, home: PathBuf, runtime_dir: PathBuf) -> Launcher {
        let mut allowed_uids = vec![0, rustix::process::getuid().as_raw()];
        if let Some(uid) = uid_of("aiosd") {
            allowed_uids.push(uid);
        }
        Launcher {
            socket,
            runtime_dir,
            wayland_display: std::env::var("WAYLAND_DISPLAY")
                .unwrap_or_else(|_| "wayland-0".into()),
            home,
            allowed_uids,
        }
    }

    /// Bind the socket (group-writable, so aiosd's group can connect) and
    /// serve it on a thread.
    pub fn spawn(self) -> Result<(), String> {
        let _ = std::fs::remove_file(&self.socket);
        let listener = UnixListener::bind(&self.socket)
            .map_err(|e| format!("{}: {e}", self.socket.display()))?;
        std::fs::set_permissions(&self.socket, std::fs::Permissions::from_mode(0o660))
            .map_err(|e| e.to_string())?;
        eprintln!("aios-shell: launcher on {}", self.socket.display());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if let Err(e) = self.serve(stream) {
                    eprintln!("aios-shell: launch failed: {e}");
                }
            }
        });
        Ok(())
    }

    fn serve(&self, stream: UnixStream) -> Result<(), String> {
        let uid = rustix::net::sockopt::socket_peercred(&stream)
            .map_err(|e| e.to_string())?
            .uid
            .as_raw();
        if !self.allowed_uids.contains(&uid) {
            return Err(format!("refused launch request from uid {uid}"));
        }
        let mut w = stream.try_clone().map_err(|e| e.to_string())?;
        let mut line = String::new();
        BufReader::new(stream)
            .take(64 * 1024)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        let reply = match serde_json::from_str::<LaunchRequest>(&line)
            .map_err(|e| format!("bad launch request: {e}"))
            .and_then(|req| self.launch(&req))
        {
            Ok(pid) => LaunchReply {
                ok: true,
                pid: Some(pid),
                error: None,
            },
            Err(e) => LaunchReply {
                ok: false,
                pid: None,
                error: Some(e),
            },
        };
        let mut out = serde_json::to_string(&reply).map_err(|e| e.to_string())?;
        out.push('\n');
        w.write_all(out.as_bytes()).map_err(|e| e.to_string())
    }

    fn launch(&self, req: &LaunchRequest) -> Result<u32, String> {
        validate(req)?;
        let scratch = self.home.join(".apps").join(req.app.id());
        std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
        let argv = sandbox_argv(
            req,
            &SessionPaths {
                runtime_dir: &self.runtime_dir,
                wayland_display: &self.wayland_display,
                scratch: &scratch,
            },
        );
        let (prog, args) = argv.split_first().ok_or("empty command line")?;
        let mut child = Command::new(prog)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin")
            .current_dir(&scratch)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", req.app.program()))?;
        let pid = child.id();
        eprintln!(
            "aios-shell: launched {} pid {pid} url {:?}",
            req.app.id(),
            req.url
        );
        // Reap it when it exits.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(pid)
    }
}

/// Drop every capability the session handed us. systemd gives user sessions
/// `CAP_WAKE_ALARM` as an ambient capability, and unprivileged bubblewrap
/// refuses to start while any capability is set, so apps would die at once.
pub fn drop_capabilities() {
    use rustix::thread::{
        clear_ambient_capability_set, set_capabilities, CapabilitySet, CapabilitySets,
    };
    if let Err(e) = clear_ambient_capability_set() {
        eprintln!("aios-shell: cannot clear ambient capabilities: {e}");
    }
    let none = CapabilitySets {
        effective: CapabilitySet::empty(),
        permitted: CapabilitySet::empty(),
        inheritable: CapabilitySet::empty(),
    };
    if let Err(e) = set_capabilities(None, none) {
        eprintln!("aios-shell: cannot drop capabilities: {e}");
    }
}

fn uid_of(user: &str) -> Option<u32> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|l| {
        let mut f = l.split(':');
        (f.next()? == user).then_some(())?;
        f.nth(1)?.parse().ok()
    })
}

pub fn default_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/home/aios").to_path_buf())
}
