//! # TUI Adapter
//!
//! The ratatui-specific layer. Handles terminal I/O, renders the UI,
//! and translates keyboard events into core::Action values.
//!
//! This is the only module that knows about ratatui and crossterm.
//! The intention is to swap this out for a different adapter (React, web, etc.) in the future
//! if needed.
//!
//! ## Redraw Strategy
//!
//! The event loop uses conditional redraw to avoid unnecessary work:
//!
//! - **Animating** (landing page, loading): draws every ~80ms for smooth animation.
//! - **Idle** (conversation, no input): sleeps up to 500ms, only redraws on events
//!   or terminal resize. Animation math is also skipped.
//!
//! A `SteadyBlock` cursor style is used instead of a blinking cursor because
//! ratatui's `set_cursor_position` resets the terminal's blink timer on every
//! `draw()` call, making blinking cursors appear erratic during continuous redraws.

mod component;
mod components;
mod event;
pub mod markdown;
mod ui;

use log::{debug, info, warn};
use std::io::stdout;
use std::sync::{Arc, mpsc};

use crossterm::cursor::{Hide, SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;

use crate::core::action::{Action, Effect, update};
use crate::core::config::ResolvedConfig;
use crate::core::session;
use crate::core::state::App;
use crate::inference::Effort;
use crate::inference::{
    CompletionProvider, CompletionRequest, ContextItem, LmStudioProvider, OpenRouterProvider,
    StreamChunk,
};
use crate::tui::component::EventHandler;
use crate::tui::components::{InputBox, InputEvent, MessageListState, SessionManagerState};
use crate::tui::components::session_manager::SessionEvent;
use crate::tui::event::{TuiEvent, poll_event_immediate, poll_event_timeout};

/// Modal input mode: determines how keyboard events are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Navigate messages with arrow keys. Typing auto-switches to Input.
    Cursor,
    /// Text editing in the input box. Esc switches to Cursor.
    Input,
}

/// TUI-specific presentation state (not part of core business logic)
pub struct TuiState {
    // Persistent component states
    pub message_list: MessageListState,
    pub input_box: InputBox,
    // Modal input mode
    pub input_mode: InputMode,
    // Animation state
    pub pulse_value: f32,
    // Session manager overlay (None = hidden)
    pub session_manager: Option<SessionManagerState>,
}

impl TuiState {
    pub fn new(initial_effort: Effort) -> Self {
        Self {
            message_list: MessageListState::new(),
            input_box: InputBox::new(initial_effort),
            input_mode: InputMode::Input, // User expects to type immediately
            pulse_value: 0.0,
            session_manager: None,
        }
    }
}

struct TerminalModeGuard;

impl TerminalModeGuard {
    fn new() -> std::io::Result<Self> {
        // Enable Kitty keyboard protocol unconditionally (allows Shift+Enter detection)
        // Detection via supports_keyboard_enhancement() fails in WSL, but the protocol
        // is harmlessly ignored by terminals that don't support it
        execute!(
            stdout(),
            EnableMouseCapture,
            EnableBracketedPaste,
            Show,                        // Show cursor for input editing
            SetCursorStyle::SteadyBlock, // Non-blinking: avoids blink timer reset from continuous redraws
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
        info!(
            "Terminal modes enabled (mouse, bracketed paste, steady block cursor, keyboard enhancement)"
        );
        Ok(Self)
    }
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        let _ = execute!(
            stdout(),
            PopKeyboardEnhancementFlags,
            DisableMouseCapture,
            DisableBracketedPaste,
            Hide // Hide cursor on exit
        );
    }
}

/// Build a provider from a resolved config's provider name and credentials.
pub fn build_provider(config: &ResolvedConfig) -> Arc<dyn CompletionProvider> {
    match config.provider.as_str() {
        "lmstudio" => Arc::new(LmStudioProvider::new(config.lmstudio_base_url.clone())),
        _ => {
            // Default to openrouter
            let api_key = config
                .openrouter_api_key
                .clone()
                .expect("OpenRouter API key must be set (config file, OPENROUTER_API_KEY env var, or --provider lmstudio)");
            Arc::new(OpenRouterProvider::new(
                api_key,
                Some(config.openrouter_base_url.clone()),
            ))
        }
    }
}

