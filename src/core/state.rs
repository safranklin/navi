//! # Application State
//!
//! Core business state for Navi. This module contains domain logic only -
//! no TUI-specific types. Presentation state lives in the `tui` module.
//!
//! ```text
//! App
//! ├── provider: Arc<dyn CompletionProvider>  // LLM provider
//! ├── context: Context              // conversation history
//! ├── status_message: String        // status bar text
//! ├── model_name: String            // current model
//! ├── is_loading: bool              // waiting for API
//! ├── effort: Effort                // reasoning effort level
//! ├── error: Option<String>         // error message
//! ├── registry: Arc<ToolRegistry>   // tool registry
//! ├── pending_tool_calls: HashSet   // call_ids awaiting results
//! ├── agentic_rounds: u8           // loop iteration counter
//! ├── stream_done: bool            // SSE stream finished
//! ├── had_tool_calls: bool         // tool calls received this round
//! ├── usage_stats: UsageStats     // accumulated inference metrics
//! └── message_stats: HashMap      // per-message stats keyed by item index
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

use crate::core::config::{
    self, ModelEntry, ResolvedConfig, DEFAULT_MAX_AGENTIC_ROUNDS, DEFAULT_MAX_OUTPUT_TOKENS,
};
use crate::core::tools::ToolRegistry;
use crate::inference::{CompletionProvider, Context, Effort, ToolDefinition, UsageStats};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct App {
    pub provider: Arc<dyn CompletionProvider>,
    pub context: Context,
    pub status_message: String,
    pub model_name: String,
    pub is_loading: bool,
    pub effort: Effort,
    pub error: Option<String>,
    pub registry: Arc<ToolRegistry>,
    pub pending_tool_calls: HashSet<String>, // call_ids awaiting results
    pub agentic_rounds: u8,
    /// True after `ResponseDone` — the SSE stream has finished sending events.
    pub stream_done: bool,
    /// True if any `ToolCallReceived` arrived this round.
    pub had_tool_calls: bool,
    /// Accumulated usage stats for the current submission (across agentic rounds).
    pub usage_stats: UsageStats,
    /// Per-message usage stats, keyed by context item index.
    /// Each model message gets the stats from its agentic round.
    pub message_stats: HashMap<usize, UsageStats>,
    /// Active session ID (None = unsaved new session).
    pub current_session_id: Option<String>,

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
    /// Creates an App with default settings. Used by tests and backward-compat paths.
    pub fn new(provider: Arc<dyn CompletionProvider>, model_name: String) -> Self {
        Self {
            provider,
            context: Context::new(),
            status_message: String::from("Welcome to Navi!"),
            model_name,
            is_loading: false,
            effort: Effort::default(),
            error: None,
            registry: Arc::new(crate::core::tools::default_registry()),
            pending_tool_calls: HashSet::new(),
            agentic_rounds: 0,
            stream_done: false,
            had_tool_calls: false,
            usage_stats: UsageStats::default(),
            message_stats: HashMap::new(),
            current_session_id: None,
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
    pub fn from_config(
        provider: Arc<dyn CompletionProvider>,
        config: &ResolvedConfig,
    ) -> Self {
        Self {
            provider,
            context: Context::with_system_prompt(config.system_prompt.clone()),
            status_message: String::from("Welcome to Navi!"),
            model_name: config.model_name.clone(),
            is_loading: false,
            effort: config.effort,
            error: None,
            registry: Arc::new(crate::core::tools::default_registry()),
            pending_tool_calls: HashSet::new(),
            agentic_rounds: 0,
            stream_done: false,
            had_tool_calls: false,
            usage_stats: UsageStats::default(),
            message_stats: HashMap::new(),
            current_session_id: None,
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
        assert_eq!(app.status_message, "Welcome to Navi!");
        assert!(!app.is_loading);
        assert_eq!(app.model_name, "test-model");
    }
}
