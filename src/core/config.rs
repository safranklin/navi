//! # Configuration
//!
//! Centralizes all settings with a clear override hierarchy:
//! defaults → config file → env vars → CLI flags.
//!
//! Config lives at `~/.navi/config.toml`. If missing on first run, a
//! commented-out default is generated so users can discover all options.

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;

use crate::inference::Effort;

// ============================================================================
// Config Structs (all fields Option<T> for sparse TOML)
// ============================================================================

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct NaviConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub openrouter: OpenRouterConfig,
    #[serde(default)]
    pub lmstudio: LmStudioConfig,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GeneralConfig {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub max_agentic_rounds: Option<u8>,
    pub max_output_tokens: Option<u32>,
    pub reasoning_effort: Option<Effort>,
    pub system_prompt: Option<String>,
    pub system_prompt_file: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct OpenRouterConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct LmStudioConfig {
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelEntry {
    pub name: String,
    pub provider: String,
    pub description: Option<String>,
}

// ============================================================================
// Defaults
// ============================================================================

pub const DEFAULT_MAX_AGENTIC_ROUNDS: u8 = 20;
pub const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 16384;
pub const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const DEFAULT_LMSTUDIO_BASE_URL: &str = "http://localhost:1234/v1";

const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful assistant. \
    TOOL USE IS MANDATORY: if a registered tool can perform a computation, lookup, or action, you MUST call it. \
    NEVER perform arithmetic, math, or calculations yourself — always delegate to the appropriate tool. \
    When independent sub-expressions can be computed simultaneously, call multiple tools in parallel. \
    When a result depends on a previous tool's output, wait for that result before proceeding. \
    Your text responses should only interpret and present tool results, never substitute for them. \
    Be direct, be honest about uncertainty, and prefer clarity over hedging.";

// ============================================================================
// Resolved Config (concrete values, no Options)
// ============================================================================

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub provider: String,
    pub model_name: String,
    pub max_agentic_rounds: u8,
    pub max_output_tokens: u32,
    pub effort: Effort,
    pub system_prompt: String,
    pub openrouter_api_key: Option<String>,
    pub openrouter_base_url: String,
    pub lmstudio_base_url: String,
    pub models: Vec<ModelEntry>,
}

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config I/O error: {e}"),
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

// ============================================================================
// Loading
// ============================================================================

/// Returns the path to `~/.navi/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".navi").join("config.toml"))
}

/// Load config from `~/.navi/config.toml`.
///
/// If the file doesn't exist, generates a commented-out default and
/// returns `NaviConfig::default()`. If it exists but is malformed,
/// returns `ConfigError::Parse`.
pub fn load_config() -> Result<NaviConfig, ConfigError> {
    let path = match config_path() {
        Some(p) => p,
        None => {
            warn!("Could not determine home directory, using default config");
            return Ok(NaviConfig::default());
        }
    };

    if !path.exists() {
        info!("No config file found, generating default at {}", path.display());
        generate_default_config(&path);
        return Ok(NaviConfig::default());
    }

    let contents = fs::read_to_string(&path).map_err(ConfigError::Io)?;
    let config: NaviConfig = toml::from_str(&contents).map_err(ConfigError::Parse)?;
    info!("Loaded config from {}", path.display());
    debug!("Config: {:?}", config);
    Ok(config)
}