pub fn run(config: ResolvedConfig) -> std::io::Result<()> {
    let provider = build_provider(&config);
    let mut app = App::from_config(provider, &config);
    let mut tui = TuiState::new(app.effort);

    let mut terminal = ratatui::init();
    let _terminal_mode_guard = TerminalModeGuard::new();

    // Channel for actions from background tasks
    let (tx, rx) = mpsc::channel();

    // Abort handles for the current generation (used by Escape-to-cancel)
    let mut active_abort_handles: Vec<tokio::task::AbortHandle> = Vec::new();

    // Animation timer
    let start_time = std::time::Instant::now();
    let mut needs_redraw = true; // Force first frame

    loop {
        // Sync InputBox props with App/TUI state
        tui.input_box.effort = app.effort;
        tui.input_box.dimmed = matches!(tui.input_mode, InputMode::Cursor);

        // Determine if animations are running (landing page or loading spinner)
        let has_visible_messages = app.context.items.iter().any(|item|
            matches!(item, crate::inference::ContextItem::Message(seg) if matches!(seg.source, crate::inference::Source::User | crate::inference::Source::Model))
        );
        let animating = app.is_loading || !has_visible_messages;

        if animating {
            needs_redraw = true;
        }

        // Only draw when something changed
        if needs_redraw {
            let elapsed = start_time.elapsed().as_secs_f32();
            tui.pulse_value = (elapsed * 5.0).sin() * 0.5 + 0.5;
            let spinner_frame = (elapsed * 12.0) as usize;
            terminal.draw(|f| ui::draw_ui(f, &app, &mut tui, spinner_frame))?;
            needs_redraw = false;
        }

        // Dynamic poll timeout: short when animating (~12fps), long when idle
        let timeout = if animating {
            std::time::Duration::from_millis(80)
        } else {
            std::time::Duration::from_millis(500)
        };
        let first_event = poll_event_timeout(timeout);

        // Process first event + drain ALL pending events before next draw
        let mut should_quit = false;
        if first_event.is_some() {
            needs_redraw = true;
        }
        for event in first_event
            .into_iter()
            .chain(std::iter::from_fn(poll_event_immediate))
        {
            // Resize just needs a redraw (already flagged above)
            if matches!(event, TuiEvent::Resize) {
                continue;
            }

            // ForceQuit (Ctrl+C) always quits regardless of mode
            if matches!(event, TuiEvent::ForceQuit) {
                let effect = update(&mut app, Action::Quit);
                if effect == Effect::Quit {
                    should_quit = true;
                }
                continue;
            }

            // Ctrl+O opens session manager
            if matches!(event, TuiEvent::OpenSessionManager) {
                let index = session::load_index().unwrap_or_default();
                tui.session_manager = Some(SessionManagerState::new(index.sessions));
                continue;
            }

            // When session manager is open, route all events to it
            if let Some(ref mut sm) = tui.session_manager {
                if let Some(session_event) = sm.handle_event(&event) {
                    match session_event {
                        SessionEvent::Load(id) => {
                            match session::load_session(&id) {
                                Ok(data) => {
                                    let effect = update(&mut app, Action::LoadSession(data));
                                    if effect == Effect::Quit {
                                        should_quit = true;
                                    }
                                    tui.message_list = MessageListState::new();
                                }
                                Err(e) => {
                                    warn!("Failed to load session {}: {}", id, e);
                                    app.status_message = format!("Load failed: {}", e);
                                }
                            }
                            tui.session_manager = None;
                        }
                        SessionEvent::CreateNew => {
                            let effect = update(&mut app, Action::NewSession);
                            if effect == Effect::Quit {
                                should_quit = true;
                            }
                            tui.message_list = MessageListState::new();
                            tui.session_manager = None;
                        }
                        SessionEvent::Delete(id) => {
                            if let Err(e) = session::delete_session(&id) {
                                warn!("Failed to delete session {}: {}", id, e);
                            }
                            sm.remove_session(&id);
                            // If we deleted the active session, clear the ID
                            if app.current_session_id.as_deref() == Some(&id) {
                                app.current_session_id = None;
                            }
                        }
                        SessionEvent::Dismiss => {
                            tui.session_manager = None;
                        }
                    }
                }
                continue;
            }

            // Mouse hover — always active regardless of mode
            if let TuiEvent::MouseMove(_col, row) = event {
                let frame_area = terminal.get_frame().area();
                let scroll_offset = tui.message_list.scroll_state.offset().y;
                let input_height = tui.input_box.calculate_height(frame_area.width);

                tui.message_list.selected_index = ui::hit_test_message(
                    row,
                    frame_area,
                    scroll_offset,
                    &tui.message_list.layout.prefix_heights,
                    input_height,
                );
                continue;
            }

            // Mouse click — toggle expansion on tool calls
            if let TuiEvent::MouseClick(_col, row) = event {
                let frame_area = terminal.get_frame().area();
                let scroll_offset = tui.message_list.scroll_state.offset().y;
                let input_height = tui.input_box.calculate_height(frame_area.width);

                let hit = ui::hit_test_message(
                    row,
                    frame_area,
                    scroll_offset,
                    &tui.message_list.layout.prefix_heights,
                    input_height,
                );

                if let Some(idx) = hit {
                    tui.message_list.selected_index = Some(idx);
                    if matches!(app.context.items.get(idx), Some(ContextItem::ToolCall(_)))
                        && !tui.message_list.expanded_indices.remove(&idx)
                    {
                        tui.message_list.expanded_indices.insert(idx);
                    }
                }
                continue;
            }

            // Scroll events — always go to MessageList regardless of mode
            if matches!(
                event,
                TuiEvent::ScrollUp
                    | TuiEvent::ScrollDown
                    | TuiEvent::ScrollPageUp
                    | TuiEvent::ScrollPageDown
            ) {
                tui.message_list.handle_event(&event);
                continue;
            }

            // Modal event dispatch
            match tui.input_mode {
                InputMode::Input => {
                    // Esc while loading → cancel generation
                    if matches!(event, TuiEvent::Escape) && app.is_loading {
                        for handle in active_abort_handles.drain(..) {
                            handle.abort();
                        }
                        let effect = update(&mut app, Action::CancelGeneration);
                        if effect == Effect::Quit {
                            should_quit = true;
                        }
                        continue;
                    }
                    // Esc → switch to Cursor mode
                    if matches!(event, TuiEvent::Escape) {
                        tui.input_mode = InputMode::Cursor;
                        // Select the last non-ToolResult item when entering Cursor mode
                        let items = &app.context.items;
                        let mut idx = items.len();
                        while idx > 0 {
                            idx -= 1;
                            if !matches!(items[idx], ContextItem::ToolResult(_)) {
                                break;
                            }
                        }
                        tui.message_list.selected_index =
                            if !items.is_empty() { Some(idx) } else { None };
                        continue;
                    }

                    // InputBox handles everything else
                    if let Some(input_event) = tui.input_box.handle_event(&event) {
                        match input_event {
                            InputEvent::Submit(text) => {
                                if !app.is_loading {
                                    let effect = update(&mut app, Action::Submit(text));
                                    if effect == Effect::SpawnRequest {
                                        active_abort_handles = spawn_request(&app, tx.clone());
                                    }
                                }
                            }
                            InputEvent::CycleEffort => {
                                app.effort = app.effort.next();
                                app.status_message = format!("Reasoning: {}", app.effort.label());
                            }
                            InputEvent::ContentChanged => {}
                        }
                    }
                }
                InputMode::Cursor => {
                    match event {
                        // Esc while loading → cancel generation
                        TuiEvent::Escape if app.is_loading => {
                            for handle in active_abort_handles.drain(..) {
                                handle.abort();
                            }
                            let effect = update(&mut app, Action::CancelGeneration);
                            if effect == Effect::Quit {
                                should_quit = true;
                            }
                        }
                        // Esc in Cursor mode is a no-op
                        TuiEvent::Escape => {}
                        // Space toggles expansion of selected tool call
                        TuiEvent::InputChar(' ') => {
                            if let Some(idx) = tui.message_list.selected_index
                                && matches!(
                                    app.context.items.get(idx),
                                    Some(ContextItem::ToolCall(_))
                                )
                                && !tui.message_list.expanded_indices.remove(&idx)
                            {
                                tui.message_list.expanded_indices.insert(idx);
                            }
                        }
                        // Typing auto-switches to Input mode and forwards the event
                        TuiEvent::InputChar(_) | TuiEvent::Paste(_) => {
                            tui.input_mode = InputMode::Input;
                            tui.message_list.selected_index = None;
                            tui.input_box.handle_event(&event);
                        }
                        // Enter switches to Input mode
                        TuiEvent::Submit => {
                            tui.input_mode = InputMode::Input;
                            tui.message_list.selected_index = None;
                        }
                        // Up/Down navigate messages (skipping consumed ToolResults)
                        TuiEvent::CursorUp => {
                            let items = &app.context.items;
                            if !items.is_empty() {
                                let mut idx = tui
                                    .message_list
                                    .selected_index
                                    .map(|i| i.saturating_sub(1))
                                    .unwrap_or(items.len() - 1);
                                // Skip backwards past ToolResult items
                                while idx > 0 && matches!(items[idx], ContextItem::ToolResult(_)) {
                                    idx -= 1;
                                }
                                tui.message_list.selected_index = Some(idx);
                                tui.message_list.scroll_to_selected();
                            }
                        }
                        TuiEvent::CursorDown => {
                            let items = &app.context.items;
                            if let Some(mut idx) = tui.message_list.selected_index
                                && idx + 1 < items.len()
                            {
                                idx += 1;
                                // Skip forwards past ToolResult items
                                while idx < items.len()
                                    && matches!(items[idx], ContextItem::ToolResult(_))
                                {
                                    idx += 1;
                                }
                                // Only update if we landed on a valid index
                                if idx < items.len() {
                                    tui.message_list.selected_index = Some(idx);
                                    tui.message_list.scroll_to_selected();
                                }
                            }
                        }
                        // CycleEffort works in both modes
                        TuiEvent::CycleEffort => {
                            app.effort = app.effort.next();
                            app.status_message = format!("Reasoning: {}", app.effort.label());
                        }
                        _ => {}
                    }
                }
            }
        }

        if should_quit {
            break;
        }

        // Handle background task actions (streaming responses)
        while let Ok(action) = rx.try_recv() {
            needs_redraw = true;
            debug!("Event loop received: {:?}", action);
            let effect = update(&mut app, action);
            match effect {
                Effect::Quit => break,
                Effect::SpawnRequest => {
                    active_abort_handles = spawn_request(&app, tx.clone());
                }
                Effect::ExecuteTool(tool_call) => {
                    spawn_tool_execution(tool_call, app.registry.clone(), tx.clone());
                }
                Effect::SaveSession => {
                    session::save_current_session(&mut app);
                }
                _ => {}
            }
        }
    }

    // Save on exit if there's content
    session::save_current_session(&mut app);

    ratatui::restore();
    Ok(())
}

