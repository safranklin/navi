//! # Event Handlers
//!
//! Event semantics live here; scheduling lives in `run()`. The event loop
//! decides *when* to draw, poll, and quit. These handlers decide *what an
//! event means* — which mode we're in, what state to mutate, which effects
//! to fire. Keeping the two apart means you can reason about timing without
//! reading business logic, and vice versa.
//!
//! Two public entry points:
//! - `handle_event()` — dispatches a single `TuiEvent` to the right handler
//! - `process_background_actions()` — drains the action channel and processes effects
//!
//! Plus modal overlay handlers (session manager, model picker) from the earlier refactor.

use log::{debug, info, warn};
use ratatui::layout::Rect;
use std::sync::mpsc;

use crate::core::action::{Action, Effect, update};
use crate::core::config::ResolvedConfig;
use crate::core::session;
use crate::core::state::App;
use crate::inference::ContextItem;
use crate::tui::component::EventHandler;
use crate::tui::components::model_picker::ModelPickerEvent;
use crate::tui::components::session_manager::SessionEvent;
use crate::tui::components::{InputEvent, MessageListState, ModelPickerState, SessionManagerState};
use crate::tui::event::TuiEvent;
use crate::tui::tasks;
use crate::tui::{InputMode, TuiState};

// ── Public entry points ─────────────────────────────────────────────

/// Dispatch a single TUI event. Returns `true` if the app should quit.
pub fn handle_event(
    event: &TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    config: &ResolvedConfig,
    tx: &mpsc::Sender<Action>,
    frame_area: Rect,
) -> bool {
    // ForceQuit (Ctrl+C) always quits regardless of mode
    if matches!(event, TuiEvent::ForceQuit) {
        return update(app, Action::Quit) == Effect::Quit;
    }

    // Ctrl+O opens session manager
    if matches!(event, TuiEvent::OpenSessionManager) {
        let index = session::load_index().unwrap_or_default();
        tui.session_manager = Some(SessionManagerState::new(index.sessions));
        return false;
    }

    // Ctrl+P opens model picker
    if matches!(event, TuiEvent::OpenModelPicker) {
        let mut picker = ModelPickerState::new(app.available_models.clone());
        if let Some(ref models) = tui.fetched_models {
            picker.set_fetched_models(models.clone());
        }
        tui.model_picker = Some(picker);
        return false;
    }

    // Modal overlays consume all events when open
    if tui.model_picker.is_some() {
        let picker_event = tui.model_picker.as_mut().unwrap().handle_event(event);
        if let Some(ev) = picker_event {
            handle_model_picker_event(ev, app, tui, config);
        }
        return false;
    }

    if tui.session_manager.is_some() {
        let session_event = tui.session_manager.as_mut().unwrap().handle_event(event);
        if let Some(ev) = session_event {
            return handle_session_event(ev, app, tui);
        }
        return false;
    }

    // Mouse events — always active regardless of input mode
    if let TuiEvent::MouseMove(_col, row) = *event {
        handle_mouse_move(row, app, tui, frame_area);
        return false;
    }

    if let TuiEvent::MouseClick(_col, row) = *event {
        handle_mouse_click(row, app, tui, frame_area);
        return false;
    }

    // Scroll events — always go to MessageList
    if matches!(
        event,
        TuiEvent::ScrollUp | TuiEvent::ScrollDown | TuiEvent::ScrollPageUp | TuiEvent::ScrollPageDown
    ) {
        tui.message_list.handle_event(event);
        return false;
    }

    // Modal input mode dispatch
    match tui.input_mode {
        InputMode::Input => handle_input_mode(event, app, tui, tx),
        InputMode::Cursor => handle_cursor_mode(event, app, tui, tx),
    }
}

