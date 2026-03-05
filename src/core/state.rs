//! # Application State
//!
//! Core business state for Navi. This module contains domain logic only -
//! no TUI-specific types. Presentation state lives in the `tui` module.
//!
//! ```text
//! App
//! ├── provider: Arc<dyn CompletionProvider>  // LLM provider (config lifetime)
//! ├── session: SessionState                  // per-conversation state
//! ├── effort: Effort                         // reasoning effort level
//! ├── registry: Arc<ToolRegistry>            // tool registry
//! ├── provider_name: String                  // active provider name
//! └── ... config-driven fields ...
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

use crate::core::config::{ModelEntry, ResolvedConfig};
#[cfg(test)]
use crate::core::config::{self, DEFAULT_MAX_AGENTIC_ROUNDS, DEFAULT_MAX_OUTPUT_TOKENS};
use crate::core::tools::ToolRegistry;
use crate::inference::{CompletionProvider, Context, Effort, ToolDefinition, UsageStats};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Per-session state that gets fully replaced on NewSession / LoadSession.
///
/// Extracting this from App means session resets are a single assignment
/// instead of resetting 13 fields individually.
pub struct SessionState {
    pub context: Context,
    pub current_session_id: Option<String>,
    pub session_title: String,
    pub is_loading: bool,
    pub pending_tool_calls: HashSet<String>,
    pub stream_done: bool,
    pub had_tool_calls: bool,
    pub agentic_rounds: u8,
    pub usage_stats: UsageStats,
    pub message_stats: HashMap<usize, UsageStats>,
    pub session_total_tokens: u32,
    pub error: Option<String>,
    pub status_message: String,
}

impl SessionState {
    pub fn new(system_prompt: &str) -> Self {
        Self {
            context: Context::with_system_prompt(system_prompt.to_string()),
            current_session_id: None,
            session_title: String::new(),
            is_loading: false,
            pending_tool_calls: HashSet::new(),
            stream_done: false,
            had_tool_calls: false,
            agentic_rounds: 0,
            usage_stats: UsageStats::default(),
            message_stats: HashMap::new(),
            session_total_tokens: 0,
            error: None,
            status_message: String::from("Welcome to Navi!"),
        }
    }
}

pub struct App {
    pub provider: Arc<dyn CompletionProvider>,
    pub session: SessionState,
    pub effort: Effort,
    pub registry: Arc<ToolRegistry>,
    pub model_name: String,
    /// Provider name for the active model (e.g. "openrouter", "lmstudio").
    pub provider_name: String,

    // --- Config-driven fields ---
    pub max_agentic_rounds: u8,
    pub max_output_tokens: u32,
    pub system_prompt: String,
    pub available_models: Vec<ModelEntry>,
    pub openrouter_api_key: Option<String>,
    pub openrouter_base_url: String,
    pub lmstudio_base_url: String,
}

impl App {
    /// Creates an App with default settings. Used by tests via `test_app()`.
    #[cfg(test)]
    pub fn new(provider: Arc<dyn CompletionProvider>, model_name: String) -> Self {
        Self {
            provider,
            session: SessionState::new(config::DEFAULT_SYSTEM_PROMPT),
            model_name,
            effort: Effort::default(),
            registry: Arc::new(crate::core::tools::default_registry()),
            provider_name: String::new(),
            max_agentic_rounds: DEFAULT_MAX_AGENTIC_ROUNDS,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
            system_prompt: config::DEFAULT_SYSTEM_PROMPT.to_string(),
            available_models: Vec::new(),
            openrouter_api_key: None,
            openrouter_base_url: config::DEFAULT_OPENROUTER_BASE_URL.to_string(),
            lmstudio_base_url: config::DEFAULT_LMSTUDIO_BASE_URL.to_string(),
        }
    }

    /// Creates an App from resolved config values.
    pub fn from_config(provider: Arc<dyn CompletionProvider>, config: &ResolvedConfig) -> Self {
        Self {
            provider,
            session: SessionState::new(&config.system_prompt),
            model_name: config.model_name.clone(),
            effort: config.effort,
            registry: Arc::new(crate::core::tools::default_registry()),
            provider_name: config.provider.clone(),
            max_agentic_rounds: config.max_agentic_rounds,
            max_output_tokens: config.max_output_tokens,
            system_prompt: config.system_prompt.clone(),
            available_models: config.models.clone(),
            openrouter_api_key: config.openrouter_api_key.clone(),
            openrouter_base_url: config.openrouter_base_url.clone(),
            lmstudio_base_url: config.lmstudio_base_url.clone(),
        }
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.registry.definitions()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::test_app;

    #[test]
    fn test_app_new_defaults() {
        let app = test_app();
        assert_eq!(app.session.status_message, "Welcome to Navi!");
        assert!(!app.session.is_loading);
        assert_eq!(app.model_name, "test-model");
    }
}
