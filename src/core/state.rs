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
//! ├── model: ActiveModel                     // model name + provider
//! ├── config: ResolvedConfig                 // connection details (URLs, keys)
//! └── ... mutable overrides ...
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

#[cfg(test)]
use crate::core::config::{self, DEFAULT_MAX_AGENTIC_ROUNDS, DEFAULT_MAX_OUTPUT_TOKENS};
use crate::core::config::{ModelEntry, ResolvedConfig};
use crate::core::tools::ToolRegistry;
use crate::inference::{CompletionProvider, Context, Effort, ToolCall, ToolDefinition, UsageStats};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// The currently-selected model and provider. These two must stay in sync -
/// a model name alone is ambiguous across providers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveModel {
    pub name: String,
    pub provider: String,
}

impl ActiveModel {
    pub fn new(name: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: provider.into(),
        }
    }
}

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
    /// Tools awaiting user approval before execution.
    pub approval_queue: VecDeque<ToolCall>,
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
            approval_queue: VecDeque::new(),
        }
    }
}

pub struct App {
    pub provider: Arc<dyn CompletionProvider>,
    pub session: SessionState,
    pub effort: Effort,
    pub registry: Arc<ToolRegistry>,
    pub model: ActiveModel,

    // --- Config-driven fields ---
    pub config: ResolvedConfig,
    pub max_agentic_rounds: u8,
    pub max_output_tokens: u32,
    pub system_prompt: String,
    pub available_models: Vec<ModelEntry>,
}

impl App {
    /// Creates an App with default settings. Used by tests via `test_app()`.
    #[cfg(test)]
    pub fn new(provider: Arc<dyn CompletionProvider>, model_name: String) -> Self {
        let resolved = config::resolve(&config::NaviConfig::default(), None);
        Self {
            provider,
            session: SessionState::new(config::DEFAULT_SYSTEM_PROMPT),
            model: ActiveModel::new(model_name, ""),
            effort: Effort::default(),
            registry: Arc::new(crate::core::tools::default_registry()),
            config: resolved,
            max_agentic_rounds: DEFAULT_MAX_AGENTIC_ROUNDS,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
            system_prompt: config::DEFAULT_SYSTEM_PROMPT.to_string(),
            available_models: Vec::new(),
        }
    }

    /// Creates an App from resolved config values.
    pub fn from_config(provider: Arc<dyn CompletionProvider>, config: ResolvedConfig) -> Self {
        Self {
            provider,
            session: SessionState::new(&config.system_prompt),
            model: ActiveModel::new(config.model_name.clone(), config.provider.clone()),
            effort: config.effort,
            registry: Arc::new(crate::core::tools::default_registry()),
            max_agentic_rounds: config.max_agentic_rounds,
            max_output_tokens: config.max_output_tokens,
            system_prompt: config.system_prompt.clone(),
            available_models: config.models.clone(),
            config,
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
        assert_eq!(app.model.name, "test-model");
    }
}
