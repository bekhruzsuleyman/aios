//! `/etc/aios/aios.toml` (system defaults) plus `ai.toml` / `ai.key` on the
//! data partition (written by first-run setup). See plan section 5.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub paths: Paths,
    #[serde(default)]
    pub search: Search,
    #[serde(default)]
    pub policy: Policy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    pub socket: PathBuf,
    pub approve: PathBuf,
    #[serde(default = "default_launch")]
    pub launch: PathBuf,
    pub audit: PathBuf,
    pub notes: PathBuf,
    pub read_roots: Vec<PathBuf>,
    #[serde(default = "default_ai_toml")]
    pub ai_toml: PathBuf,
    #[serde(default = "default_ai_key")]
    pub ai_key: PathBuf,
}

fn default_launch() -> PathBuf {
    PathBuf::from("/run/aios/launch.sock")
}
fn default_ai_toml() -> PathBuf {
    PathBuf::from("/var/lib/aios/ai.toml")
}
fn default_ai_key() -> PathBuf {
    PathBuf::from("/var/lib/aios/ai.key")
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Search {
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    BrowserFallback,
    HttpJson,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default = "default_policy")]
    pub default: String,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            default: default_policy(),
        }
    }
}

fn default_policy() -> String {
    "ask".into()
}

/// The AI endpoint as stored by first-run setup.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiSettings {
    pub endpoint: String,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    60_000
}

impl Config {
    /// Risk of actions that are not in the policy table.
    pub fn unknown_risk(&self) -> protocol::Risk {
        if self.policy.default == "never" {
            protocol::Risk::Never
        } else {
            protocol::Risk::Ask
        }
    }

    pub fn load(path: &Path, dev: bool) -> Result<Config, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        Config::parse(&text, dev)
    }

    pub fn parse(text: &str, dev: bool) -> Result<Config, String> {
        let mut cfg: Config = toml::from_str(text).map_err(|e| format!("config: {e}"))?;
        if !matches!(cfg.policy.default.as_str(), "ask" | "never") {
            return Err(format!(
                "config: [policy] default must be \"ask\" or \"never\", not {:?}",
                cfg.policy.default
            ));
        }
        if dev {
            cfg.dev_paths();
        }
        Ok(cfg)
    }

    /// Dev mode (unit tests and host runs only): `/var/lib/aios` becomes
    /// `./build/dev-data` and `/run/aios` becomes `$XDG_RUNTIME_DIR/aios`.
    fn dev_paths(&mut self) {
        let data = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("build/dev-data");
        let run = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("aios");
        let map = |p: &PathBuf| -> PathBuf {
            if let Ok(rest) = p.strip_prefix("/var/lib/aios") {
                data.join(rest)
            } else if let Ok(rest) = p.strip_prefix("/run/aios") {
                run.join(rest)
            } else {
                p.clone()
            }
        };
        let p = &mut self.paths;
        p.socket = map(&p.socket);
        p.approve = map(&p.approve);
        p.launch = map(&p.launch);
        p.audit = map(&p.audit);
        p.notes = map(&p.notes);
        p.ai_toml = map(&p.ai_toml);
        p.ai_key = map(&p.ai_key);
        p.read_roots = p.read_roots.iter().map(map).collect();
    }

    /// Store the AI settings: `ai.toml` (0640) and the key in `ai.key`
    /// (0600, only aiosd can read it). Written atomically.
    pub fn save_ai(&self, s: &AiSettings, key: &str) -> Result<(), String> {
        let text = toml::to_string(s).map_err(|e| e.to_string())?;
        write_atomic(&self.paths.ai_key, key.as_bytes(), 0o600)?;
        write_atomic(&self.paths.ai_toml, text.as_bytes(), 0o640)
    }

    /// Current AI settings, or None when not configured. Read on every
    /// use so first-run setup takes effect without a restart.
    pub fn ai(&self) -> Option<(AiSettings, String)> {
        let text = std::fs::read_to_string(&self.paths.ai_toml).ok()?;
        let s: AiSettings = toml::from_str(&text).ok()?;
        if s.endpoint.trim().is_empty() || s.model.trim().is_empty() {
            return None;
        }
        let key = std::fs::read_to_string(&self.paths.ai_key)
            .map(|k| k.trim().to_string())
            .unwrap_or_default();
        Some((s, key))
    }
}

fn write_atomic(path: &Path, data: &[u8], mode: u32) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)
        .map_err(err)?;
    f.write_all(data).map_err(err)?;
    f.sync_all().map_err(err)?;
    std::fs::rename(&tmp, path).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_config_parses() {
        let cfg = Config::parse(include_str!("../../../config/aios.toml"), false).unwrap();
        assert_eq!(cfg.paths.socket, PathBuf::from("/run/aios/agent.sock"));
        assert_eq!(cfg.paths.approve, PathBuf::from("/run/aios/approve.sock"));
        assert_eq!(cfg.search.provider, Provider::BrowserFallback);
        assert_eq!(cfg.policy.default, "ask");
    }

    #[test]
    fn dev_paths_are_rewritten() {
        let cfg = Config::parse(include_str!("../../../config/aios.toml"), true).unwrap();
        assert!(cfg.paths.notes.ends_with("build/dev-data/notes"));
        assert!(cfg.paths.socket.ends_with("aios/agent.sock"));
        assert!(!cfg.paths.socket.starts_with("/run/aios"));
    }

    #[test]
    fn ai_settings_round_trip_with_private_key() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("aiosd-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut cfg = Config::parse(include_str!("../../../config/aios.toml"), false).unwrap();
        cfg.paths.ai_toml = dir.join("ai.toml");
        cfg.paths.ai_key = dir.join("ai.key");
        assert!(cfg.ai().is_none());
        let s = AiSettings {
            endpoint: "http://10.0.2.2:8081/v1".into(),
            model: "m".into(),
            timeout_ms: 1000,
        };
        cfg.save_ai(&s, "sk-test").unwrap();
        assert_eq!(cfg.ai(), Some((s, "sk-test".to_string())));
        let mode = std::fs::metadata(&cfg.paths.ai_key)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
