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

use log::{debug, warn};
use crate::core::state::App;
use crate::inference::{ToolCall, ToolResult};

#[derive(Debug)]
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
    // Model wants to call a tool
    ToolCallReceived(ToolCall),
    // A tool execution completed
    ToolResultReady { call_id: String, output: String },
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Render,
    Quit,
    SpawnRequest,
    ExecuteTool(ToolCall), // Run a tool asynchronously
}

pub fn update(app_state: &mut App, action: Action) -> Effect {
    match action {
        Action::Quit => {
            Effect::Quit
        }
        Action::Submit(message) => {
            if message.is_empty() || app_state.is_loading {
                return Effect::None; // noop on empty input or if already loading
            }
            app_state.context.add_user_message(message);
            app_state.is_loading = true;
            app_state.status_message = String::from("Loading...");
            Effect::SpawnRequest
        }
        Action::ResponseChunk(chunk) => {
            app_state.context.append_to_last_model_message(&chunk);
            // Log total message length after append
            if let Some(crate::inference::ContextItem::Message(last)) = app_state.context.items.last() {
                debug!("ResponseChunk applied: chunk_len={}, total_msg_len={}", chunk.len(), last.content.len());
            }
            app_state.status_message = String::from("Receiving...");
            Effect::Render
        }
        Action::ThinkingChunk(chunk) => {
            app_state.context.append_to_last_thinking_message(&chunk);
            debug!("ThinkingChunk applied: chunk_len={}", chunk.len());
            app_state.status_message = String::from("Thinking...");
            Effect::Render
        }
        Action::ResponseDone => {
            if let Some(crate::inference::ContextItem::Message(last)) = app_state.context.items.last() {
                debug!("ResponseDone: final message length={}", last.content.len());
            }
            if app_state.pending_tool_calls.is_empty() {
                app_state.is_loading = false;
                app_state.status_message = String::from("Response complete.");
            }
            // If tools are pending, stay loading — the agentic loop continues
            Effect::Render
        }
        Action::ToolCallReceived(tool_call) => {
            if tool_call.call_id.is_empty() {
                warn!("Received tool call with empty call_id, skipping: {}", tool_call.name);
                return Effect::Render;
            }
            app_state.pending_tool_calls.insert(tool_call.call_id.clone());
            app_state.context.add_tool_call(tool_call.clone());
            app_state.status_message = format!("Calling: {}...", tool_call.name);
            Effect::ExecuteTool(tool_call)
        }
        Action::ToolResultReady { call_id, output } => {
            app_state.pending_tool_calls.remove(&call_id);
            app_state.context.add_tool_result(ToolResult {
                call_id,
                output,
            });

            if app_state.pending_tool_calls.is_empty() {
                app_state.status_message = String::from("Resuming...");
                Effect::SpawnRequest
            } else {
                app_state.status_message = format!(
                    "Waiting for {} more tool(s)...",
                    app_state.pending_tool_calls.len()
                );
                Effect::Render
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{ContextItem, Source};
    use crate::test_support::test_app;

    #[test]
    fn test_quit_returns_quit_effect() {
        let mut app = test_app();

        let effect = update(&mut app, Action::Quit);

        assert_eq!(effect, Effect::Quit);
    }

    #[test]
    fn test_submit_noop_on_empty_message() {
        let mut app = test_app();
        let initial_context_len = app.context.items.len();

        let effect = update(&mut app, Action::Submit(String::new()));

        assert_eq!(app.context.items.len(), initial_context_len);
        assert!(!app.is_loading);
        assert_eq!(effect, Effect::None);
    }

    #[test]
    fn test_submit_adds_message_and_triggers_request() {
        let mut app = test_app();

        let effect = update(&mut app, Action::Submit("Hello, model!".to_string()));

        assert_eq!(app.context.items.len(), 2); // System + User
        assert!(matches!(&app.context.items[1], ContextItem::Message(seg) if seg.content == "Hello, model!"));
        assert!(app.is_loading);
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_response_chunk_appends_and_updates_status() {
        let mut app = test_app();
        app.is_loading = true;

        let effect = update(&mut app, Action::ResponseChunk("Response ".to_string()));

        assert_eq!(app.context.items.len(), 2); // System + Model (new)
        assert!(matches!(&app.context.items[1], ContextItem::Message(seg) if seg.content == "Response " && seg.source == Source::Model));
        assert!(app.is_loading);
        assert_eq!(app.status_message, "Receiving...");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_response_done_stops_loading() {
        let mut app = test_app();
        app.is_loading = true;

        let effect = update(&mut app, Action::ResponseDone);

        assert!(!app.is_loading);
        assert_eq!(app.status_message, "Response complete.");
        assert_eq!(effect, Effect::Render);
    }

    fn make_tool_call(name: &str, call_id: &str) -> crate::inference::ToolCall {
        crate::inference::ToolCall {
            id: format!("fc_{call_id}"),
            call_id: call_id.to_string(),
            name: name.to_string(),
            arguments: "{}".to_string(),
        }
    }

    #[test]
    fn test_tool_call_received_returns_execute_effect() {
        let mut app = test_app();
        app.is_loading = true;
        let tc = make_tool_call("get_weather", "call_1");

        let effect = update(&mut app, Action::ToolCallReceived(tc.clone()));

        assert!(app.pending_tool_calls.contains("call_1"));
        assert!(matches!(effect, Effect::ExecuteTool(ref t) if t.call_id == "call_1"));
        assert!(app.status_message.contains("get_weather"));
    }

    #[test]
    fn test_tool_result_ready_last_tool_spawns_request() {
        let mut app = test_app();
        app.is_loading = true;
        app.pending_tool_calls.insert("call_1".to_string());

        let effect = update(&mut app, Action::ToolResultReady {
            call_id: "call_1".to_string(),
            output: r#"{"temp": 72}"#.to_string(),
        });

        assert!(app.pending_tool_calls.is_empty());
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_tool_result_ready_with_remaining_tools_renders() {
        let mut app = test_app();
        app.is_loading = true;
        app.pending_tool_calls.insert("call_1".to_string());
        app.pending_tool_calls.insert("call_2".to_string());

        let effect = update(&mut app, Action::ToolResultReady {
            call_id: "call_1".to_string(),
            output: "done".to_string(),
        });

        assert_eq!(app.pending_tool_calls.len(), 1);
        assert_eq!(effect, Effect::Render);
        assert!(app.status_message.contains("1 more"));
    }

    #[test]
    fn test_response_done_stays_loading_when_tools_pending() {
        let mut app = test_app();
        app.is_loading = true;
        app.pending_tool_calls.insert("call_1".to_string());

        let effect = update(&mut app, Action::ResponseDone);

        assert!(app.is_loading); // Still loading — tools not done yet
        assert_eq!(effect, Effect::Render);
    }
}