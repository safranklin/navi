//! # TUI Adapter
//!
//! The ratatui-specific layer. Handles terminal I/O, renders the UI,
//! and translates keyboard events into core::Action values.
//!
//! This is the only module that knows about ratatui and crossterm.
//! The intention is to swap this out for a different adapter (React, web, etc.) in the future
//! if needed.

mod event;
mod ui;

use std::env;
use std::io::stdout;
use crossterm::execute;
use crossterm::event::{EnableMouseCapture, DisableMouseCapture};
use tui_scrollview::ScrollViewState;

use crate::core::action::{Action, update, Effect};
use crate::core::state::App;
use crate::tui::event::{poll_event, TuiEvent};

use std::sync::mpsc;
use crate::api::client::stream_completion;
use crate::api::types::StreamChunk;

/// TUI-specific presentation state (not part of core business logic)
pub struct TuiState {
    pub scroll_state: ScrollViewState,
    pub has_unseen_content: bool,
    pub hovered_index: Option<usize>,
    pub input_buffer: String,
    /// Cached segment heights from last render (for hit testing)
    pub segment_heights: Vec<u16>,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            scroll_state: ScrollViewState::default(),
            has_unseen_content: false,
            hovered_index: None,
            input_buffer: String::new(),
            segment_heights: Vec::new(),
        }
    }
}

struct MouseCaptureGuard;

impl MouseCaptureGuard {
    fn new() -> std::io::Result<Self> {
        execute!(stdout(), EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for MouseCaptureGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableMouseCapture);
    }
}

pub fn run() -> std::io::Result<()> {
    let mut app = App::new(env::var("PRIMARY_MODEL_NAME").expect("PRIMARY_MODEL_NAME must be set"));
    let mut tui = TuiState::new();
    let mut terminal = ratatui::init();
    let _mouse_capture_guard = MouseCaptureGuard::new();

    // Channel for actions from background tasks
    let (tx, rx) = mpsc::channel();

    loop {
        terminal.draw(|f| ui::draw_ui(f, &app, &mut tui))?;

        // Handle user input
        if let Some(event) = poll_event() {
            match event {
                // TUI-local events - modify TuiState directly
                TuiEvent::InputChar(c) => {
                    tui.input_buffer.push(c);
                }
                TuiEvent::Backspace => {
                    tui.input_buffer.pop();
                }
                TuiEvent::ScrollUp => {
                    tui.scroll_state.scroll_up();
                }
                TuiEvent::ScrollDown => {
                    tui.scroll_state.scroll_down();
                }
                TuiEvent::MouseMove(_col, row) => {
                    let frame_area = terminal.get_frame().area();
                    let scroll_offset = tui.scroll_state.offset().y;
                    tui.hovered_index = ui::hit_test_message(
                        row,
                        frame_area,
                        scroll_offset,
                        &tui.segment_heights,
                    );
                }

                // Core events - pass to core::update
                TuiEvent::Quit => {
                    let effect = update(&mut app, Action::Quit);
                    if effect == Effect::Quit {
                        break;
                    }
                }
                TuiEvent::Submit => {
                    let message = std::mem::take(&mut tui.input_buffer);
                    let effect = update(&mut app, Action::Submit(message));
                    if effect == Effect::SpawnRequest {
                        spawn_request(&app, tx.clone());
                    }
                }
            }
        }

        // Handle background task actions (streaming responses)
        while let Ok(action) = rx.try_recv() {
            let effect = update(&mut app, action);
            match effect {
                Effect::Quit => break,
                Effect::SpawnRequest => {
                    spawn_request(&app, tx.clone());
                }
                _ => {}
            }
        }
    }
    ratatui::restore();
    Ok(())
}

fn spawn_request(app: &App, tx: mpsc::Sender<Action>) {
    let context = app.context.clone();
    
    // Channel for the stream chunks (StreamChunk)
    let (str_tx, str_rx) = mpsc::channel();
    
    // Spawn async task to drive the stream
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = stream_completion(&context, str_tx).await {
            let _ = tx_clone.send(Action::ResponseChunk(format!("\n[Error: {}]", e)));
            let _ = tx_clone.send(Action::ResponseDone);
        }
    });
    
    // Spawn blocking task to forward chunks as Actions
    let tx_forward = tx.clone();
    tokio::task::spawn_blocking(move || {
        while let Ok(chunk) = str_rx.recv() {
            match chunk {
                StreamChunk::Content(c) => {
                    let _ = tx_forward.send(Action::ResponseChunk(c));
                }
                StreamChunk::Thinking(t) => {
                    let _ = tx_forward.send(Action::ThinkingChunk(t));
                }
            }
        }
        let _ = tx_forward.send(Action::ResponseDone);
    }); 
}