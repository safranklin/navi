//! # Application State
//!
//! Everything Navi "knows" at any moment lives in the `App` struct.
//! No scattered globals: one struct representing a single source of truth.
//!
//! ```text
//! App
//! ├── context: Context                    // conversation history
//! ├── input_buffer: String                // what the user is currently typing
//! ├── scroll_state: ScrollViewState       // scroll position (owned by tui-scrollview)
//! ├── has_unseen_content: bool            // "↓ New" indicator
//! ├── status_message: String              // status bar text
//! ├── should_quit: bool                   // exit signal
//! ├── is_loading: bool                    // waiting for API response
//! ├── model_name: String                  // current model
//! └── error: Option<String>               // error message
//! ```
//!
//! State changes only happen through `update(state, action)` in action.rs.
//! This keeps things predictable, so no surprise mutations.

use crate::api::Context;
use tui_scrollview::ScrollViewState;

pub struct App {
    pub context: Context,
    pub input_buffer: String,
    pub scroll_state: ScrollViewState,   // Component-owned scroll state
    pub has_unseen_content: bool,        // Shows "↓ New" when content below viewport
    pub status_message: String,
    pub should_quit: bool,
    pub is_loading: bool,
    pub model_name: String,
    pub error: Option<String>,
}

impl App {
    pub fn new(model_name: String) -> Self {
        Self {
            context: Context::new(),
            input_buffer: String::new(),
            scroll_state: ScrollViewState::default(),
            has_unseen_content: false,
            status_message: String::from("Welcome to Navi!"),
            should_quit: false,
            is_loading: false,
            model_name,
            error: None,
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