//! `config/aios.toml`, see plan section 4.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    #[default]
    Fake,
    Real,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub agent: AgentKind,
    pub ai: AiConfig,
    pub paths: Paths,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub shell: ShellConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key_env: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    pub socket: PathBuf,
    pub audit: PathBuf,
    pub notes: PathBuf,
    pub read_roots: Vec<PathBuf>,
    pub shell: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    #[serde(default = "default_policy")]
    pub default: String,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        PolicyConfig {
            default: default_policy(),
        }
    }
}

fn default_policy() -> String {
    "ask".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
}

impl Default for ShellConfig {
    fn default() -> Self {
        ShellConfig {
            listen: default_listen(),
        }
    }
}

fn default_listen() -> String {
    "127.0.0.1:7700".to_string()
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        Config::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Config, String> {
        let mut cfg: Config = toml::from_str(text).map_err(|e| format!("config: {e}"))?;
        cfg.paths.socket = expand(&cfg.paths.socket);
        cfg.paths.audit = expand(&cfg.paths.audit);
        cfg.paths.notes = expand(&cfg.paths.notes);
        cfg.paths.shell = expand(&cfg.paths.shell);
        cfg.paths.read_roots = cfg.paths.read_roots.iter().map(|p| expand(p)).collect();
        Ok(cfg)
    }
}

/// Expand a leading `$XDG_RUNTIME_DIR` or `$HOME` so dev configs can use them.
fn expand(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    for var in ["XDG_RUNTIME_DIR", "HOME"] {
        let prefix = format!("${var}");
        if let Some(rest) = s.strip_prefix(&prefix) {
            if let Ok(val) = std::env::var(var) {
                return PathBuf::from(format!("{val}{rest}"));
            }
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_config_parses() {
        let text = include_str!("../../../config/aios.toml");
        let cfg = Config::parse(text).unwrap();
        assert_eq!(cfg.agent, AgentKind::Fake);
        assert_eq!(cfg.ai.api_key_env, "AIOS_API_KEY");
        assert_eq!(cfg.policy.default, "ask");
        assert_eq!(cfg.shell.listen, "127.0.0.1:7700");
    }

    #[test]
    fn expands_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            expand(Path::new("$HOME/x")),
            PathBuf::from(format!("{home}/x"))
        );
        assert_eq!(expand(Path::new("/abs")), PathBuf::from("/abs"));
    }
}
