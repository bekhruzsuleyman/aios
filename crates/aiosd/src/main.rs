//! aiosd: the AiOS agent daemon. Takes requests from the shell, plans them
//! (router or AI), and runs every step through the permission broker.

mod agent;
mod ai;
mod config;
mod daemon;
mod router;
mod sandbox;
mod server;

use actions::{Context, SearchProvider};
use broker::{AuditSink, Broker};
use config::{Config, Provider};
use daemon::{Daemon, Origin};
use std::path::PathBuf;
use std::sync::Arc;

const USAGE: &str = "usage: aiosd [--config PATH] [--dev] [--check-sandbox]";

fn main() {
    if let Err(e) = real_main() {
        eprintln!("aiosd: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let mut config = PathBuf::from("/etc/aios/aios.toml");
    let mut dev = std::env::var_os("AIOS_DEV").is_some();
    let mut check_sandbox = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config = args.next().map(PathBuf::from).ok_or(USAGE)?,
            "--dev" => dev = true,
            "--check-sandbox" => check_sandbox = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let cfg = Arc::new(Config::load(&config, dev)?);
    let p = &cfg.paths;
    for dir in [Some(p.notes.as_path()), p.audit.parent(), p.socket.parent()]
        .into_iter()
        .flatten()
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    if check_sandbox {
        return sandbox::check(&cfg);
    }
    // HTTP clients load proxy settings and root certificates now, before
    // the sandbox hides the files they come from.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let ai_http = ai::client()?;
    // Before any other thread exists, so the whole daemon is inside it.
    let status = sandbox::apply(&cfg)?;
    eprintln!(
        "aiosd: sandbox: landlock {}, seccomp {}",
        status.landlock,
        if status.seccomp { "on" } else { "off" }
    );
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(run(cfg, http, ai_http))
}

async fn run(
    cfg: Arc<Config>,
    http: reqwest::Client,
    ai_http: reqwest::Client,
) -> Result<(), String> {
    let p = &cfg.paths;
    let search = match cfg.search.provider {
        Provider::BrowserFallback => SearchProvider::BrowserFallback,
        Provider::HttpJson => SearchProvider::HttpJson {
            url: cfg.search.url.clone(),
        },
    };
    let ctx = Context {
        notes_dir: p.notes.clone(),
        read_roots: p.read_roots.clone(),
        launch_socket: p.launch.clone(),
        search,
        http,
    };
    let broker = Arc::new(
        Broker::new(ctx, AuditSink::File(p.audit.clone())).with_unknown_risk(cfg.unknown_risk()),
    );
    let daemon = Daemon::new(cfg.clone(), broker, ai_http);

    let agent_l = server::bind(&p.socket).map_err(|e| format!("{}: {e}", p.socket.display()))?;
    let approve_l =
        server::bind(&p.approve).map_err(|e| format!("{}: {e}", p.approve.display()))?;
    let allowed = server::Allowed::for_socket(&p.socket, "aios").map_err(|e| e.to_string())?;
    eprintln!(
        "aiosd: listening on {} and {} (uids {:?})",
        p.socket.display(),
        p.approve.display(),
        allowed.0
    );
    tokio::spawn(server::serve(
        agent_l,
        daemon.clone(),
        Origin::Agent,
        allowed.clone(),
    ));
    tokio::spawn(server::serve(approve_l, daemon, Origin::Approve, allowed));
    tokio::signal::ctrl_c().await.map_err(|e| e.to_string())?;
    Ok(())
}
