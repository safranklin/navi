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

use crate::core::session::SessionData;
use crate::core::state::{App, MAX_AGENTIC_ROUNDS};
use crate::inference::{Context, ToolCall, ToolResult, UsageStats};
use log::{debug, warn};

#[derive(Debug)]
pub enum Action {
    // Quit the application
    Quit,
    // Submit a user message (TUI passes the message content)
    Submit(String),
    // Receive a chunk of content from the API (streaming)
    ResponseChunk {
        text: String,
        item_id: Option<String>,
    },
    // Receive a chunk of thinking/reasoning from the API
    ThinkingChunk {
        text: String,
        item_id: Option<String>,
    },
    // Signal that the streaming response is complete, with optional usage stats.
    ResponseDone(Option<UsageStats>),
    // Model wants to call a tool
    ToolCallReceived(ToolCall),
    // A tool execution completed
    ToolResultReady {
        call_id: String,
        output: String,
    },
    // User cancelled the in-progress generation
    CancelGeneration,
    // Replace context with a loaded session
    LoadSession(SessionData),
    // Reset to a fresh conversation
    NewSession,
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Render,
    Quit,
    SpawnRequest,
    ExecuteTool(ToolCall), // Run a tool asynchronously
    SaveSession,           // Persist current session to disk
}

/// Checks whether the current agentic round is fully complete (stream finished
/// AND all tool results received). Called by both `ResponseDone` and `ToolResultReady`.
///
/// This two-sided gate prevents the race where a fast tool result arrives before
/// the stream has finished sending all tool calls, which would prematurely fire
/// `SpawnRequest` and re-enter the agentic loop with incomplete context.
fn check_round_complete(app_state: &mut App) -> Effect {
    if app_state.stream_done && app_state.pending_tool_calls.is_empty() {
        if app_state.had_tool_calls {
            // Tool-calling round complete — advance the agentic loop
            app_state.agentic_rounds += 1;
            if app_state.agentic_rounds > MAX_AGENTIC_ROUNDS {
                warn!("Agentic loop limit reached ({} rounds)", MAX_AGENTIC_ROUNDS);
                app_state.is_loading = false;
                app_state.error = Some(format!(
                    "Agentic loop stopped after {} rounds. The model may be stuck in a tool-calling loop.",
                    MAX_AGENTIC_ROUNDS
                ));
                app_state.status_message = String::from("Loop limit reached.");
                app_state.stream_done = false;
                app_state.had_tool_calls = false;
                Effect::Render
            } else {
                app_state.status_message = String::from("Resuming...");
                app_state.stream_done = false;
                app_state.had_tool_calls = false;
                Effect::SpawnRequest
            }
        } else {
            // Pure text response — no tools were called
            app_state.is_loading = false;
            app_state.status_message = app_state.usage_stats.display_summary();
            Effect::SaveSession
        }
    } else if !app_state.pending_tool_calls.is_empty() {
        app_state.status_message = format!(
            "Waiting for {} more tool(s)...",
            app_state.pending_tool_calls.len()
        );
        Effect::Render
    } else {
        // Stream not done yet, or tools still pending — keep waiting
        Effect::Render
    }
}

