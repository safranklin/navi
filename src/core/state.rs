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
