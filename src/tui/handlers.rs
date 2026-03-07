//! Event dispatch and effect processing, extracted from the main event loop.

use log::{debug, info, warn};
use std::sync::mpsc;

use ratatui::layout::Rect;

use crate::core::action::{Action, Effect, update};
use crate::core::config::ResolvedConfig;
use crate::core::session;
use crate::core::state::{ActiveModel, App};
use crate::inference::ContextItem;
use crate::tui::component::EventHandler;
use crate::tui::components::model_picker::ModelPickerEvent;
use crate::tui::components::session_manager::SessionEvent;
use crate::tui::components::{InputEvent, MessageListState, ModelPickerState, SessionManagerState};
use crate::tui::event::TuiEvent;
use crate::tui::{InputMode, TuiState, tasks, ui};

/// Dispatch a single TuiEvent. Returns true if the app should quit.
pub fn handle_event(
    event: TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    config: &ResolvedConfig,
    tx: &mpsc::Sender<Action>,
    frame_area: Rect,
) -> bool {
    if matches!(event, TuiEvent::Resize) {
        return false;
    }

    if matches!(event, TuiEvent::ForceQuit) {
        return update(app, Action::Quit) == Effect::Quit;
    }

    if matches!(event, TuiEvent::OpenSessionManager) {
        let index = session::load_index().unwrap_or_default();
        tui.session_manager = Some(SessionManagerState::new(index.sessions));
        return false;
    }

    if matches!(event, TuiEvent::OpenModelPicker) {
        let mut picker = ModelPickerState::new(app.available_models.clone());
        if let Some(ref models) = tui.fetched_models {
            picker.set_fetched_models(models.clone());
        }
        tui.model_picker = Some(picker);
        return false;
    }

    if tui.model_picker.is_some() {
        return handle_model_picker_event(&event, app, tui, config);
    }

    if tui.session_manager.is_some() {
        return handle_session_event(&event, app, tui, config);
    }

    if let TuiEvent::MouseMove(_col, row) = event {
        handle_mouse_move(row, app, tui, frame_area);
        return false;
    }

    if let TuiEvent::MouseClick(_col, row) = event {
        handle_mouse_click(row, app, tui, frame_area);
        return false;
    }

    if matches!(
        event,
        TuiEvent::ScrollUp
            | TuiEvent::ScrollDown
            | TuiEvent::ScrollPageUp
            | TuiEvent::ScrollPageDown
    ) {
        tui.message_list.handle_event(&event);
        return false;
    }

    match tui.input_mode {
        InputMode::Input => handle_input_mode(&event, app, tui, tx),
        InputMode::Cursor => handle_cursor_mode(&event, app, tui, tx),
    }
}

/// Drain the background action channel. Returns (should_quit, had_actions).
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
            Effect::Quit => return (true, had_actions),
            Effect::SpawnRequest => {
                tui.active_abort_handles = tasks::spawn_request(app, tx.clone());
            }
            Effect::ExecuteTool(tool_call) => {
                tasks::spawn_tool_execution(tool_call, app.registry.clone(), tx.clone());
            }
            Effect::SaveSession => {
                session::save_current_session(app);
            }
            Effect::RebuildProvider => {
                warn!("Unexpected RebuildProvider from background action");
            }
            _ => {}
        }
    }
    (false, had_actions)
}

// --- Private helpers ---

/// Cancel in-progress generation: abort tasks and dispatch CancelGeneration.
/// Returns true if the app should quit.
fn try_cancel_generation(app: &mut App, tui: &mut TuiState) -> bool {
    for handle in tui.active_abort_handles.drain(..) {
        handle.abort();
    }
    update(app, Action::CancelGeneration) == Effect::Quit
}