/// Generates a commented-out default config file at the given path.
fn generate_default_config(path: &PathBuf) {
    let default_content = r#"# Navi Configuration
# All settings are optional — defaults are used for anything not specified.
# Override hierarchy: defaults → this file → env vars → CLI flags.

# [general]
# default_provider = "openrouter"    # "openrouter" or "lmstudio"
# default_model = "anthropic/claude-sonnet-4"
# max_agentic_rounds = 20
# max_output_tokens = 16384
# reasoning_effort = "auto"          # "high", "medium", "low", "auto", "none"
# system_prompt = "You are a helpful assistant."
# system_prompt_file = "system.md"   # Path relative to ~/.navi/

# [openrouter]
# api_key = "sk-or-..."              # Or set OPENROUTER_API_KEY env var
# base_url = "https://openrouter.ai/api/v1"

# [lmstudio]
# base_url = "http://localhost:1234/v1"

# [[models]]
# name = "anthropic/claude-sonnet-4"
# provider = "openrouter"
# description = "Fast, balanced reasoning"

# [[models]]
# name = "qwen2.5-coder-32b"
# provider = "lmstudio"
# description = "Local coding model"
"#;

    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!("Failed to create config directory: {}", e);
            return;
        }
    }
    if let Err(e) = fs::write(path, default_content) {
        warn!("Failed to write default config: {}", e);
    }
}

// ============================================================================
// Resolution
// ============================================================================

/// Resolve the final config by collapsing: defaults → config file → env vars → CLI.
///
/// `cli_provider` and `cli_model` are from CLI flags (None = not specified).
pub fn resolve(config: &NaviConfig, cli_provider: Option<&str>) -> ResolvedConfig {
    // Provider: CLI → env → config → default
    let provider = cli_provider
        .map(|s| s.to_string())
        .or_else(|| std::env::var("NAVI_PROVIDER").ok())
        .or_else(|| config.general.default_provider.clone())
        .unwrap_or_else(|| "openrouter".to_string());

    // Model: env → config → default
    let model_name = std::env::var("PRIMARY_MODEL_NAME")
        .ok()
        .or_else(|| config.general.default_model.clone())
        .unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string());

    // System prompt: inline config wins over file, both win over default
    let system_prompt = resolve_system_prompt(config);

    // OpenRouter API key: env → config
    let openrouter_api_key = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .or_else(|| config.openrouter.api_key.clone());

    // OpenRouter base URL: env → config → default
    let openrouter_base_url = std::env::var("OPENROUTER_BASE_URL")
        .ok()
        .or_else(|| config.openrouter.base_url.clone())
        .unwrap_or_else(|| DEFAULT_OPENROUTER_BASE_URL.to_string());

    // LM Studio base URL: env → config → default
    let lmstudio_base_url = std::env::var("LM_STUDIO_BASE_URL")
        .ok()
        .or_else(|| config.lmstudio.base_url.clone())
        .unwrap_or_else(|| DEFAULT_LMSTUDIO_BASE_URL.to_string());

    ResolvedConfig {
        provider,
        model_name,
        max_agentic_rounds: config
            .general
            .max_agentic_rounds
            .unwrap_or(DEFAULT_MAX_AGENTIC_ROUNDS),
        max_output_tokens: config
            .general
            .max_output_tokens
            .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
        effort: config.general.reasoning_effort.unwrap_or_default(),
        system_prompt,
        openrouter_api_key,
        openrouter_base_url,
        lmstudio_base_url,
        models: config.models.clone(),
    }
}

