//! # Application State
//!
//! Everything Navi "knows" at any moment lives in the `App` struct.
//! No scattered globals: one struct representing a single source of truth.
//!
//! ```text
//! App
//! ├── context: Context         // conversation history (reuses api::types)
//! ├── input_buffer: String     // what the user is currently typing
//! ├── scroll_offset: usize     // where in the chat history we're viewing
//! ├── status_message: String   // status bar text
//! ├── should_quit: bool        // exit signal
//! ├── is_loading: bool         // waiting for API response
//! └── model_name: String       // current model
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

use crate::api::Context;

pub struct App {
    pub context: Context, // model state
    pub input_buffer: String , // user input
    // pub scroll_offset: usize, // where in the chat history we're viewing
    pub status_message: String, // status bar text
    pub should_quit: bool, // exit signal
    pub is_loading: bool, // waiting for API response
    pub model_name: String, // current model
}

impl App {
    pub fn new(model_name: String) -> Self {
        Self {
            context: Context::new(),
            input_buffer: String::new(),
            // scroll_offset: 0,
            status_message: String::from("Welcome to Navi!"),
            should_quit: false,
            is_loading: false,
            model_name,
        }
    }
}

#[test]
fn test_app_new_defaults() {
    let app = App::new("model".to_string());
    assert_eq!(app.status_message, "Welcome to Navi!");
    assert!(!app.should_quit);
    assert!(!app.is_loading);
    assert!(app.input_buffer.is_empty());
}