fn handle_input_mode(
    event: &TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    tx: &mpsc::Sender<Action>,
) -> bool {
    // Esc while loading → cancel generation
    if matches!(event, TuiEvent::Escape) && app.session.is_loading {
        return try_cancel_generation(app, tui);
    }
    // Esc → switch to Cursor mode
    if matches!(event, TuiEvent::Escape) {
        tui.input_mode = InputMode::Cursor;
        // Select the last non-ToolResult item
        let items = &app.session.context.items;
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

    if let Some(input_event) = tui.input_box.handle_event(event) {
        match input_event {
            InputEvent::Submit(text) => {
                if !app.session.is_loading {
                    let effect = update(app, Action::Submit(text));
                    if effect == Effect::SpawnRequest {
                        tui.active_abort_handles = tasks::spawn_request(app, tx.clone());
                    }
                }
            }
            InputEvent::CycleEffort => {
                return update(app, Action::CycleEffort) == Effect::Quit;
            }
            InputEvent::ContentChanged => {}
        }
    }
    false
}

fn handle_cursor_mode(
    event: &TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    tx: &mpsc::Sender<Action>,
) -> bool {
    let _ = tx; // unused here but kept for symmetry with handle_input_mode
    match event {
        TuiEvent::Escape if app.session.is_loading => try_cancel_generation(app, tui),
        TuiEvent::Escape => false,
        TuiEvent::InputChar(' ') => {
            if let Some(idx) = tui.message_list.selected_index
                && matches!(
                    app.session.context.items.get(idx),
                    Some(ContextItem::ToolCall(_))
                )
                && !tui.message_list.expanded_indices.remove(&idx)
            {
                tui.message_list.expanded_indices.insert(idx);
            }
            false
        }
        TuiEvent::InputChar(_) | TuiEvent::Paste(_) => {
            tui.input_mode = InputMode::Input;
            tui.message_list.selected_index = None;
            tui.input_box.handle_event(event);
            false
        }
        TuiEvent::Submit => {
            tui.input_mode = InputMode::Input;
            tui.message_list.selected_index = None;
            false
        }
        TuiEvent::CursorUp => {
            navigate_messages_up(app, tui);
            false
        }
        TuiEvent::CursorDown => {
            navigate_messages_down(app, tui);
            false
        }
        TuiEvent::CycleEffort => update(app, Action::CycleEffort) == Effect::Quit,
        _ => false,
    }
}

fn navigate_messages_up(app: &App, tui: &mut TuiState) {
    let items = &app.session.context.items;
    if !items.is_empty() {
        let mut idx = tui
            .message_list
            .selected_index
            .map(|i| i.saturating_sub(1))
            .unwrap_or(items.len() - 1);
        while idx > 0 && matches!(items[idx], ContextItem::ToolResult(_)) {
            idx -= 1;
        }
        tui.message_list.selected_index = Some(idx);
        tui.message_list.scroll_to_selected();
    }
}

fn navigate_messages_down(app: &App, tui: &mut TuiState) {
    let items = &app.session.context.items;
    if let Some(mut idx) = tui.message_list.selected_index
        && idx + 1 < items.len()
    {
        idx += 1;
        while idx < items.len() && matches!(items[idx], ContextItem::ToolResult(_)) {
            idx += 1;
        }
        if idx < items.len() {
            tui.message_list.selected_index = Some(idx);
            tui.message_list.scroll_to_selected();
        }
    }
}

fn handle_mouse_move(row: u16, _app: &App, tui: &mut TuiState, frame_area: Rect) {
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
        if matches!(
            app.session.context.items.get(idx),
            Some(ContextItem::ToolCall(_))
        ) && !tui.message_list.expanded_indices.remove(&idx)
        {
            tui.message_list.expanded_indices.insert(idx);
        }
    }
}

