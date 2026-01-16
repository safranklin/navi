//! # Application State
//!
//! Core business state for Navi. This module contains domain logic only -
//! no TUI-specific types. Presentation state lives in the `tui` module.
//!
//! ```text
//! App
//! ├── context: Context              // conversation history
//! ├── status_message: String        // status bar text
//! ├── model_name: String            // current model
//! ├── is_loading: bool              // waiting for API
//! └── error: Option<String>         // error message
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

use crate::api::Context;

#[derive(Debug, PartialEq)]
pub struct App {
    pub context: Context,
    pub status_message: String,
    pub model_name: String,
    pub is_loading: bool,
    pub error: Option<String>,
}

impl App {
    pub fn new(model_name: String) -> Self {
        Self {
            context: Context::new(),
            status_message: String::from("Welcome to Navi!"),
            model_name,
            is_loading: false,
            error: None,
        }
    }
}

#[test]
fn test_app_new_defaults() {
    let app = App::new("model".to_string());
    assert_eq!(app.status_message, "Welcome to Navi!");
    assert!(!app.is_loading);
    assert_eq!(app.model_name, "model");
}