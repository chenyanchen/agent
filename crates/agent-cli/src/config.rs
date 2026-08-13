use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use agent_core::{ConfirmGuard, Decision, Guard, RiskLevel};

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "failed to parse {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

// ── Top-level Config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub guard: GuardConfig,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl Config {
    /// Load config from `~/.agent/config.toml`.
    /// A missing file uses defaults; unreadable or invalid files return an error.
    pub fn load() -> Result<Self, ConfigError> {
        let Some(home) = dirs::home_dir() else {
            return Ok(Self::default());
        };
        Self::load_from(home.join(".agent").join("config.toml"))
    }

    fn load_from(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        toml::from_str(&content).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
    }
}

// ── ModelConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "default_model_id")]
    pub model_id: String,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub context_window: Option<u32>,
}

fn default_model_id() -> String {
    "gpt-4o".into()
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_id: default_model_id(),
            api_key: None,
            api_base: None,
            context_window: None,
        }
    }
}

// ── GuardConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardConfig {
    #[serde(default = "default_guard_mode")]
    pub mode: GuardMode,
}

fn default_guard_mode() -> GuardMode {
    GuardMode::Confirm
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            mode: default_guard_mode(),
        }
    }
}

// ── GuardMode ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardMode {
    Auto,
    Confirm,
}

#[async_trait::async_trait]
impl Guard for GuardMode {
    async fn check(
        &self,
        tool_name: &str,
        risk_level: RiskLevel,
        input: &serde_json::Value,
    ) -> Decision {
        match self {
            Self::Auto => Decision::Allow,
            Self::Confirm => ConfirmGuard.check(tool_name, risk_level, input).await,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = Config::default();
        assert_eq!(cfg.model.model_id, "gpt-4o");
        assert!(cfg.model.api_key.is_none());
        assert!(cfg.model.api_base.is_none());
        assert!(cfg.model.context_window.is_none());
        assert_eq!(cfg.guard.mode, GuardMode::Confirm);
        assert!(cfg.system_prompt.is_none());
    }

    #[test]
    fn parse_full_toml() {
        let toml_str = r#"
system_prompt = "You are a helpful assistant."

[model]
model_id = "gpt-4-turbo"
api_key = "sk-test"
api_base = "https://my.proxy/v1"
context_window = 128000

[guard]
mode = "auto"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.model.model_id, "gpt-4-turbo");
        assert_eq!(cfg.model.api_key.as_deref(), Some("sk-test"));
        assert_eq!(cfg.model.api_base.as_deref(), Some("https://my.proxy/v1"));
        assert_eq!(cfg.model.context_window, Some(128000));
        assert_eq!(cfg.guard.mode, GuardMode::Auto);
        assert_eq!(
            cfg.system_prompt.as_deref(),
            Some("You are a helpful assistant.")
        );
    }

    #[test]
    fn parse_minimal_toml_uses_defaults() {
        let toml_str = r#"
[model]
model_id = "gpt-3.5-turbo"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.model.model_id, "gpt-3.5-turbo");
        assert!(cfg.model.api_key.is_none());
        assert_eq!(cfg.guard.mode, GuardMode::Confirm);
    }

    #[test]
    fn parse_empty_toml_is_all_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.model.model_id, "gpt-4o");
        assert_eq!(cfg.guard.mode, GuardMode::Confirm);
    }

    #[test]
    fn missing_config_uses_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let cfg = Config::load_from(directory.path().join("missing.toml")).unwrap();
        assert_eq!(cfg.model.model_id, "gpt-4o");
    }

    #[test]
    fn invalid_config_reports_path_and_location() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "[guard]\nmode = 123\n").unwrap();

        let error = Config::load_from(&path).unwrap_err().to_string();

        assert!(error.contains(&path.display().to_string()));
        assert!(error.contains("line 2"));
        assert!(error.contains("mode"));
    }
}