pub fn update(app_state: &mut App, action: Action) -> Effect {
    match action {
        Action::Quit => Effect::Quit,
        Action::Submit(message) => {
            if message.is_empty() || app_state.is_loading {
                return Effect::None; // noop on empty input or if already loading
            }
            app_state.context.add_user_message(message);
            app_state.is_loading = true;
            app_state.agentic_rounds = 0;
            app_state.stream_done = false;
            app_state.had_tool_calls = false;
            app_state.usage_stats = UsageStats::default();
            app_state.message_stats.clear();
            app_state.status_message = String::from("Loading...");
            Effect::SpawnRequest
        }
        Action::ResponseChunk { text, item_id } => {
            app_state
                .context
                .append_to_last_model_message(&text, item_id.as_deref());
            // Log total message length after append
            if let Some(crate::inference::ContextItem::Message(last)) =
                app_state.context.items.last()
            {
                debug!(
                    "ResponseChunk applied: chunk_len={}, total_msg_len={}",
                    text.len(),
                    last.content.len()
                );
            }
            app_state.status_message = String::from("Receiving...");
            Effect::Render
        }
        Action::ThinkingChunk { text, item_id } => {
            app_state
                .context
                .append_to_last_thinking_message(&text, item_id.as_deref());
            debug!("ThinkingChunk applied: chunk_len={}", text.len());
            app_state.status_message = String::from("Thinking...");
            Effect::Render
        }
        Action::ResponseDone(stats) => {
            app_state.context.clear_active_streams();
            app_state.stream_done = true;
            if let Some(round_stats) = stats {
                app_state.usage_stats.accumulate(&round_stats);
                // Store per-message stats on the last Model message
                if let Some(idx) = app_state
                    .context
                    .items
                    .iter()
                    .rposition(|item| {
                        matches!(item, crate::inference::ContextItem::Message(seg) if seg.source == crate::inference::Source::Model)
                    })
                {
                    app_state.message_stats.insert(idx, round_stats);
                }
            }
            if let Some(crate::inference::ContextItem::Message(last)) =
                app_state.context.items.last()
            {
                debug!("ResponseDone: final message length={}", last.content.len());
            }
            check_round_complete(app_state)
        }
        Action::ToolCallReceived(tool_call) => {
            if tool_call.call_id.is_empty() {
                warn!(
                    "Received tool call with empty call_id, skipping: {}",
                    tool_call.name
                );
                return Effect::Render;
            }
            app_state.had_tool_calls = true;
            app_state
                .pending_tool_calls
                .insert(tool_call.call_id.clone());
            app_state.context.add_tool_call(tool_call.clone());
            app_state.status_message = format!("Calling: {}...", tool_call.name);
            Effect::ExecuteTool(tool_call)
        }
        Action::ToolResultReady { call_id, output } => {
            app_state.pending_tool_calls.remove(&call_id);
            app_state
                .context
                .add_tool_result(ToolResult { call_id, output });
            check_round_complete(app_state)
        }
        Action::CancelGeneration => {
            app_state.is_loading = false;
            app_state.pending_tool_calls.clear();
            app_state.stream_done = false;
            app_state.had_tool_calls = false;
            app_state.usage_stats = UsageStats::default();
            app_state.context.clear_active_streams();
            app_state.status_message = String::from("Cancelled.");
            Effect::Render
        }
        Action::LoadSession(data) => {
            // Rebuild fresh context with system directive, then push loaded items
            let mut context = Context::new();
            for item in data.items {
                context.items.push(item);
            }
            app_state.context = context;
            app_state.current_session_id = Some(data.meta.id);
            app_state.model_name = data.meta.model_name;
            app_state.is_loading = false;
            app_state.pending_tool_calls.clear();
            app_state.stream_done = false;
            app_state.had_tool_calls = false;
            app_state.agentic_rounds = 0;
            app_state.usage_stats = UsageStats::default();
            app_state.message_stats.clear();
            app_state.error = None;
            app_state.status_message = format!("Loaded: {}", data.meta.title);
            Effect::Render
        }
        Action::NewSession => {
            app_state.context = Context::new();
            app_state.current_session_id = None;
            app_state.is_loading = false;
            app_state.pending_tool_calls.clear();
            app_state.stream_done = false;
            app_state.had_tool_calls = false;
            app_state.agentic_rounds = 0;
            app_state.usage_stats = UsageStats::default();
            app_state.message_stats.clear();
            app_state.error = None;
            app_state.status_message = String::from("New session.");
            Effect::Render
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
        assert!(
            matches!(&app.context.items[1], ContextItem::Message(seg) if seg.content == "Hello, model!")
        );
        assert!(app.is_loading);
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_response_chunk_appends_and_updates_status() {
        let mut app = test_app();
        app.is_loading = true;

        let effect = update(
            &mut app,
            Action::ResponseChunk {
                text: "Response ".to_string(),
                item_id: None,
            },
        );

        assert_eq!(app.context.items.len(), 2); // System + Model (new)
        assert!(
            matches!(&app.context.items[1], ContextItem::Message(seg) if seg.content == "Response " && seg.source == Source::Model)
        );
        assert!(app.is_loading);
        assert_eq!(app.status_message, "Receiving...");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_response_done_stops_loading() {
        let mut app = test_app();
        app.is_loading = true;

        let effect = update(&mut app, Action::ResponseDone(None));

        assert!(!app.is_loading);
        assert_eq!(app.status_message, "Response complete.");
        assert_eq!(effect, Effect::SaveSession);
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
        app.had_tool_calls = true;
        app.stream_done = true; // stream already finished
        app.pending_tool_calls.insert("call_1".to_string());

        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: r#"{"temp": 72}"#.to_string(),
            },
        );

        assert!(app.pending_tool_calls.is_empty());
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_tool_result_ready_with_remaining_tools_renders() {
        let mut app = test_app();
        app.is_loading = true;
        app.had_tool_calls = true;
        app.stream_done = true;
        app.pending_tool_calls.insert("call_1".to_string());
        app.pending_tool_calls.insert("call_2".to_string());

        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: "done".to_string(),
            },
        );

        assert_eq!(app.pending_tool_calls.len(), 1);
        assert_eq!(effect, Effect::Render);
        assert!(app.status_message.contains("1 more"));
    }

    #[test]
    fn test_tool_call_with_empty_call_id_is_skipped() {
        let mut app = test_app();
        app.is_loading = true;
        let tc = ToolCall {
            id: "fc_1".into(),
            call_id: String::new(),
            name: "add".into(),
            arguments: "{}".into(),
        };
        let effect = update(&mut app, Action::ToolCallReceived(tc));
        assert_eq!(effect, Effect::Render);
        assert!(app.pending_tool_calls.is_empty());
    }

    #[test]
    fn test_agentic_loop_bound_enforced() {
        use crate::core::state::MAX_AGENTIC_ROUNDS;

        let mut app = test_app();
        app.is_loading = true;
        app.had_tool_calls = true;
        app.stream_done = true;
        app.agentic_rounds = MAX_AGENTIC_ROUNDS;
        app.pending_tool_calls.insert("call_1".to_string());

        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: r#"{"result": 42}"#.to_string(),
            },
        );

        assert_eq!(effect, Effect::Render);
        assert!(!app.is_loading);
        assert!(app.error.is_some());
        assert!(app.error.as_ref().unwrap().contains("loop"));
    }

    #[test]
    fn test_agentic_rounds_reset_on_submit() {
        let mut app = test_app();
        app.agentic_rounds = 5;

        update(&mut app, Action::Submit("hello".to_string()));

        assert_eq!(app.agentic_rounds, 0);
    }

    #[test]
    fn test_response_done_stays_loading_when_tools_pending() {
        let mut app = test_app();
        app.is_loading = true;
        app.had_tool_calls = true;
        app.pending_tool_calls.insert("call_1".to_string());

        let effect = update(&mut app, Action::ResponseDone(None));

        assert!(app.is_loading); // Still loading — tools not done yet
        assert!(app.stream_done);
        assert_eq!(effect, Effect::Render);
    }

    /// Regression test: tool result arrives BEFORE the stream finishes sending
    /// all tool calls. Previously this would fire SpawnRequest prematurely.
    #[test]
    fn test_tool_result_before_stream_done_does_not_spawn() {
        let mut app = test_app();
        app.is_loading = true;

        // Stream sends first tool call
        let tc1 = make_tool_call("add", "call_1");
        let effect = update(&mut app, Action::ToolCallReceived(tc1));
        assert!(matches!(effect, Effect::ExecuteTool(_)));
        assert!(app.had_tool_calls);

        // Tool executes fast and returns before stream is done
        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: r#"{"result": 3}"#.to_string(),
            },
        );
        // Should NOT fire SpawnRequest — stream_done is still false
        assert_eq!(effect, Effect::Render);
        assert!(app.is_loading);

        // Stream sends second tool call
        let tc2 = make_tool_call("subtract", "call_2");
        let effect = update(&mut app, Action::ToolCallReceived(tc2));
        assert!(matches!(effect, Effect::ExecuteTool(_)));

        // Stream finishes — but call_2 is still pending
        let effect = update(&mut app, Action::ResponseDone(None));
        assert_eq!(effect, Effect::Render);
        assert!(app.is_loading);

        // Second tool completes — NOW we can spawn
        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_2".to_string(),
                output: r#"{"result": 1}"#.to_string(),
            },
        );
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_submit_resets_stream_flags() {
        let mut app = test_app();
        app.stream_done = true;
        app.had_tool_calls = true;

        update(&mut app, Action::Submit("hello".to_string()));

        assert!(!app.stream_done);
        assert!(!app.had_tool_calls);
        assert_eq!(app.agentic_rounds, 0);
    }

    #[test]
    fn test_response_done_with_stats_updates_status() {
        let mut app = test_app();
        app.is_loading = true;

        let stats = UsageStats {
            input_tokens: Some(100),
            output_tokens: Some(30),
            ttft_ms: Some(250),
            generation_duration_ms: Some(1000),
            tokens_per_sec: Some(30.0),
            ..Default::default()
        };
        let effect = update(&mut app, Action::ResponseDone(Some(stats)));

        assert!(!app.is_loading);
        assert!(app.status_message.contains("100 in"));
        assert!(app.status_message.contains("30 out"));
        assert!(app.status_message.contains("TTFT 250ms"));
        assert_eq!(effect, Effect::SaveSession);
    }

    #[test]
    fn test_submit_resets_usage_stats() {
        let mut app = test_app();
        app.usage_stats.input_tokens = Some(500);
        app.usage_stats.output_tokens = Some(100);

        update(&mut app, Action::Submit("hello".to_string()));

        assert!(app.usage_stats.input_tokens.is_none());
        assert!(app.usage_stats.output_tokens.is_none());
    }

    #[test]
    fn test_stats_accumulate_across_agentic_rounds() {
        let mut app = test_app();
        app.is_loading = true;

        // Round 1: tool-calling round
        let tc = make_tool_call("add", "call_1");
        update(&mut app, Action::ToolCallReceived(tc));

        let round1_stats = UsageStats {
            input_tokens: Some(100),
            output_tokens: Some(20),
            ttft_ms: Some(300),
            generation_duration_ms: Some(800),
            ..Default::default()
        };
        update(&mut app, Action::ResponseDone(Some(round1_stats)));

        // Complete the tool
        update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: "42".to_string(),
            },
        );

        // Round 2: pure text
        let round2_stats = UsageStats {
            input_tokens: Some(150),
            output_tokens: Some(30),
            ttft_ms: Some(200),
            generation_duration_ms: Some(1200),
            ..Default::default()
        };
        update(&mut app, Action::ResponseDone(Some(round2_stats)));

        // Tokens should be summed
        assert_eq!(app.usage_stats.input_tokens, Some(250));
        assert_eq!(app.usage_stats.output_tokens, Some(50));
        // First TTFT preserved
        assert_eq!(app.usage_stats.ttft_ms, Some(300));
        // Durations summed
        assert_eq!(app.usage_stats.generation_duration_ms, Some(2000));
        // Status should show the summary
        assert!(app.status_message.contains("250 in"));
    }
}