fn handle_session_event(
    event: &TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    config: &ResolvedConfig,
) -> bool {
    let sm = tui.session_manager.as_mut().unwrap();
    if let Some(session_event) = sm.handle_event(event) {
        match session_event {
            SessionEvent::Load(id) => {
                match session::load_session(&id) {
                    Ok(data) => {
                        let effect = update(app, Action::LoadSession(data));
                        if effect == Effect::RebuildProvider {
                            rebuild_provider(app, config);
                        }
                        if effect == Effect::Quit {
                            tui.session_manager = None;
                            return true;
                        }
                        tui.message_list = MessageListState::new();
                    }
                    Err(e) => {
                        warn!("Failed to load session {}: {}", id, e);
                        app.session.status_message = format!("Load failed: {}", e);
                    }
                }
                tui.session_manager = None;
            }
            SessionEvent::CreateNew => {
                let title = format!("Session #{}", session::next_session_number());
                let effect = update(app, Action::NewSession { title });
                if effect == Effect::Quit {
                    tui.session_manager = None;
                    return true;
                }
                tui.message_list = MessageListState::new();
                tui.session_manager = None;
            }
            SessionEvent::Rename { id, new_title } => {
                if let Err(e) = session::rename_session(&id, &new_title) {
                    warn!("Failed to rename session {}: {}", id, e);
                }
                if update(app, Action::SessionRenamed { id, new_title }) == Effect::Quit {
                    return true;
                }
            }
            SessionEvent::Delete(id) => {
                let is_active = app.session.current_session_id.as_deref() == Some(&id);
                if let Err(e) = session::delete_session(&id) {
                    warn!("Failed to delete session {}: {}", id, e);
                }
                sm.remove_session(&id);
                let effect = update(app, Action::SessionDeleted(id));
                if is_active {
                    tui.message_list = MessageListState::new();
                }
                if effect == Effect::Quit {
                    return true;
                }
            }
            SessionEvent::Dismiss => {
                tui.session_manager = None;
            }
        }
    }
    false
}

fn rebuild_provider(app: &mut App, config: &ResolvedConfig) {
    let mut new_config = config.clone();
    new_config.provider = app.model.provider.clone();
    new_config.model_name = app.model.name.clone();
    app.provider = crate::inference::build_provider(&new_config);
}