fn spawn_tool_execution(
    tool_call: crate::inference::ToolCall,
    registry: Arc<crate::core::tools::ToolRegistry>,
    tx: mpsc::Sender<Action>,
) {
    info!(
        "Spawning tool execution: {} (call_id={})",
        tool_call.name, tool_call.call_id
    );
    tokio::spawn(async move {
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            registry.execute(&tool_call),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    "Tool '{}' timed out after 30s (call_id={})",
                    tool_call.name, tool_call.call_id
                );
                serde_json::json!({"error": "Tool execution timed out after 30s"}).to_string()
            }
        };
        if tx
            .send(Action::ToolResultReady {
                call_id: tool_call.call_id.clone(),
                output,
            })
            .is_err()
        {
            warn!(
                "Failed to send tool result for call_id={}: receiver dropped",
                tool_call.call_id
            );
        }
    });
}

fn spawn_request(app: &App, tx: mpsc::Sender<Action>) -> Vec<tokio::task::AbortHandle> {
    info!("Spawning API request");

    // Clone what we need for the async task
    let provider = app.provider.clone();
    let context = app.context.clone();
    let model = app.model_name.clone();
    let effort = app.effort;
    let tools = app.tool_definitions();
    let max_output_tokens = Some(app.max_output_tokens);

    // Async channel for streaming chunks
    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<StreamChunk>(100);

    // Clone tx for the streaming task
    let tx_stream = tx.clone();

    // Spawn the provider streaming task
    let stream_handle = tokio::spawn(async move {
        let request = CompletionRequest {
            context: &context,
            model: &model,
            effort,
            tools: &tools,
            max_output_tokens,
        };

        if let Err(e) = provider.stream_completion(request, chunk_tx).await {
            info!("Stream error: {}", e);
            if tx_stream
                .send(Action::ResponseChunk {
                    text: format!("\n[Error: {}]", e),
                    item_id: None,
                })
                .is_err()
            {
                warn!("Failed to send stream error action: receiver dropped");
            }
        }
    });

    // Spawn a task to forward chunks to the Action channel
    let forward_handle = tokio::spawn(async move {
        let mut forwarded_count = 0usize;
        let mut total_content_len = 0usize;

        // Client-side timing
        let request_start = std::time::Instant::now();
        let mut first_content_time: Option<std::time::Instant> = None;

        while let Some(chunk) = chunk_rx.recv().await {
            forwarded_count += 1;
            match chunk {
                StreamChunk::Content { text, item_id } => {
                    if first_content_time.is_none() {
                        first_content_time = Some(std::time::Instant::now());
                    }
                    total_content_len += text.len();
                    debug!(
                        "Forwarding Action::ResponseChunk (len={}, total={})",
                        text.len(),
                        total_content_len
                    );
                    if tx.send(Action::ResponseChunk { text, item_id }).is_err() {
                        warn!("Failed to forward ResponseChunk: receiver dropped");
                        return;
                    }
                }
                StreamChunk::Thinking { text, item_id } => {
                    if first_content_time.is_none() {
                        first_content_time = Some(std::time::Instant::now());
                    }
                    debug!("Forwarding Action::ThinkingChunk (len={})", text.len());
                    if tx.send(Action::ThinkingChunk { text, item_id }).is_err() {
                        warn!("Failed to forward ThinkingChunk: receiver dropped");
                        return;
                    }
                }
                StreamChunk::ToolCall(tc) => {
                    debug!("Forwarding ToolCall: {} (call_id={})", tc.name, tc.call_id);
                    if tx.send(Action::ToolCallReceived(tc)).is_err() {
                        warn!("Failed to forward ToolCall: receiver dropped");
                        return;
                    }
                }
                StreamChunk::Completed(provider_stats) => {
                    let duration_ms = request_start.elapsed().as_millis() as u64;
                    let ttft_ms =
                        first_content_time.map(|t| (t - request_start).as_millis() as u64);

                    // Enrich provider stats with client-side timing
                    let mut stats = provider_stats.unwrap_or_default();
                    stats.ttft_ms = ttft_ms;
                    stats.generation_duration_ms = Some(duration_ms);
                    if let Some(output_tokens) = stats.output_tokens
                        && duration_ms > 0
                    {
                        stats.tokens_per_sec =
                            Some(output_tokens as f32 / (duration_ms as f32 / 1000.0));
                    }

                    debug!(
                        "Stream completed: ttft={}ms, duration={}ms, tok/s={:?}",
                        ttft_ms.unwrap_or(0),
                        duration_ms,
                        stats.tokens_per_sec
                    );

                    info!(
                        "Forwarding complete: {} actions, {} content bytes",
                        forwarded_count, total_content_len
                    );
                    if tx.send(Action::ResponseDone(Some(stats))).is_err() {
                        warn!("Failed to send ResponseDone: receiver dropped");
                    }
                    return;
                }
            }
        }

        // Fallback: channel closed without a Completed event
        info!(
            "Stream channel closed: {} actions, {} content bytes",
            forwarded_count, total_content_len
        );
        if tx.send(Action::ResponseDone(None)).is_err() {
            warn!("Failed to send ResponseDone: receiver dropped");
        }
    });

    vec![stream_handle.abort_handle(), forward_handle.abort_handle()]
}
