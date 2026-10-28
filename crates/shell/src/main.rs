//! aios-shell: the AiOS desktop shell, a native Wayland layer-shell client.
//!
//!   aios-shell                    run the shell (started by labwc autostart)
//!   aios-shell --toggle-palette   ask the running shell to open/close the palette
//!   aios-shell --show-palette     open the palette (and give it keyboard focus)

mod app;
mod gfx;
mod launcher;
mod text;
mod ui;
mod wifi;

use smithay_client_toolkit::reexports::calloop::channel;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Private socket for `--toggle-palette`, in the user's runtime dir.
fn control_path() -> PathBuf {
    runtime_dir().join("aios-shell.sock")
}

fn sock(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn control(cmd: &[u8]) -> Result<(), String> {
    let mut s =
        UnixStream::connect(control_path()).map_err(|e| format!("shell not running: {e}"))?;
    s.write_all(cmd).map_err(|e| e.to_string())
}

fn control_server(tx: channel::Sender<app::Msg>) -> Result<(), String> {
    let path = control_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 16];
            let n = stream.read(&mut buf).unwrap_or(0);
            let msg = match &buf[..n] {
                b if b.starts_with(b"toggle") => app::Msg::TogglePalette,
                b if b.starts_with(b"show") => app::Msg::ShowPalette,
                _ => continue,
            };
            if tx.send(msg).is_err() {
                break;
            }
        }
    });
    Ok(())
}

fn main() -> Result<(), String> {
    if std::env::args().any(|a| a == "--toggle-palette") {
        return control(b"toggle\n");
    }
    if std::env::args().any(|a| a == "--show-palette") {
        return control(b"show\n");
    }
    // Before any thread exists, so every thread (and every app) inherits it.
    launcher::drop_capabilities();
    let (tx, rx) = channel::channel();
    control_server(tx.clone())?;
    let launcher = launcher::Launcher::from_env(
        sock("AIOS_LAUNCH_SOCK", "/run/aios/launch.sock"),
        launcher::default_home(),
        runtime_dir(),
    );
    // Without the launcher, app.launch fails with a clear error in aiosd;
    // the rest of the shell still works.
    if let Err(e) = launcher.spawn() {
        eprintln!("aios-shell: launcher unavailable: {e}");
    }
    let paths = app::Paths {
        agent: sock("AIOS_AGENT_SOCK", "/run/aios/agent.sock"),
        approve: sock("AIOS_APPROVE_SOCK", "/run/aios/approve.sock"),
    };
    app::run(paths, launcher, rx, tx)
}
