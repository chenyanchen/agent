use serde::{Deserialize, Serialize};

// ── Top-level Config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub guard: GuardConfig,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            guard: GuardConfig::default(),
            system_prompt: None,
        }
    }
}

impl Config {
    /// Load config from `~/.agent/config.toml`.
    /// If the file is missing or unreadable, returns the default config silently.
    pub fn load() -> Self {
        let Some(home) = dirs::home_dir() else {
            return Self::default();
        };
        let path = home.join(".agent").join("config.toml");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }
}

// ── ModelConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "default_model_id")]
    pub model_id: String,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
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

[guard]
mode = "auto"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.model.model_id, "gpt-4-turbo");
        assert_eq!(cfg.model.api_key.as_deref(), Some("sk-test"));
        assert_eq!(cfg.model.api_base.as_deref(), Some("https://my.proxy/v1"));
        assert_eq!(cfg.guard.mode, GuardMode::Auto);
        assert_eq!(cfg.system_prompt.as_deref(), Some("You are a helpful assistant."));
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
}