/// Drain background actions from the channel and process effects.
/// Returns `(should_quit, had_actions)`.
pub fn process_background_actions(
    rx: &mpsc::Receiver<Action>,
    app: &mut App,
    tui: &mut TuiState,
    tx: &mpsc::Sender<Action>,
) -> (bool, bool) {
    let mut had_actions = false;

    while let Ok(action) = rx.try_recv() {
        had_actions = true;

        // Intercept ModelsFetched — TUI-only state, not core business logic
        if let Action::ModelsFetched(models) = action {
            debug!("Received {} fetched models", models.len());
            tui.fetched_models = Some(models.clone());
            if let Some(ref mut mp) = tui.model_picker {
                mp.set_fetched_models(models);
            }
            continue;
        }

        debug!("Event loop received: {:?}", action);
        let effect = update(app, action);
        match effect {
            Effect::Quit => return (true, true),
            Effect::SpawnRequest => {
                tui.active_abort_handles = tasks::spawn_request(app, tx.clone());
            }
            Effect::ExecuteTool(tool_call) => {
                tasks::spawn_tool_execution(tool_call, app.registry.clone(), tx.clone());
            }
            Effect::SaveSession => {
                session::save_current_session(app);
                if !tui.title_generation_pending && session::needs_title_generation(app) {
                    tui.title_generation_pending = true;
                    tasks::spawn_title_generation(app, tx.clone());
                }
            }
            _ => {}
        }
    }

    (false, had_actions)
}

// ── Private helpers ─────────────────────────────────────────────────

/// Try to cancel in-flight generation. Returns `true` if cancellation happened.
fn try_cancel_generation(app: &mut App, tui: &mut TuiState) -> bool {
    if !app.is_loading {
        return false;
    }
    for handle in tui.active_abort_handles.drain(..) {
        handle.abort();
    }
    update(app, Action::CancelGeneration);
    true
}

