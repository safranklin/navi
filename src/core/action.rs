//! # Actions
//!
//! Everything that can happen in Navi becomes an `Action`.
//! User presses Enter? That's `Action::SubmitMessage`.
//! API responds? That's `Action::ResponseReceived(segment)`.
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
    // Add a character to the input buffer
    InputChar(char),
    // Remove the last character from the input buffer
    Backspace,
    // Submit the current input buffer as a message
    Submit,
    // Receive a chunk of content from the API (streaming)
    ResponseChunk(String),
    // Signal that the streaming response is complete
    ResponseDone,
    // Scroll the chat view up (see older messages)
    ScrollUp,
    // Scroll the chat view down (see newer messages)
    ScrollDown,
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
        Action::InputChar(c) => {
            app_state.input_buffer.push(c);
            Effect::Render
        }
        Action::Backspace => {
            if !app_state.input_buffer.is_empty() {
                app_state.input_buffer.pop();
            }
            Effect::Render
        }
        Action::Submit => {
            if app_state.input_buffer.is_empty() {
                return Effect::None; // noop on empty input
            }
            let user_message = app_state.input_buffer.clone();
            app_state.context.add_user_message(user_message);
            app_state.input_buffer.clear();
            app_state.is_loading = true;
            app_state.status_message = String::from("Loading...");
            Effect::SpawnRequest
        }
        Action::ResponseChunk(chunk) => {
            app_state.context.append_to_last_model_message(&chunk);
            // We don't set is_loading = false here, as more chunks are coming
            app_state.status_message = String::from("Receiving...");
            Effect::Render
        }
        Action::ResponseDone => {
            app_state.is_loading = false;
            app_state.status_message = String::from("Response complete.");
            Effect::Render
        }
        Action::ScrollUp => {
            app_state.scroll_state.scroll_up();
            Effect::Render
        }
        Action::ScrollDown => {
            app_state.scroll_state.scroll_down();
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
    fn test_input_char_adds_character() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::from("Hello");
        let effect = update(&mut app, Action::InputChar('!'));
        assert_eq!(app.input_buffer, "Hello!");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_backspace_removes_character() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::from("Hello!");
        let effect = update(&mut app, Action::Backspace);
        assert_eq!(app.input_buffer, "Hello");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_backspace_on_empty_buffer() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::new();
        let effect = update(&mut app, Action::Backspace);
        assert_eq!(app.input_buffer, "");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_submit_noop_on_empty_input() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::new();
        let initial_context_len = app.context.items.len();
        let effect = update(&mut app, Action::Submit);
        assert_eq!(app.context.items.len(), initial_context_len); // No new message added
        assert!(!app.is_loading); // is_loading should remain false
        assert_eq!(effect, Effect::None);
    }

    #[test]
    fn test_submit_clears_input_and_adds_message() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::from("Hello, model!");
        let effect = update(&mut app, Action::Submit);
        assert_eq!(app.input_buffer, "");
        assert_eq!(app.context.items.len(), 2); // Assuming initial context contains the system directive and now the user message
        assert_eq!(app.context.items[1].content, "Hello, model!");
        assert!(app.is_loading);
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_response_chunk_appends_and_updates_status() {
        let mut app = App::new("test-model".to_string());
        // Simulate loading state
        app.is_loading = true;
        
        let chunk = String::from("Response ");
        let effect = update(&mut app, Action::ResponseChunk(chunk));
        
        // Should have added the chunk to context
        assert_eq!(app.context.items.len(), 2); // System + Model (new)
        assert_eq!(app.context.items[1].content, "Response ");
        assert_eq!(app.context.items[1].source, crate::api::types::Source::Model);
        
        // Should still be loading
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