fn handle_model_picker_event(
    event: &TuiEvent,
    app: &mut App,
    tui: &mut TuiState,
    config: &ResolvedConfig,
) -> bool {
    let mp = tui.model_picker.as_mut().unwrap();
    if let Some(picker_event) = mp.handle_event(event) {
        match picker_event {
            ModelPickerEvent::Select(entry) => {
                let model = ActiveModel::new(entry.name.clone(), entry.provider.clone());
                let effect = update(app, Action::SwitchModel(model));
                if effect == Effect::RebuildProvider {
                    rebuild_provider(app, config);
                }
                info!("Model switched: {} ({})", entry.name, entry.provider);
                tui.model_picker = None;
            }
            ModelPickerEvent::Dismiss => {
                tui.model_picker = None;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{
        DEFAULT_LMSTUDIO_BASE_URL, DEFAULT_MAX_AGENTIC_ROUNDS, DEFAULT_MAX_OUTPUT_TOKENS,
        DEFAULT_OPENROUTER_BASE_URL, DEFAULT_SYSTEM_PROMPT, ModelEntry,
    };
    use crate::inference::{ContextItem, ContextSegment, Effort, Source, ToolResult};
    use crate::test_support::test_app;

    fn test_tui_state() -> TuiState {
        TuiState::new(Effort::default())
    }

    fn test_config() -> ResolvedConfig {
        ResolvedConfig {
            provider: "openrouter".to_string(),
            model_name: "test-model".to_string(),
            max_agentic_rounds: DEFAULT_MAX_AGENTIC_ROUNDS,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
            effort: Effort::default(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            openrouter_api_key: None,
            openrouter_base_url: DEFAULT_OPENROUTER_BASE_URL.to_string(),
            lmstudio_base_url: DEFAULT_LMSTUDIO_BASE_URL.to_string(),
            models: Vec::new(),
        }
    }

    fn test_frame_area() -> Rect {
        Rect::new(0, 0, 80, 24)
    }

    // --- Phase 2: Top-level dispatch ---

    #[test]
    fn test_resize_is_noop() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        let quit = handle_event(
            TuiEvent::Resize,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert!(!quit);
    }

    #[test]
    fn test_force_quit_returns_true() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        let quit = handle_event(
            TuiEvent::ForceQuit,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert!(quit);
    }

    #[test]
    fn test_open_model_picker() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.fetched_models = Some(vec![ModelEntry {
            name: "fetched-model".to_string(),
            provider: "openrouter".to_string(),
            description: None,
        }]);
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        let quit = handle_event(
            TuiEvent::OpenModelPicker,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert!(!quit);
        assert!(tui.model_picker.is_some());
    }

    #[test]
    fn test_model_picker_dismiss() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.model_picker = Some(ModelPickerState::new(vec![]));
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        let quit = handle_event(
            TuiEvent::Escape,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert!(!quit);
        assert!(tui.model_picker.is_none());
    }

    #[test]
    fn test_scroll_events_dont_quit() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        for event in [
            TuiEvent::ScrollUp,
            TuiEvent::ScrollDown,
            TuiEvent::ScrollPageUp,
            TuiEvent::ScrollPageDown,
        ] {
            let quit = handle_event(event, &mut app, &mut tui, &config, &tx, test_frame_area());
            assert!(!quit);
        }
    }

    // --- Phase 3: Mode switching and input routing ---

    #[test]
    fn test_escape_switches_to_cursor_mode() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        assert_eq!(tui.input_mode, InputMode::Input);
        // Add a user message so there's something to select
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::User,
                content: "hello".to_string(),
            }));
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::Escape,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert_eq!(tui.input_mode, InputMode::Cursor);
        assert!(tui.message_list.selected_index.is_some());
    }

    #[test]
    fn test_escape_selects_last_non_tool_result() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        // items[0] = system directive (from test_app)
        // items[1] = user message
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::User,
                content: "hello".to_string(),
            }));
        // items[2] = tool result (should be skipped)
        app.session
            .context
            .items
            .push(ContextItem::ToolResult(ToolResult {
                call_id: "call_1".to_string(),
                output: "result".to_string(),
            }));
        // items[3] = model message (should be selected)
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::Model,
                content: "response".to_string(),
            }));
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::Escape,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert_eq!(tui.message_list.selected_index, Some(3));
    }

    #[test]
    fn test_typing_in_cursor_mode_switches_to_input() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.input_mode = InputMode::Cursor;
        tui.message_list.selected_index = Some(0);
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::InputChar('a'),
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert_eq!(tui.input_mode, InputMode::Input);
        assert_eq!(tui.message_list.selected_index, None);
    }

    #[test]
    fn test_enter_in_cursor_mode_switches_to_input() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.input_mode = InputMode::Cursor;
        tui.message_list.selected_index = Some(0);
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::Submit,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert_eq!(tui.input_mode, InputMode::Input);
        assert_eq!(tui.message_list.selected_index, None);
    }

    #[test]
    fn test_escape_while_loading_cancels_generation() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        app.session.is_loading = true;
        // Add a fake abort handle
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let handle = rt.spawn(async {
            loop {
                tokio::task::yield_now().await
            }
        });
        tui.active_abort_handles.push(handle.abort_handle());
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        let quit = handle_event(
            TuiEvent::Escape,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert!(!quit);
        assert!(tui.active_abort_handles.is_empty());
        assert!(!app.session.is_loading);
    }

    // --- Phase 4: Navigation ---

    #[test]
    fn test_navigate_up_from_bottom() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.input_mode = InputMode::Cursor;
        // Add 3 user messages (indices 1, 2, 3 — 0 is system)
        for i in 0..3 {
            app.session
                .context
                .items
                .push(ContextItem::Message(ContextSegment {
                    source: Source::User,
                    content: format!("msg {i}"),
                }));
        }
        tui.message_list.selected_index = Some(3); // last item
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::CursorUp,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert_eq!(tui.message_list.selected_index, Some(2));
    }

    #[test]
    fn test_navigate_up_skips_tool_results() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.input_mode = InputMode::Cursor;
        // [0]=system, [1]=user, [2]=tool_result, [3]=model
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::User,
                content: "hello".to_string(),
            }));
        app.session
            .context
            .items
            .push(ContextItem::ToolResult(ToolResult {
                call_id: "c1".to_string(),
                output: "out".to_string(),
            }));
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::Model,
                content: "response".to_string(),
            }));
        tui.message_list.selected_index = Some(3);
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::CursorUp,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        // Should skip index 2 (ToolResult) and land on 1
        assert_eq!(tui.message_list.selected_index, Some(1));
    }

    #[test]
    fn test_navigate_down_skips_tool_results() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.input_mode = InputMode::Cursor;
        // [0]=system, [1]=user, [2]=tool_result, [3]=model
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::User,
                content: "hello".to_string(),
            }));
        app.session
            .context
            .items
            .push(ContextItem::ToolResult(ToolResult {
                call_id: "c1".to_string(),
                output: "out".to_string(),
            }));
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::Model,
                content: "response".to_string(),
            }));
        tui.message_list.selected_index = Some(1);
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::CursorDown,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        // Should skip index 2 (ToolResult) and land on 3
        assert_eq!(tui.message_list.selected_index, Some(3));
    }

    #[test]
    fn test_navigate_up_from_none_selects_last() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.input_mode = InputMode::Cursor;
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::User,
                content: "hello".to_string(),
            }));
        tui.message_list.selected_index = None;
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::CursorUp,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        // Should select last item (index 1)
        assert_eq!(tui.message_list.selected_index, Some(1));
    }

    #[test]
    fn test_navigate_down_at_end_stays_put() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        tui.input_mode = InputMode::Cursor;
        app.session
            .context
            .items
            .push(ContextItem::Message(ContextSegment {
                source: Source::User,
                content: "hello".to_string(),
            }));
        tui.message_list.selected_index = Some(1); // last item
        let config = test_config();
        let (tx, _rx) = mpsc::channel();

        handle_event(
            TuiEvent::CursorDown,
            &mut app,
            &mut tui,
            &config,
            &tx,
            test_frame_area(),
        );

        assert_eq!(tui.message_list.selected_index, Some(1));
    }

    // --- Phase 5: Background action processing ---

    #[test]
    fn test_background_actions_empty_channel() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        let (tx, rx) = mpsc::channel();

        let (quit, had_actions) = process_background_actions(&rx, &mut app, &mut tui, &tx);

        assert!(!quit);
        assert!(!had_actions);
    }

    #[test]
    fn test_background_actions_models_fetched() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        let (tx, rx) = mpsc::channel();
        let models = vec![ModelEntry {
            name: "test-model".to_string(),
            provider: "openrouter".to_string(),
            description: None,
        }];
        tx.send(Action::ModelsFetched(models.clone())).unwrap();

        let (quit, had_actions) = process_background_actions(&rx, &mut app, &mut tui, &tx);

        assert!(!quit);
        assert!(had_actions);
        assert_eq!(tui.fetched_models.as_ref().unwrap().len(), 1);
        // ModelsFetched shouldn't reach the reducer — context items unchanged
        assert_eq!(app.session.context.items.len(), 1); // just system
    }

    #[test]
    fn test_background_actions_quit() {
        let mut app = test_app();
        let mut tui = test_tui_state();
        let (tx, rx) = mpsc::channel();
        tx.send(Action::Quit).unwrap();

        let (quit, had_actions) = process_background_actions(&rx, &mut app, &mut tui, &tx);

        assert!(quit);
        assert!(had_actions);
    }

    #[test]
    fn test_background_actions_response_chunk() {
        let mut app = test_app();
        app.session.is_loading = true;
        let mut tui = test_tui_state();
        let (tx, rx) = mpsc::channel();
        tx.send(Action::ResponseChunk {
            text: "hello".to_string(),
            item_id: None,
        })
        .unwrap();

        let (quit, had_actions) = process_background_actions(&rx, &mut app, &mut tui, &tx);

        assert!(!quit);
        assert!(had_actions);
        // The chunk should have been processed by the reducer
        assert_eq!(app.session.context.items.len(), 2); // system + model response
    }
}
