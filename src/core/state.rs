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
//! ├── tools: Vec<ToolDefinition>    // available tools
//! └── pending_tool_calls: HashSet   // call_ids awaiting results
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

use std::collections::HashSet;
use std::sync::Arc;
use crate::inference::{CompletionProvider, Context, Effort, ToolDefinition};

pub struct App {
    pub provider: Arc<dyn CompletionProvider>,
    pub context: Context,
    pub status_message: String,
    pub model_name: String,
    pub is_loading: bool,
    pub effort: Effort,
    pub error: Option<String>,
    pub tools: Vec<ToolDefinition>,
    pub pending_tool_calls: HashSet<String>, // call_ids awaiting results
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
            tools: crate::core::tools::available(),
            pending_tool_calls: HashSet::new(),
        }
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
