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

use crate::core::config::ModelEntry;
use crate::core::session::SessionData;
use crate::core::state::{ActiveModel, App, SessionState};
use crate::core::tools::ToolPermission;
use crate::inference::{ToolCall, ToolResult, UsageStats};
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
    // Cycle to next reasoning effort level
    CycleEffort,
    // Switch to a different model/provider
    SwitchModel(ActiveModel),
    // Replace context with a loaded session
    LoadSession(SessionData),
    // Reset to a fresh conversation with the given title
    NewSession {
        title: String,
    },
    // Session was renamed on disk — update domain state if it's the active session
    SessionRenamed {
        id: String,
        new_title: String,
    },
    // Session was deleted on disk - clear active session if it matches
    SessionDeleted(String),
    // User approved a tool in the approval queue
    ToolApproved(String),
    // User denied a tool in the approval queue
    ToolDenied(String),
    // Dynamic models fetched from provider APIs (handled by TUI, not core)
    ModelsFetched(Vec<ModelEntry>),
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Render,
    Quit,
    SpawnRequest,
    ExecuteTool(ToolCall), // Run a tool asynchronously
    PromptToolApproval,    // Show tool approval modal to the user
    SaveSession,           // Persist current session to disk
    SwitchProvider,        // Reconstruct the provider after model switch
}

