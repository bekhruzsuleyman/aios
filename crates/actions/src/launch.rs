//! `app.launch`: a fixed allow-list of apps, each started inside a
//! bubblewrap sandbox that hides `/run/aios` and the data partition.
//!
//! aiosd runs as its own system user and cannot start programs in the
//! desktop session itself, so after the broker allows a launch it sends a
//! typed `LaunchRequest` to the session's launcher (inside aios-shell,
//! socket `/run/aios/launch.sock`). The launcher builds the command line
//! with `sandbox_argv` from the app id alone; no text from the request is
//! ever used as a program or option, and the only free argument (a URL) is
//! validated to be http(s).

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum App {
    Browser,
    Editor,
    Terminal,
}

impl App {
    pub fn from_name(name: &str) -> Option<App> {
        match name.trim().to_ascii_lowercase().as_str() {
            "browser" | "web browser" | "web" | "firefox" => Some(App::Browser),
            "editor" | "text editor" | "mousepad" => Some(App::Editor),
            "terminal" | "foot" | "console" => Some(App::Terminal),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            App::Browser => "the browser",
            App::Editor => "the text editor",
            App::Terminal => "the terminal",
        }
    }

    /// The program for this app. Fixed; see DECISIONS.md (browser choice).
    pub fn program(self) -> &'static str {
        match self {
            App::Browser => "/usr/bin/firefox",
            App::Editor => "/usr/bin/mousepad",
            App::Terminal => "/usr/bin/foot",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            App::Browser => "browser",
            App::Editor => "editor",
            App::Terminal => "terminal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRequest {
    pub app: App,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchReply {
    pub ok: bool,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Accept only absolute http(s) URLs, and only for the browser.
pub fn validate(req: &LaunchRequest) -> Result<(), String> {
    let Some(u) = &req.url else {
        return Ok(());
    };
    if req.app != App::Browser {
        return Err("only the browser takes a URL".into());
    }
    let parsed = url::Url::parse(u).map_err(|e| format!("bad url: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("only http(s) URLs can be opened".into());
    }
    Ok(())
}

/// Where the session's sockets and per-app scratch directories are.
pub struct SessionPaths<'a> {
    /// The session's XDG_RUNTIME_DIR (holds the Wayland socket).
    pub runtime_dir: &'a Path,
    pub wayland_display: &'a str,
    /// Per-app writable home inside the sandbox.
    pub scratch: &'a Path,
}

/// The full bubblewrap command line for a validated request.
pub fn sandbox_argv(req: &LaunchRequest, s: &SessionPaths) -> Vec<String> {
    let rt = s.runtime_dir.to_string_lossy().into_owned();
    let wl = format!("{rt}/{}", s.wayland_display);
    let home = s.scratch.to_string_lossy().into_owned();
    let mut v: Vec<String> = [
        "/usr/bin/bwrap",
        "--die-with-parent",
        "--new-session",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--ro-bind",
        "/usr",
        "/usr",
        "--symlink",
        "usr/bin",
        "/bin",
        "--symlink",
        "usr/sbin",
        "/sbin",
        "--symlink",
        "usr/lib",
        "/lib",
        "--symlink",
        "usr/lib64",
        "/lib64",
        "--ro-bind",
        "/etc",
        "/etc",
        "--proc",
        "/proc",
        // Read-only /sys: WebKit's own inner sandbox binds parts of it.
        "--ro-bind",
        "/sys",
        "/sys",
        "--dev",
        "/dev",
        // Firefox needs a writable /dev/shm for its content processes.
        "--tmpfs",
        "/dev/shm",
        "--dev-bind-try",
        "/dev/dri",
        "/dev/dri",
        "--tmpfs",
        "/tmp",
        // Empty /run and /var: no agent sockets, no data partition.
        "--tmpfs",
        "/run",
        "--tmpfs",
        "/var",
        "--ro-bind-try",
        "/run/systemd/resolve",
        "/run/systemd/resolve",
        "--dir",
        &rt,
        "--chmod",
        "0700",
        &rt,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.extend(["--bind".into(), wl.clone(), wl]);
    v.extend(["--bind".into(), home.clone(), home.clone()]);
    for (k, val) in [
        ("HOME", home.as_str()),
        ("XDG_RUNTIME_DIR", rt.as_str()),
        ("WAYLAND_DISPLAY", s.wayland_display),
        ("XDG_SESSION_TYPE", "wayland"),
        ("MOZ_ENABLE_WAYLAND", "1"),
        ("GDK_BACKEND", "wayland"),
        ("GTK_A11Y", "none"),
    ] {
        v.extend(["--setenv".into(), k.into(), val.into()]);
    }
    v.extend(["--unsetenv".into(), "DBUS_SESSION_BUS_ADDRESS".into()]);
    // Apps get a private session bus that lives and dies with the sandbox;
    // the user's bus (if any) is never reachable from inside.
    v.extend(["--".into(), "/usr/bin/dbus-run-session".into(), "--".into()]);
    v.push(req.app.program().into());
    if let Some(u) = &req.url {
        // validate() guarantees an http(s) URL, so it cannot start with '-'.
        v.push(u.clone());
    }
    v
}

/// Ask the session launcher to start an app. Returns the sandbox's pid.
pub async fn request(socket: &Path, app: App, url: Option<&str>) -> Result<u32, String> {
    let req = LaunchRequest {
        app,
        url: url.map(str::to_string),
    };
    validate(&req)?;
    let fut = async {
        let mut stream = tokio::net::UnixStream::connect(socket)
            .await
            .map_err(|e| format!("desktop launcher unavailable: {e}"))?;
        let mut line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        line.push('\n');
        stream
            .write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let mut reply = String::new();
        BufReader::new(stream)
            .read_line(&mut reply)
            .await
            .map_err(|e| e.to_string())?;
        let reply: LaunchReply =
            serde_json::from_str(&reply).map_err(|e| format!("launcher reply: {e}"))?;
        match (reply.ok, reply.pid) {
            (true, Some(pid)) => Ok(pid),
            _ => Err(reply.error.unwrap_or_else(|| "launch failed".into())),
        }
    };
    tokio::time::timeout(Duration::from_secs(10), fut)
        .await
        .map_err(|_| "desktop launcher did not answer".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn paths() -> (PathBuf, PathBuf) {
        (
            PathBuf::from("/run/user/1000"),
            PathBuf::from("/home/aios/.apps/browser"),
        )
    }

    #[test]
    fn urls_are_validated() {
        let ok = LaunchRequest {
            app: App::Browser,
            url: Some("https://duckduckgo.com/?q=a%20b".into()),
        };
        assert!(validate(&ok).is_ok());
        for bad in [
            "--remote-debugging-port=1",
            "file:///etc/shadow",
            "javascript:alert(1)",
            "https://",
            "data:text/html,x",
        ] {
            let r = LaunchRequest {
                app: App::Browser,
                url: Some(bad.into()),
            };
            assert!(validate(&r).is_err(), "{bad}");
        }
        let r = LaunchRequest {
            app: App::Terminal,
            url: Some("https://x.org".into()),
        };
        assert!(validate(&r).is_err());
    }

    #[test]
    fn sandbox_hides_agent_sockets_and_data() {
        let (rt, home) = paths();
        let s = SessionPaths {
            runtime_dir: &rt,
            wayland_display: "wayland-0",
            scratch: &home,
        };
        let req = LaunchRequest {
            app: App::Browser,
            url: Some("https://example.org/".into()),
        };
        let argv = sandbox_argv(&req, &s);
        assert_eq!(argv[0], "/usr/bin/bwrap");
        let joined = argv.join(" ");
        assert!(joined.contains("--tmpfs /run"));
        assert!(joined.contains("--tmpfs /var"));
        assert!(joined.contains("--unshare-pid"));
        assert!(!joined.contains("/run/aios"));
        assert!(!joined.contains("/var/lib/aios"));
        let sep = argv.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &argv[sep + 1..],
            [
                "/usr/bin/dbus-run-session",
                "--",
                "/usr/bin/firefox",
                "https://example.org/"
            ]
        );
    }

    #[test]
    fn app_names() {
        assert_eq!(App::from_name(" Terminal "), Some(App::Terminal));
        assert_eq!(App::from_name("sh"), None);
        let r: Result<LaunchRequest, _> =
            serde_json::from_str(r#"{"app":"browser","argv":["sh"]}"#);
        assert!(r.is_err());
    }
}
