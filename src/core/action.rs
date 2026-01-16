//! # Actions
//!
//! Core business actions for Navi. These actions modify domain state only.
//! TUI-specific actions (input, scroll, hover) are handled directly in the TUI module.
//!
//! The `update()` function takes the current state and an action,
//! then returns the new state. No side effects here. I/O happens elsewhere.
//!
//! ```text
//! State + Action  →  update()  →  New State
//! ```
//!
//! This makes everything testable: `assert_eq!(update(state, action), expected)`.
//! And debuggable: log every action, replay the exact session.

use crate::core::state::App;

pub enum Action {
    // Quit the application
    Quit,
    // Submit a user message (TUI passes the message content)
    Submit(String),
    // Receive a chunk of content from the API (streaming)
    ResponseChunk(String),
    // Receive a chunk of thinking/reasoning from the API
    ThinkingChunk(String),
    // Signal that the streaming response is complete
    ResponseDone,
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Render, // Just re-render (default behavior really, but explicit is nice)
    Quit,
    SpawnRequest,
}

pub fn update(app_state: &mut App, action: Action) -> Effect {
    match action {
        Action::Quit => {
            Effect::Quit
        }
        Action::Submit(message) => {
            if message.is_empty() {
                return Effect::None; // noop on empty input
            }
            app_state.context.add_user_message(message);
            app_state.is_loading = true;
            app_state.status_message = String::from("Loading...");
            Effect::SpawnRequest
        }
        Action::ResponseChunk(chunk) => {
            app_state.context.append_to_last_model_message(&chunk);
            app_state.status_message = String::from("Receiving...");
            Effect::Render
        }
        Action::ThinkingChunk(chunk) => {
            app_state.context.append_to_last_thinking_message(&chunk);
            app_state.status_message = String::from("Thinking...");
            Effect::Render
        }
        Action::ResponseDone => {
            app_state.is_loading = false;
            app_state.status_message = String::from("Response complete.");
            Effect::Render
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quit_returns_quit_effect() {
        let mut app = App::new("test-model".to_string());

        let effect = update(&mut app, Action::Quit);

        assert_eq!(effect, Effect::Quit);
    }

    #[test]
    fn test_submit_noop_on_empty_message() {
        let mut app = App::new("test-model".to_string());
        let initial_context_len = app.context.items.len();

        let effect = update(&mut app, Action::Submit(String::new()));

        assert_eq!(app.context.items.len(), initial_context_len);
        assert!(!app.is_loading);
        assert_eq!(effect, Effect::None);
    }

    #[test]
    fn test_submit_adds_message_and_triggers_request() {
        let mut app = App::new("test-model".to_string());

        let effect = update(&mut app, Action::Submit("Hello, model!".to_string()));

        assert_eq!(app.context.items.len(), 2); // System + User
        assert_eq!(app.context.items[1].content, "Hello, model!");
        assert!(app.is_loading);
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_response_chunk_appends_and_updates_status() {
        let mut app = App::new("test-model".to_string());
        app.is_loading = true;

        let effect = update(&mut app, Action::ResponseChunk("Response ".to_string()));

        assert_eq!(app.context.items.len(), 2); // System + Model (new)
        assert_eq!(app.context.items[1].content, "Response ");
        assert_eq!(app.context.items[1].source, crate::api::types::Source::Model);
        assert!(app.is_loading);
        assert_eq!(app.status_message, "Receiving...");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_response_done_stops_loading() {
        let mut app = App::new("test-model".to_string());
        app.is_loading = true;

        let effect = update(&mut app, Action::ResponseDone);

        assert!(!app.is_loading);
        assert_eq!(app.status_message, "Response complete.");
        assert_eq!(effect, Effect::Render);
    }
}