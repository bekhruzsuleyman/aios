use aiosd::agent::Agent;
use aiosd::config::{AgentKind, Config};
use aiosd::daemon::Daemon;
use aiosd::fake::FakeAgent;
use std::path::PathBuf;
use std::time::Duration;

fn config_path() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--config" {
            if let Some(p) = args.next() {
                return PathBuf::from(p);
            }
        }
    }
    PathBuf::from("/etc/aios/aios.toml")
}

fn main() -> Result<(), String> {
    let cfg = Config::load(&config_path())?;
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(run(cfg))
}

async fn run(cfg: Config) -> Result<(), String> {
    let agent = match cfg.agent {
        AgentKind::Fake => Agent::Fake(FakeAgent::new(Duration::from_millis(100))),
        AgentKind::Real => return Err("agent = \"real\" is not implemented yet".into()),
    };
    let daemon = Daemon::new(agent);
    let listener = aiosd::server::bind(&cfg.paths.socket)
        .map_err(|e| format!("binding {}: {e}", cfg.paths.socket.display()))?;
    eprintln!("aiosd: listening on {}", cfg.paths.socket.display());
    tokio::select! {
        _ = aiosd::server::serve(listener, daemon) => {}
        _ = tokio::signal::ctrl_c() => eprintln!("aiosd: shutting down"),
    }
    Ok(())
}
