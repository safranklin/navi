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
//! ├── last_response_id: Option<String>  // server response ID for caching
//! └── context_watermark: usize     // items cached server-side
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

use crate::core::tools::ToolRegistry;
use crate::inference::{CompletionProvider, Context, Effort, ToolDefinition};
use std::collections::HashSet;
use std::sync::Arc;

/// Maximum number of agentic tool-calling rounds before the loop is forcibly stopped.
pub const MAX_AGENTIC_ROUNDS: u8 = 20;

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
    /// Server's response ID from the last successful completion (for prompt caching).
    pub last_response_id: Option<String>,
    /// Index into context.items up to which the server has cached.
    /// Items before this index are reconstructed server-side via previous_response_id.
    pub context_watermark: usize,
}

impl App {
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
            last_response_id: None,
            context_watermark: 0,
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