/// Checks whether the current agentic round is fully complete (stream finished
/// AND all tool results received). Called by both `ResponseDone` and `ToolResultReady`.
///
/// This two-sided gate prevents the race where a fast tool result arrives before
/// the stream has finished sending all tool calls, which would prematurely fire
/// `SpawnRequest` and re-enter the agentic loop with incomplete context.
fn check_round_complete(app_state: &mut App) -> Effect {
    let s = &mut app_state.session;
    if s.stream_done && s.pending_tool_calls.is_empty() {
        if s.had_tool_calls {
            // Tool-calling round complete — advance the agentic loop
            s.agentic_rounds += 1;
            let max_rounds = app_state.max_agentic_rounds;
            if s.agentic_rounds > max_rounds {
                warn!("Agentic loop limit reached ({} rounds)", max_rounds);
                s.is_loading = false;
                s.error = Some(format!(
                    "Agentic loop stopped after {} rounds. The model may be stuck in a tool-calling loop.",
                    max_rounds
                ));
                s.status_message = String::from("Loop limit reached.");
                s.stream_done = false;
                s.had_tool_calls = false;
                Effect::Render
            } else {
                s.status_message = String::from("Resuming...");
                s.stream_done = false;
                s.had_tool_calls = false;
                Effect::SpawnRequest
            }
        } else {
            // Pure text response — no tools were called
            s.is_loading = false;
            s.status_message = s.usage_stats.display_summary();
            Effect::SaveSession
        }
    } else if !s.pending_tool_calls.is_empty() {
        s.status_message = format!("Waiting for {} more tool(s)...", s.pending_tool_calls.len());
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
            if message.is_empty() || app_state.session.is_loading {
                return Effect::None; // noop on empty input or if already loading
            }
            let s = &mut app_state.session;
            s.context.add_user_message(message);
            s.is_loading = true;
            s.agentic_rounds = 0;
            s.stream_done = false;
            s.had_tool_calls = false;
            s.usage_stats = UsageStats::default();
            s.message_stats.clear();
            s.status_message = String::from("Loading...");
            Effect::SpawnRequest
        }
        Action::ResponseChunk { text, item_id } => {
            app_state
                .session
                .context
                .append_to_last_model_message(&text, item_id.as_deref());
            // Log total message length after append
            if let Some(crate::inference::ContextItem::Message(last)) =
                app_state.session.context.items.last()
            {
                debug!(
                    "ResponseChunk applied: chunk_len={}, total_msg_len={}",
                    text.len(),
                    last.content.len()
                );
            }
            app_state.session.status_message = String::from("Receiving...");
            Effect::Render
        }
        Action::ThinkingChunk { text, item_id } => {
            app_state
                .session
                .context
                .append_to_last_thinking_message(&text, item_id.as_deref());
            debug!("ThinkingChunk applied: chunk_len={}", text.len());
            app_state.session.status_message = String::from("Thinking...");
            Effect::Render
        }
        Action::ResponseDone(stats) => {
            app_state.session.context.clear_active_streams();
            app_state.session.stream_done = true;
            if let Some(round_stats) = stats {
                app_state.session.usage_stats.accumulate(&round_stats);
                // Accumulate into session-level running total
                if let Some(tokens) = round_stats.total_tokens {
                    app_state.session.session_total_tokens += tokens;
                }
                // Store per-message stats on the last Model message
                if let Some(idx) = app_state
                    .session
                    .context
                    .items
                    .iter()
                    .rposition(|item| {
                        matches!(item, crate::inference::ContextItem::Message(seg) if seg.source == crate::inference::Source::Model)
                    })
                {
                    app_state.session.message_stats.insert(idx, round_stats);
                }
            }
            if let Some(crate::inference::ContextItem::Message(last)) =
                app_state.session.context.items.last()
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
            let s = &mut app_state.session;
            s.had_tool_calls = true;
            s.pending_tool_calls.insert(tool_call.call_id.clone());
            s.context.add_tool_call(tool_call.clone());

            // Check permission: Safe tools execute immediately, Prompt tools queue for approval
            let permission = app_state
                .registry
                .permission(&tool_call.name)
                .unwrap_or(ToolPermission::Prompt);

            match permission {
                ToolPermission::Safe => {
                    s.status_message = format!("Calling: {}...", tool_call.name);
                    Effect::ExecuteTool(tool_call)
                }
                ToolPermission::Prompt => {
                    s.status_message = format!("Approval needed: {}", tool_call.name);
                    s.approval_queue.push_back(tool_call);
                    Effect::PromptToolApproval
                }
            }
        }
        Action::ToolResultReady { call_id, output } => {
            app_state.session.pending_tool_calls.remove(&call_id);
            app_state
                .session
                .context
                .add_tool_result(ToolResult { call_id, output });
            check_round_complete(app_state)
        }
        Action::CancelGeneration => {
            let s = &mut app_state.session;
            s.is_loading = false;
            s.pending_tool_calls.clear();
            s.approval_queue.clear();
            s.stream_done = false;
            s.had_tool_calls = false;
            s.usage_stats = UsageStats::default();
            s.context.clear_active_streams();
            s.status_message = String::from("Cancelled.");
            Effect::Render
        }
        Action::LoadSession(data) => {
            let mut session = SessionState::new(&app_state.system_prompt);
            for item in data.items {
                session.context.items.push(item);
            }
            session.current_session_id = Some(data.meta.id);
            session.session_title = data.meta.title.clone();
            session.status_message = format!("Loaded: {}", data.meta.title);
            let loaded_model = ActiveModel::new(data.meta.model_name, data.meta.provider_name);
            let provider_changed =
                !loaded_model.provider.is_empty() && loaded_model.provider != app_state.model.provider;
            if !loaded_model.provider.is_empty() {
                app_state.model = loaded_model;
            } else {
                // Legacy session without provider - update name only
                app_state.model.name = loaded_model.name;
            }
            app_state.session = session;
            if provider_changed {
                Effect::SwitchProvider
            } else {
                Effect::Render
            }
        }
        Action::NewSession { title } => {
            app_state.session = SessionState::new(&app_state.system_prompt);
            app_state.session.session_title = title;
            app_state.session.status_message = String::from("New session.");
            Effect::Render
        }
        Action::SessionRenamed { id, new_title } => {
            if app_state.session.current_session_id.as_deref() == Some(&id) {
                app_state.session.session_title = new_title;
            }
            Effect::Render
        }
        Action::SessionDeleted(id) => {
            if app_state.session.current_session_id.as_deref() == Some(&id) {
                app_state.session = SessionState::new(&app_state.system_prompt);
                app_state.session.status_message = String::from("Session deleted.");
            }
            Effect::Render
        }
        Action::SwitchModel(model) => {
            app_state.session.status_message =
                format!("Switched to {} ({})", model.name, model.provider);
            app_state.model = model;
            Effect::SwitchProvider
        }
        Action::CycleEffort => {
            app_state.effort = app_state.effort.next();
            app_state.session.status_message = format!("Reasoning: {}", app_state.effort.label());
            Effect::Render
        }
        Action::ToolApproved(call_id) => {
            let s = &mut app_state.session;
            if let Some(pos) = s.approval_queue.iter().position(|tc| tc.call_id == call_id) {
                let tool_call = s.approval_queue.remove(pos).unwrap();
                s.status_message = format!("Calling: {}...", tool_call.name);
                Effect::ExecuteTool(tool_call)
            } else {
                warn!("ToolApproved for unknown call_id: {}", call_id);
                Effect::Render
            }
        }
        Action::ToolDenied(call_id) => {
            let s = &mut app_state.session;
            if let Some(pos) = s.approval_queue.iter().position(|tc| tc.call_id == call_id) {
                let tool_call = s.approval_queue.remove(pos).unwrap();
                s.pending_tool_calls.remove(&call_id);
                s.context.add_tool_result(ToolResult {
                    call_id: call_id.clone(),
                    output: serde_json::json!({"error": format!("Tool '{}' was denied by the user.", tool_call.name)}).to_string(),
                });
                check_round_complete(app_state)
            } else {
                warn!("ToolDenied for unknown call_id: {}", call_id);
                Effect::Render
            }
        }
        // ModelsFetched carries TUI-only state (picker list). The TUI event loop
        // intercepts this action before it reaches update(). This no-op handler
        // exists as a defensive fallthrough — if the TUI intercept is ever removed,
        // core silently ignores it rather than panicking on an unhandled variant.
        Action::ModelsFetched(_) => Effect::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{ContextItem, Effort, Source};
    use crate::test_support::{test_app, test_app_with_prompt_tool};

    #[test]
    fn test_quit_returns_quit_effect() {
        let mut app = test_app();

        let effect = update(&mut app, Action::Quit);

        assert_eq!(effect, Effect::Quit);
    }

    #[test]
    fn test_submit_noop_on_empty_message() {
        let mut app = test_app();
        let initial_context_len = app.session.context.items.len();

        let effect = update(&mut app, Action::Submit(String::new()));

        assert_eq!(app.session.context.items.len(), initial_context_len);
        assert!(!app.session.is_loading);
        assert_eq!(effect, Effect::None);
    }

    #[test]
    fn test_submit_adds_message_and_triggers_request() {
        let mut app = test_app();

        let effect = update(&mut app, Action::Submit("Hello, model!".to_string()));

        assert_eq!(app.session.context.items.len(), 2); // System + User
        assert!(
            matches!(&app.session.context.items[1], ContextItem::Message(seg) if seg.content == "Hello, model!")
        );
        assert!(app.session.is_loading);
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_response_chunk_appends_and_updates_status() {
        let mut app = test_app();
        app.session.is_loading = true;

        let effect = update(
            &mut app,
            Action::ResponseChunk {
                text: "Response ".to_string(),
                item_id: None,
            },
        );

        assert_eq!(app.session.context.items.len(), 2); // System + Model (new)
        assert!(
            matches!(&app.session.context.items[1], ContextItem::Message(seg) if seg.content == "Response " && seg.source == Source::Model)
        );
        assert!(app.session.is_loading);
        assert_eq!(app.session.status_message, "Receiving...");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_response_done_stops_loading() {
        let mut app = test_app();
        app.session.is_loading = true;

        let effect = update(&mut app, Action::ResponseDone(None));

        assert!(!app.session.is_loading);
        assert_eq!(app.session.status_message, "Response complete.");
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
        app.session.is_loading = true;
        let tc = make_tool_call("math_operation", "call_1");

        let effect = update(&mut app, Action::ToolCallReceived(tc.clone()));

        assert!(app.session.pending_tool_calls.contains("call_1"));
        assert!(matches!(effect, Effect::ExecuteTool(ref t) if t.call_id == "call_1"));
        assert!(app.session.status_message.contains("math_operation"));
    }

    #[test]
    fn test_tool_result_ready_last_tool_spawns_request() {
        let mut app = test_app();
        app.session.is_loading = true;
        app.session.had_tool_calls = true;
        app.session.stream_done = true; // stream already finished
        app.session.pending_tool_calls.insert("call_1".to_string());

        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: r#"{"temp": 72}"#.to_string(),
            },
        );

        assert!(app.session.pending_tool_calls.is_empty());
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_tool_result_ready_with_remaining_tools_renders() {
        let mut app = test_app();
        app.session.is_loading = true;
        app.session.had_tool_calls = true;
        app.session.stream_done = true;
        app.session.pending_tool_calls.insert("call_1".to_string());
        app.session.pending_tool_calls.insert("call_2".to_string());

        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: "done".to_string(),
            },
        );

        assert_eq!(app.session.pending_tool_calls.len(), 1);
        assert_eq!(effect, Effect::Render);
        assert!(app.session.status_message.contains("1 more"));
    }

    #[test]
    fn test_tool_call_with_empty_call_id_is_skipped() {
        let mut app = test_app();
        app.session.is_loading = true;
        let tc = ToolCall {
            id: "fc_1".into(),
            call_id: String::new(),
            name: "add".into(),
            arguments: "{}".into(),
        };
        let effect = update(&mut app, Action::ToolCallReceived(tc));
        assert_eq!(effect, Effect::Render);
        assert!(app.session.pending_tool_calls.is_empty());
    }

    #[test]
    fn test_agentic_loop_bound_enforced() {
        let mut app = test_app();
        app.session.is_loading = true;
        app.session.had_tool_calls = true;
        app.session.stream_done = true;
        app.session.agentic_rounds = app.max_agentic_rounds;
        app.session.pending_tool_calls.insert("call_1".to_string());

        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: r#"{"result": 42}"#.to_string(),
            },
        );

        assert_eq!(effect, Effect::Render);
        assert!(!app.session.is_loading);
        assert!(app.session.error.is_some());
        assert!(app.session.error.as_ref().unwrap().contains("loop"));
    }

    #[test]
    fn test_agentic_rounds_reset_on_submit() {
        let mut app = test_app();
        app.session.agentic_rounds = 5;

        update(&mut app, Action::Submit("hello".to_string()));

        assert_eq!(app.session.agentic_rounds, 0);
    }

    #[test]
    fn test_response_done_stays_loading_when_tools_pending() {
        let mut app = test_app();
        app.session.is_loading = true;
        app.session.had_tool_calls = true;
        app.session.pending_tool_calls.insert("call_1".to_string());

        let effect = update(&mut app, Action::ResponseDone(None));

        assert!(app.session.is_loading); // Still loading — tools not done yet
        assert!(app.session.stream_done);
        assert_eq!(effect, Effect::Render);
    }

    /// Regression test: tool result arrives BEFORE the stream finishes sending
    /// all tool calls. Previously this would fire SpawnRequest prematurely.
    #[test]
    fn test_tool_result_before_stream_done_does_not_spawn() {
        let mut app = test_app();
        app.session.is_loading = true;

        // Stream sends first tool call (use registered safe tool names)
        let tc1 = make_tool_call("math_operation", "call_1");
        let effect = update(&mut app, Action::ToolCallReceived(tc1));
        assert!(matches!(effect, Effect::ExecuteTool(_)));
        assert!(app.session.had_tool_calls);

        // Tool executes fast and returns before stream is done
        let effect = update(
            &mut app,
            Action::ToolResultReady {
                call_id: "call_1".to_string(),
                output: r#"{"result": 3}"#.to_string(),
            },
        );
        // Should NOT fire SpawnRequest - stream_done is still false
        assert_eq!(effect, Effect::Render);
        assert!(app.session.is_loading);

        // Stream sends second tool call
        let tc2 = make_tool_call("read_file", "call_2");
        let effect = update(&mut app, Action::ToolCallReceived(tc2));
        assert!(matches!(effect, Effect::ExecuteTool(_)));

        // Stream finishes — but call_2 is still pending
        let effect = update(&mut app, Action::ResponseDone(None));
        assert_eq!(effect, Effect::Render);
        assert!(app.session.is_loading);

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
        app.session.stream_done = true;
        app.session.had_tool_calls = true;

        update(&mut app, Action::Submit("hello".to_string()));

        assert!(!app.session.stream_done);
        assert!(!app.session.had_tool_calls);
        assert_eq!(app.session.agentic_rounds, 0);
    }

    #[test]
    fn test_response_done_with_stats_updates_status() {
        let mut app = test_app();
        app.session.is_loading = true;

        let stats = UsageStats {
            input_tokens: Some(100),
            output_tokens: Some(30),
            ttft_ms: Some(250),
            generation_duration_ms: Some(1000),
            tokens_per_sec: Some(30.0),
            ..Default::default()
        };
        let effect = update(&mut app, Action::ResponseDone(Some(stats)));

        assert!(!app.session.is_loading);
        assert!(app.session.status_message.contains("100 in"));
        assert!(app.session.status_message.contains("30 out"));
        assert!(app.session.status_message.contains("TTFT 250ms"));
        assert_eq!(effect, Effect::SaveSession);
    }

    #[test]
    fn test_new_session_sets_title() {
        let mut app = test_app();
        app.session.context.add_user_message("hello".to_string());
        app.session.current_session_id = Some("old-id".to_string());

        let effect = update(
            &mut app,
            Action::NewSession {
                title: "Session #5".to_string(),
            },
        );

        assert_eq!(app.session.session_title, "Session #5");
        assert!(app.session.current_session_id.is_none());
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_session_renamed_updates_active_title() {
        let mut app = test_app();
        app.session.current_session_id = Some("sess-1".to_string());
        app.session.session_title = "Old Title".to_string();

        let effect = update(
            &mut app,
            Action::SessionRenamed {
                id: "sess-1".to_string(),
                new_title: "New Title".to_string(),
            },
        );

        assert_eq!(app.session.session_title, "New Title");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_session_renamed_ignores_different_session() {
        let mut app = test_app();
        app.session.current_session_id = Some("sess-1".to_string());
        app.session.session_title = "My Session".to_string();

        update(
            &mut app,
            Action::SessionRenamed {
                id: "sess-other".to_string(),
                new_title: "Other Title".to_string(),
            },
        );

        assert_eq!(app.session.session_title, "My Session");
    }

    #[test]
    fn test_session_deleted_resets_context_when_active() {
        let mut app = test_app();
        app.session.current_session_id = Some("sess-1".to_string());
        app.session.context.add_user_message("hello".to_string());
        app.session.is_loading = true;
        app.session.session_title = "My Session".to_string();

        let effect = update(&mut app, Action::SessionDeleted("sess-1".to_string()));

        // Full reset: context cleared, session ID gone, not loading
        assert!(app.session.current_session_id.is_none());
        assert!(!app.session.is_loading);
        assert!(app.session.session_title.is_empty());
        // Only the system directive should remain
        assert_eq!(app.session.context.items.len(), 1);
        assert!(matches!(
            &app.session.context.items[0],
            ContextItem::Message(seg) if seg.source == Source::Directive
        ));
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_session_deleted_ignores_different_session() {
        let mut app = test_app();
        app.session.current_session_id = Some("sess-1".to_string());

        update(&mut app, Action::SessionDeleted("sess-other".to_string()));

        assert_eq!(app.session.current_session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn test_switch_model_updates_fields() {
        let mut app = test_app();

        let effect = update(
            &mut app,
            Action::SwitchModel(ActiveModel::new("gpt-4", "openrouter")),
        );

        assert_eq!(app.model.name, "gpt-4");
        assert_eq!(app.model.provider, "openrouter");
        assert!(app.session.status_message.contains("gpt-4"));
        assert_eq!(effect, Effect::SwitchProvider);
    }

    fn make_session_data(model_name: &str, provider_name: &str) -> crate::core::session::SessionData {
        use crate::core::session::{SessionData, SessionMeta};
        SessionData {
            meta: SessionMeta {
                id: "sess-1".to_string(),
                title: "Test Session".to_string(),
                created_at: 0,
                updated_at: 0,
                message_count: 1,
                model_name: model_name.to_string(),
                provider_name: provider_name.to_string(),
            },
            items: vec![ContextItem::Message(crate::inference::ContextSegment {
                source: Source::User,
                content: "hello".to_string(),
            })],
        }
    }

    #[test]
    fn test_load_session_restores_model_and_provider() {
        let mut app = test_app();
        app.model = ActiveModel::new("old-model", "openrouter");

        let data = make_session_data("saved-model", "lmstudio");
        let effect = update(&mut app, Action::LoadSession(data));

        assert_eq!(app.model.name, "saved-model");
        assert_eq!(app.model.provider, "lmstudio");
        assert_eq!(effect, Effect::SwitchProvider);
    }

    #[test]
    fn test_load_session_same_provider_returns_render() {
        let mut app = test_app();
        app.model = ActiveModel::new("old-model", "openrouter");

        let data = make_session_data("new-model", "openrouter");
        let effect = update(&mut app, Action::LoadSession(data));

        assert_eq!(app.model.name, "new-model");
        assert_eq!(app.model.provider, "openrouter");
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_load_session_legacy_without_provider() {
        let mut app = test_app();
        app.model = ActiveModel::new("old-model", "openrouter");

        // Legacy session: provider_name is empty
        let data = make_session_data("legacy-model", "");
        let effect = update(&mut app, Action::LoadSession(data));

        assert_eq!(app.model.name, "legacy-model");
        assert_eq!(app.model.provider, "openrouter"); // preserved
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_cycle_effort_advances_and_updates_status() {
        let mut app = test_app();
        assert_eq!(app.effort, Effort::default()); // Auto

        let effect = update(&mut app, Action::CycleEffort);

        assert_eq!(app.effort, Effort::Low); // Auto -> Low
        assert!(app.session.status_message.contains("Low"));
        assert_eq!(effect, Effect::Render);
    }

    #[test]
    fn test_submit_resets_usage_stats() {
        let mut app = test_app();
        app.session.usage_stats.input_tokens = Some(500);
        app.session.usage_stats.output_tokens = Some(100);

        update(&mut app, Action::Submit("hello".to_string()));

        assert!(app.session.usage_stats.input_tokens.is_none());
        assert!(app.session.usage_stats.output_tokens.is_none());
    }

    #[test]
    fn test_stats_accumulate_across_agentic_rounds() {
        let mut app = test_app();
        app.session.is_loading = true;

        // Round 1: tool-calling round
        let tc = make_tool_call("math_operation", "call_1");
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
        assert_eq!(app.session.usage_stats.input_tokens, Some(250));
        assert_eq!(app.session.usage_stats.output_tokens, Some(50));
        // First TTFT preserved
        assert_eq!(app.session.usage_stats.ttft_ms, Some(300));
        // Durations summed
        assert_eq!(app.session.usage_stats.generation_duration_ms, Some(2000));
        // Status should show the summary
        assert!(app.session.status_message.contains("250 in"));
    }

    // --- Permission system tests ---

    #[test]
    fn test_safe_tool_executes_immediately() {
        let mut app = test_app_with_prompt_tool();
        app.session.is_loading = true;
        let tc = make_tool_call("math_operation", "call_1");

        let effect = update(&mut app, Action::ToolCallReceived(tc));

        assert!(matches!(effect, Effect::ExecuteTool(ref t) if t.call_id == "call_1"));
        assert!(app.session.approval_queue.is_empty());
    }

    #[test]
    fn test_prompt_tool_queues_for_approval() {
        let mut app = test_app_with_prompt_tool();
        app.session.is_loading = true;
        let tc = make_tool_call("stub_prompt", "call_1");

        let effect = update(&mut app, Action::ToolCallReceived(tc));

        assert_eq!(effect, Effect::PromptToolApproval);
        assert_eq!(app.session.approval_queue.len(), 1);
        assert_eq!(app.session.approval_queue[0].call_id, "call_1");
        assert!(app.session.pending_tool_calls.contains("call_1"));
    }

    #[test]
    fn test_tool_approved_returns_execute_effect() {
        let mut app = test_app_with_prompt_tool();
        app.session.is_loading = true;
        let tc = make_tool_call("stub_prompt", "call_1");
        update(&mut app, Action::ToolCallReceived(tc));

        let effect = update(&mut app, Action::ToolApproved("call_1".to_string()));

        assert!(matches!(effect, Effect::ExecuteTool(ref t) if t.call_id == "call_1"));
        assert!(app.session.approval_queue.is_empty());
    }

    #[test]
    fn test_tool_denied_synthesizes_error_result() {
        let mut app = test_app_with_prompt_tool();
        app.session.is_loading = true;
        app.session.stream_done = true;
        let tc = make_tool_call("stub_prompt", "call_1");
        update(&mut app, Action::ToolCallReceived(tc));

        let effect = update(&mut app, Action::ToolDenied("call_1".to_string()));

        assert!(app.session.approval_queue.is_empty());
        assert!(!app.session.pending_tool_calls.contains("call_1"));

        let has_error_result = app.session.context.items.iter().any(|item| {
            matches!(item, ContextItem::ToolResult(tr) if tr.call_id == "call_1" && tr.output.contains("denied"))
        });
        assert!(has_error_result);

        // Stream done + no pending = should trigger round check
        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_multiple_prompt_tools_queue_correctly() {
        let mut app = test_app_with_prompt_tool();
        app.session.is_loading = true;

        let tc1 = make_tool_call("stub_prompt", "call_1");
        let tc2 = make_tool_call("stub_prompt", "call_2");

        update(&mut app, Action::ToolCallReceived(tc1));
        update(&mut app, Action::ToolCallReceived(tc2));

        assert_eq!(app.session.approval_queue.len(), 2);
        assert_eq!(app.session.pending_tool_calls.len(), 2);
    }

    #[test]
    fn test_denied_tool_triggers_round_check() {
        let mut app = test_app_with_prompt_tool();
        app.session.is_loading = true;
        app.session.had_tool_calls = true;
        app.session.stream_done = true;

        let tc = make_tool_call("stub_prompt", "call_1");
        update(&mut app, Action::ToolCallReceived(tc));
        let effect = update(&mut app, Action::ToolDenied("call_1".to_string()));

        assert_eq!(effect, Effect::SpawnRequest);
    }

    #[test]
    fn test_unknown_tool_defaults_to_prompt() {
        let mut app = test_app_with_prompt_tool();
        app.session.is_loading = true;
        let tc = make_tool_call("nonexistent_tool", "call_1");

        let effect = update(&mut app, Action::ToolCallReceived(tc));

        assert_eq!(effect, Effect::PromptToolApproval);
        assert_eq!(app.session.approval_queue.len(), 1);
    }
}
