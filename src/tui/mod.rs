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
mod component;
mod components;

use log::{debug, info};
use std::env;
use std::io::stdout;
use std::sync::{mpsc, Arc};

use crossterm::execute;
use crossterm::event::{
    EnableMouseCapture, DisableMouseCapture, EnableBracketedPaste, DisableBracketedPaste,
    KeyboardEnhancementFlags, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
};
use crossterm::cursor::{Show, Hide, SetCursorStyle};

use crate::core::action::{Action, update, Effect};
use crate::core::state::App;
use crate::inference::{CompletionRequest, CompletionProvider, LmStudioProvider, OpenRouterProvider, StreamChunk};
use crate::inference::Effort;
use crate::Provider;
use crate::tui::event::{poll_event, poll_event_immediate, TuiEvent};
use crate::tui::component::EventHandler;
use crate::tui::components::{InputBox, InputEvent, MessageListState};

/// TUI-specific presentation state (not part of core business logic)
pub struct TuiState {
    // Persistent component states
    pub message_list: MessageListState,
    pub input_box: InputBox,
    // Animation state
    pub pulse_value: f32,
}

impl TuiState {
    pub fn new(initial_effort: Effort) -> Self {
        Self {
            message_list: MessageListState::new(),
            input_box: InputBox::new(initial_effort),
            pulse_value: 0.0,
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
            Show, // Show cursor for input editing
            SetCursorStyle::BlinkingBar, // Enable blinking cursor
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
        info!("Terminal modes enabled (mouse, bracketed paste, blinking cursor, keyboard enhancement)");
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

pub fn run(provider_choice: Provider) -> std::io::Result<()> {
    let provider: Arc<dyn CompletionProvider> = match provider_choice {
        Provider::OpenRouter => Arc::new(OpenRouterProvider::new(
            env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY must be set"),
            None,
        )),
        Provider::LmStudio => Arc::new(LmStudioProvider::new(None)),
    };

    let model_name = env::var("PRIMARY_MODEL_NAME").expect("PRIMARY_MODEL_NAME must be set");
    let mut app = App::new(provider, model_name);
    // Initialize TuiState with current effort from App
    let mut tui = TuiState::new(app.effort);
    
    let mut terminal = ratatui::init();
    let _terminal_mode_guard = TerminalModeGuard::new();

    // Channel for actions from background tasks
    let (tx, rx) = mpsc::channel();
    
    // Animation timer
    let start_time = std::time::Instant::now();

    loop {
        // Sync InputBox effort prop with App state
        tui.input_box.effort = app.effort;
        
        // Update animation state (sine wave breathing)
        let elapsed = start_time.elapsed().as_secs_f32();
        tui.pulse_value = (elapsed * 5.0).sin() * 0.5 + 0.5;
        
        // Frame counter for animations (approx 12 fps for spinner/landing)
        let spinner_frame = (elapsed * 12.0) as usize;

        terminal.draw(|f| ui::draw_ui(f, &app, &mut tui, spinner_frame))?;

        // Wait for first event (with timeout for background task responsiveness)
        let first_event = poll_event();

        // Process first event + drain ALL pending events before next draw
        let mut should_quit = false;
        for event in first_event.into_iter().chain(std::iter::from_fn(poll_event_immediate)) {
            // Priority 1: Check for global Quit
            if matches!(event, TuiEvent::Quit) {
                let effect = update(&mut app, Action::Quit);
                if effect == Effect::Quit {
                    should_quit = true;
                }
                continue;
            }

            // Priority 2: Delegate to InputBox (handles typing, pasting, submitting, cycle effort)
            // Note: InputBox returns Option<InputEvent>
            if let Some(input_event) = tui.input_box.handle_event(&event) {
                match input_event {
                    InputEvent::Submit(text) => {
                        // Don't allow submitting while loading
                        if !app.is_loading {
                            let effect = update(&mut app, Action::Submit(text));
                            if effect == Effect::SpawnRequest {
                                spawn_request(&app, tx.clone());
                            }
                        }
                    }
                    InputEvent::CycleEffort => {
                        // InputBox detected Ctrl+R, we update App state
                        app.effort = app.effort.next();
                        app.status_message = format!("Reasoning: {}", app.effort.label());
                    }
                    InputEvent::ContentChanged => {
                        // Redraw handled by main loop
                    }
                }
                continue;
            }

            // Priority 3: Delegate to MessageList (scrolling)
            if tui.message_list.handle_event(&event).is_some() {
                 // MessageList handled it
                 continue;
            }

            // Priority 4: Mouse hover (global for now, or could be in MessageListState)
            if let TuiEvent::MouseMove(_col, row) = event {
                // We need hit testing. Use layout from message_list state.
                let frame_area = terminal.get_frame().area();
                let scroll_offset = tui.message_list.scroll_state.offset().y;
                let input_height = tui.input_box.calculate_height(frame_area.width);

                tui.message_list.hovered_index = ui::hit_test_message(
                    row,
                    frame_area,
                    scroll_offset,
                    &tui.message_list.layout.prefix_heights,
                    input_height,
                );
            }
        }
        
        if should_quit {
            break;
        }

        // Handle background task actions (streaming responses)
        while let Ok(action) = rx.try_recv() {
            debug!("Event loop received: {:?}", action);
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
    info!("Spawning API request");

    // Clone what we need for the async task
    let provider = app.provider.clone();
    let context = app.context.clone();
    let model = app.model_name.clone();
    let effort = app.effort;

    // Async channel for streaming chunks
    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<StreamChunk>(100);

    // Clone tx for the streaming task
    let tx_stream = tx.clone();

    // Spawn the provider streaming task
    tokio::spawn(async move {
        let request = CompletionRequest {
            context: &context,
            model: &model,
            effort,
        };

        if let Err(e) = provider.stream_completion(request, chunk_tx).await {
            info!("Stream error: {}", e);
            let _ = tx_stream.send(Action::ResponseChunk(format!("\n[Error: {}]", e)));
        }
    });

    // Spawn a task to forward chunks to the Action channel
    tokio::spawn(async move {
        let mut forwarded_count = 0usize;
        let mut total_content_len = 0usize;

        while let Some(chunk) = chunk_rx.recv().await {
            forwarded_count += 1;
            match chunk {
                StreamChunk::Content(c) => {
                    total_content_len += c.len();
                    debug!("Forwarding Action::ResponseChunk (len={}, total={})", c.len(), total_content_len);
                    let _ = tx.send(Action::ResponseChunk(c));
                }
                StreamChunk::Thinking(t) => {
                    debug!("Forwarding Action::ThinkingChunk (len={})", t.len());
                    let _ = tx.send(Action::ThinkingChunk(t));
                }
            }
        }

        info!("Forwarding complete: {} actions, {} content bytes", forwarded_count, total_content_len);
        let _ = tx.send(Action::ResponseDone);
    });
}