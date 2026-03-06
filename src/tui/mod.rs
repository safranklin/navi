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
mod handlers;
pub mod markdown;
mod stream_buffer;
mod tasks;
mod ui;

use log::info;
use std::io::stdout;
use std::sync::mpsc;

use crossterm::cursor::{Hide, SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;

use crate::core::config::{ModelEntry, ResolvedConfig};
use crate::core::session;
use crate::core::state::App;
use crate::inference::Effort;
use crate::tui::components::{InputBox, MessageListState, ModelPickerState, SessionManagerState};
use crate::tui::event::{poll_event_immediate, poll_event_timeout};

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
    // Model picker overlay (None = hidden)
    pub model_picker: Option<ModelPickerState>,
    // Pre-fetched models from provider APIs (populated at startup)
    pub fetched_models: Option<Vec<ModelEntry>>,
    // Abort handles for the current generation (used by Escape-to-cancel)
    pub active_abort_handles: Vec<tokio::task::AbortHandle>,
}

impl TuiState {
    pub fn new(initial_effort: Effort) -> Self {
        Self {
            message_list: MessageListState::new(),
            input_box: InputBox::new(initial_effort),
            input_mode: InputMode::Input, // User expects to type immediately
            pulse_value: 0.0,
            session_manager: None,
            model_picker: None,
            fetched_models: None,
            active_abort_handles: Vec::new(),
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

        // Drain any buffered mouse/keyboard events before restoring the terminal.
        // If ratatui::restore() disables raw mode while mouse tracking escape
        // sequences are still buffered, they leak as visible garbage (e.g. "35;37;36M").
        while crossterm::event::poll(std::time::Duration::ZERO).unwrap_or(false) {
            let _ = crossterm::event::read();
        }

        ratatui::restore();
    }
}

pub fn run(config: ResolvedConfig) -> std::io::Result<()> {
    let provider = crate::inference::build_provider(&config);
    let mut app = App::from_config(provider, &config);
    let mut tui = TuiState::new(app.effort);

    // Open session manager on startup so user picks a session (or starts new)
    let index = session::load_index().unwrap_or_default();
    tui.session_manager = Some(SessionManagerState::new(index.sessions));

    let mut terminal = ratatui::init();
    let _terminal_mode_guard = TerminalModeGuard::new();

    // Channel for actions from background tasks
    let (tx, rx) = mpsc::channel();

    // Fetch available models from providers in the background at startup
    tasks::spawn_model_fetch(&app, tx.clone());

    // Animation timer
    let start_time = std::time::Instant::now();
    let mut needs_redraw = true; // Force first frame

    loop {
        // Sync InputBox props with App/TUI state
        tui.input_box.effort = app.effort;
        tui.input_box.dimmed = matches!(tui.input_mode, InputMode::Cursor);

        // Determine if animations are running (landing page or loading spinner)
        let animating = app.session.is_loading || !app.session.context.has_visible_messages();

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
        let frame_area = terminal.get_frame().area();
        for event in first_event
            .into_iter()
            .chain(std::iter::from_fn(poll_event_immediate))
        {
            if handlers::handle_event(event, &mut app, &mut tui, &config, &tx, frame_area) {
                should_quit = true;
            }
        }

        if should_quit {
            break;
        }

        // Handle background task actions (streaming responses)
        let (quit, had_actions) =
            handlers::process_background_actions(&rx, &mut app, &mut tui, &tx);
        if had_actions {
            needs_redraw = true;
        }
        if quit {
            break;
        }
    }

    // Save on exit if there's content
    session::save_current_session(&mut app);

    // Terminal restoration happens in TerminalModeGuard::drop() — it disables
    // mouse capture, drains buffered events, then calls ratatui::restore().
    // This ordering prevents escape sequence garbage on exit.
    Ok(())
}
