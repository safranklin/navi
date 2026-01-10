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

use crate::api::ModelSegment;
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
    // Receive a response segment from the API
    ResponseReceived(ModelSegment),
    // Scroll the chat view up (see older messages)
    ScrollUp,
    // Scroll the chat view down (see newer messages)
    ScrollDown,
}

pub fn update(app_state: &mut App, action: Action) {
    match action {
        Action::Quit => {
            app_state.should_quit = true;
        }
        Action::InputChar(c) => {
            app_state.input_buffer.push(c);
        }
        Action::Backspace => {
            if !app_state.input_buffer.is_empty() {
                app_state.input_buffer.pop();
            }
        }
        Action::Submit => {
            if app_state.input_buffer.is_empty() {
                return; // noop on empty input
            }
            let user_message = app_state.input_buffer.clone();
            app_state.context.add_user_message(user_message);
            app_state.input_buffer.clear();
            app_state.is_loading = true;
            app_state.status_message = String::from("Loading...");
        }
        Action::ResponseReceived(segment) => {
            app_state.context.add(segment);
            app_state.is_loading = false;
            app_state.status_message = String::from("Response received.");
            // Note: With top-down scrolling, "unseen" indicator logic is more complex
            // Would need to know if user is at bottom (requires UI geometry)
            // For now, just add the content - user can scroll down to see it
        }
        Action::ScrollUp => {
            app_state.scroll_state.scroll_up();
        }
        Action::ScrollDown => {
            app_state.scroll_state.scroll_down();
            // Clear indicator when user scrolls down (they're trying to catch up)
            // TODO: Ideally check if actually at bottom, but that requires geometry from UI
            app_state.has_unseen_content = false;
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quit_sets_should_quit() {
        let mut app = App::new("test-model".to_string());
        assert!(!app.should_quit);
        
        update(&mut app, Action::Quit);
        
        assert!(app.should_quit);
    }
    
    #[test]
    fn test_input_char_adds_character() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::from("Hello");
        update(&mut app, Action::InputChar('!'));
        assert_eq!(app.input_buffer, "Hello!");
    }

    #[test]
    fn test_backspace_removes_character() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::from("Hello!");
        update(&mut app, Action::Backspace);
        assert_eq!(app.input_buffer, "Hello");
    }

    #[test]
    fn test_backspace_on_empty_buffer() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::new();
        update(&mut app, Action::Backspace);
        assert_eq!(app.input_buffer, "");
    }

    #[test]
    fn test_submit_noop_on_empty_input() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::new();
        let initial_context_len = app.context.items.len();
        update(&mut app, Action::Submit);
        assert_eq!(app.context.items.len(), initial_context_len); // No new message added
        assert!(!app.is_loading); // is_loading should remain false
    }

    #[test]
    fn test_submit_clears_input_and_adds_message() {
        let mut app = App::new("test-model".to_string());
        app.input_buffer = String::from("Hello, model!");
        update(&mut app, Action::Submit);
        assert_eq!(app.input_buffer, "");
        assert_eq!(app.context.items.len(), 2); // Assuming initial context contains the system directive and now the user message
        assert_eq!(app.context.items[1].content, "Hello, model!");
        assert!(app.is_loading);
    }

    #[test]
    fn test_response_received_adds_segment() {
        let mut app = App::new("test-model".to_string());
        let segment = ModelSegment {
            source: crate::api::types::Source::Model,
            content: String::from("Response from model."),
        };
        update(&mut app, Action::ResponseReceived(segment.clone()));
        assert_eq!(app.context.items.len(), 2); // Assuming initial context contains, the system directive and now the model response
        assert_eq!(app.context.items[1].content, "Response from model.");
        assert!(!app.is_loading);
    }
}