/// Resolves the system prompt: inline wins over file, both win over default.
fn resolve_system_prompt(config: &NaviConfig) -> String {
    // Inline system_prompt takes priority
    if let Some(ref prompt) = config.general.system_prompt {
        return prompt.clone();
    }

    // Try loading from system_prompt_file (relative to ~/.navi/)
    if let Some(ref file) = config.general.system_prompt_file {
        if let Some(home) = dirs::home_dir() {
            let prompt_path = home.join(".navi").join(file);
            match fs::read_to_string(&prompt_path) {
                Ok(contents) => {
                    let trimmed = contents.trim().to_string();
                    if !trimmed.is_empty() {
                        info!("Loaded system prompt from {}", prompt_path.display());
                        return trimmed;
                    }
                    warn!("System prompt file is empty: {}", prompt_path.display());
                }
                Err(e) => {
                    warn!(
                        "Failed to read system prompt file {}: {}",
                        prompt_path.display(),
                        e
                    );
                }
            }
        }
    }

    DEFAULT_SYSTEM_PROMPT.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_parses() {
        let config = NaviConfig::default();
        assert!(config.models.is_empty());
        assert!(config.general.default_provider.is_none());
    }

    #[test]
    fn test_resolve_uses_defaults_when_empty() {
        let config = NaviConfig::default();
        let resolved = resolve(&config, None);
        assert_eq!(resolved.max_agentic_rounds, DEFAULT_MAX_AGENTIC_ROUNDS);
        assert_eq!(resolved.max_output_tokens, DEFAULT_MAX_OUTPUT_TOKENS);
        assert_eq!(resolved.effort, Effort::default());
        assert!(resolved.system_prompt.starts_with("You are a helpful assistant"));
    }

    #[test]
    fn test_resolve_config_values_override_defaults() {
        let config = NaviConfig {
            general: GeneralConfig {
                default_provider: Some("lmstudio".to_string()),
                default_model: Some("my-model".to_string()),
                max_agentic_rounds: Some(5),
                max_output_tokens: Some(4096),
                reasoning_effort: Some(Effort::High),
                system_prompt: Some("Custom prompt.".to_string()),
                system_prompt_file: None,
            },
            ..Default::default()
        };
        let resolved = resolve(&config, None);
        assert_eq!(resolved.provider, "lmstudio");
        assert_eq!(resolved.model_name, "my-model");
        assert_eq!(resolved.max_agentic_rounds, 5);
        assert_eq!(resolved.max_output_tokens, 4096);
        assert_eq!(resolved.effort, Effort::High);
        assert_eq!(resolved.system_prompt, "Custom prompt.");
    }

    #[test]
    fn test_resolve_cli_provider_wins() {
        let config = NaviConfig {
            general: GeneralConfig {
                default_provider: Some("lmstudio".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let resolved = resolve(&config, Some("openrouter"));
        assert_eq!(resolved.provider, "openrouter");
    }

    #[test]
    fn test_toml_round_trip() {
        let toml_str = r#"
[general]
default_provider = "openrouter"
default_model = "anthropic/claude-sonnet-4"
max_agentic_rounds = 10
max_output_tokens = 8192
reasoning_effort = "medium"

[openrouter]
api_key = "sk-test-123"

[lmstudio]
base_url = "http://192.168.1.100:1234/v1"

[[models]]
name = "anthropic/claude-sonnet-4"
provider = "openrouter"
description = "Fast reasoning"

[[models]]
name = "local-model"
provider = "lmstudio"
"#;
        let config: NaviConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.general.default_provider.as_deref(),
            Some("openrouter")
        );
        assert_eq!(config.general.max_agentic_rounds, Some(10));
        assert_eq!(
            config.openrouter.api_key.as_deref(),
            Some("sk-test-123")
        );
        assert_eq!(config.models.len(), 2);
        assert_eq!(config.models[0].name, "anthropic/claude-sonnet-4");
        assert_eq!(config.models[1].description, None);
    }

    #[test]
    fn test_sparse_toml_parses() {
        // Only override one thing — everything else stays default
        let toml_str = r#"
[general]
default_model = "my-model"
"#;
        let config: NaviConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.default_model.as_deref(), Some("my-model"));
        assert!(config.general.default_provider.is_none());
        assert!(config.general.max_agentic_rounds.is_none());
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_inline_system_prompt_wins_over_file() {
        let config = NaviConfig {
            general: GeneralConfig {
                system_prompt: Some("Inline wins.".to_string()),
                system_prompt_file: Some("should-not-load.md".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let resolved = resolve(&config, None);
        assert_eq!(resolved.system_prompt, "Inline wins.");
    }

    #[test]
    fn test_model_entry_clone() {
        let entry = ModelEntry {
            name: "test".to_string(),
            provider: "openrouter".to_string(),
            description: Some("desc".to_string()),
        };
        let cloned = entry.clone();
        assert_eq!(cloned.name, "test");
        assert_eq!(cloned.description.as_deref(), Some("desc"));
    }
}