/// Handle events in Input mode. Returns `true` if the app should quit.
fn handle_input_mode(
    event: &TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    tx: &mpsc::Sender<Action>,
) -> bool {
    // Esc while loading → cancel generation
    if matches!(event, TuiEvent::Escape) && app.is_loading {
        try_cancel_generation(app, tui);
        return false;
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
        tui.message_list.selected_index = if !items.is_empty() { Some(idx) } else { None };
        return false;
    }

    // InputBox handles everything else
    if let Some(input_event) = tui.input_box.handle_event(event) {
        match input_event {
            InputEvent::Submit(text) => {
                if !app.is_loading {
                    let effect = update(app, Action::Submit(text));
                    if effect == Effect::SpawnRequest {
                        tui.active_abort_handles = tasks::spawn_request(app, tx.clone());
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
    false
}

/// Handle events in Cursor mode. Returns `true` if the app should quit.
fn handle_cursor_mode(
    event: &TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    _tx: &mpsc::Sender<Action>,
) -> bool {
    match event {
        // Esc while loading → cancel generation
        TuiEvent::Escape if app.is_loading => {
            try_cancel_generation(app, tui);
        }
        // Esc in Cursor mode is a no-op
        TuiEvent::Escape => {}
        // Space toggles expansion of selected tool call
        TuiEvent::InputChar(' ') => {
            if let Some(idx) = tui.message_list.selected_index
                && matches!(app.context.items.get(idx), Some(ContextItem::ToolCall(_)))
                && !tui.message_list.expanded_indices.remove(&idx)
            {
                tui.message_list.expanded_indices.insert(idx);
            }
        }
        // Typing auto-switches to Input mode and forwards the event
        TuiEvent::InputChar(_) | TuiEvent::Paste(_) => {
            tui.input_mode = InputMode::Input;
            tui.message_list.selected_index = None;
            tui.input_box.handle_event(event);
        }
        // Enter switches to Input mode
        TuiEvent::Submit => {
            tui.input_mode = InputMode::Input;
            tui.message_list.selected_index = None;
        }
        // Up/Down navigate messages (skipping consumed ToolResults)
        TuiEvent::CursorUp => navigate_messages_up(app, tui),
        TuiEvent::CursorDown => navigate_messages_down(app, tui),
        // CycleEffort works in both modes
        TuiEvent::CycleEffort => {
            app.effort = app.effort.next();
            app.status_message = format!("Reasoning: {}", app.effort.label());
        }
        _ => {}
    }
    false
}

fn handle_mouse_move(row: u16, _app: &App, tui: &mut TuiState, frame_area: Rect) {
    use crate::tui::ui;

    let scroll_offset = tui.message_list.scroll_state.offset().y;
    let input_height = tui.input_box.calculate_height(frame_area.width);

    tui.message_list.selected_index = ui::hit_test_message(
        row,
        frame_area,
        scroll_offset,
        &tui.message_list.layout.prefix_heights,
        input_height,
    );
}

fn handle_mouse_click(row: u16, app: &App, tui: &mut TuiState, frame_area: Rect) {
    use crate::tui::ui;

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
}

fn navigate_messages_up(app: &App, tui: &mut TuiState) {
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

fn navigate_messages_down(app: &App, tui: &mut TuiState) {
    let items = &app.context.items;
    if let Some(mut idx) = tui.message_list.selected_index
        && idx + 1 < items.len()
    {
        idx += 1;
        // Skip forwards past ToolResult items
        while idx < items.len() && matches!(items[idx], ContextItem::ToolResult(_)) {
            idx += 1;
        }
        // Only update if we landed on a valid index
        if idx < items.len() {
            tui.message_list.selected_index = Some(idx);
            tui.message_list.scroll_to_selected();
        }
    }
}

// ── Modal overlay handlers ──────────────────────────────────────────

/// Handle a session manager event. Returns true if the app should quit.
pub fn handle_session_event(event: SessionEvent, app: &mut App, tui: &mut TuiState) -> bool {
    let mut should_quit = false;

    match event {
        SessionEvent::Load(id) => {
            // Save outgoing session and regenerate its title in the background
            session::save_current_session(app);
            tasks::spawn_title_regeneration_for_outgoing(app);

            match session::load_session(&id) {
                Ok(data) => {
                    let effect = update(app, Action::LoadSession(data));
                    if effect == Effect::Quit {
                        should_quit = true;
                    }
                    tui.message_list = MessageListState::new();
                    tui.title_generation_pending = false;
                }
                Err(e) => {
                    warn!("Failed to load session {}: {}", id, e);
                    app.status_message = format!("Load failed: {}", e);
                }
            }
            tui.session_manager = None;
        }
        SessionEvent::CreateNew => {
            // Save outgoing session and regenerate its title in the background
            session::save_current_session(app);
            tasks::spawn_title_regeneration_for_outgoing(app);

            let effect = update(app, Action::NewSession);
            if effect == Effect::Quit {
                should_quit = true;
            }
            // Assign "Session #N" title immediately
            app.session_title = format!("Session #{}", session::next_session_number());
            tui.message_list = MessageListState::new();
            tui.title_generation_pending = false;
            tui.session_manager = None;
        }
        SessionEvent::Rename { id, new_title } => {
            if let Err(e) = session::rename_session(&id, &new_title) {
                warn!("Failed to rename session {}: {}", id, e);
            }
            // Update active session title if renaming the current one
            if app.current_session_id.as_deref() == Some(&id) {
                app.session_title = new_title;
            }
        }
        SessionEvent::Delete(id) => {
            if let Err(e) = session::delete_session(&id) {
                warn!("Failed to delete session {}: {}", id, e);
            }
            if let Some(ref mut sm) = tui.session_manager {
                sm.remove_session(&id);
            }
            // If we deleted the active session, clear the ID
            if app.current_session_id.as_deref() == Some(&id) {
                app.current_session_id = None;
            }
        }
        SessionEvent::Dismiss => {
            tui.session_manager = None;
        }
    }

    should_quit
}

/// Handle a model picker event.
pub fn handle_model_picker_event(
    event: ModelPickerEvent,
    app: &mut App,
    tui: &mut TuiState,
    config: &ResolvedConfig,
) {
    match event {
        ModelPickerEvent::Select(entry) => {
            // Build a new resolved config with the selected model/provider
            let mut new_config = config.clone();
            new_config.provider = entry.provider.clone();
            new_config.model_name = entry.name.clone();

            // Rebuild the provider for the new model
            app.provider = crate::inference::build_provider(&new_config);
            app.model_name = entry.name.clone();
            app.provider_name = entry.provider.clone();
            app.status_message = format!("Switched to {} ({})", entry.name, entry.provider);
            info!("Model switched: {} ({})", entry.name, entry.provider);
            tui.model_picker = None;
        }
        ModelPickerEvent::Dismiss => {
            tui.model_picker = None;
        }
    }
}
