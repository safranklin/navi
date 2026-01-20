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

use log::{debug, info};
use std::env;
use std::io::stdout;
use crossterm::execute;
use crossterm::event::{EnableMouseCapture, DisableMouseCapture};
use tui_scrollview::ScrollViewState;

use crate::core::action::{Action, update, Effect};
use crate::core::state::App;
use crate::tui::event::{poll_event, poll_event_immediate, TuiEvent};

use std::sync::mpsc;
use crate::api::client::stream_completion;
use crate::api::types::StreamChunk;

/// Cached layout measurements for efficient rendering and hit testing
pub struct LayoutCache {
    /// Individual segment heights
    pub heights: Vec<u16>,
    /// Cumulative heights for O(log n) hit testing via binary search
    pub prefix_heights: Vec<u16>,

    /// Internal metadata for cache validity
    /// Message count when cache was last built
    message_count: usize,
    /// Content width when cache was last built
    content_width: u16,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            heights: Vec::new(),
            prefix_heights: Vec::new(),
            message_count: 0,
            content_width: 0,
        }
    }

    /// Returns how many cached heights are still valid and can be reused.
    /// Returns 0 if full rebuild needed, or N if first N heights are reusable.
    pub fn reusable_count(&self, message_count: usize, content_width: u16, is_loading: bool) -> usize {
        // Return the number of heights that can be reused (0 = full rebuild).
        if self.content_width != content_width {
            0 // Full rebuild needed, width changed which can affect all message's heights
        }
        else if self.heights.is_empty() {
            0 // Cache empty, nothing to reuse
        }
        else if message_count < self.message_count {
            0 // Some message was removed, full rebuild needed since we don't track individual message validity; this is expected to be rare
        }
        else if is_loading {
            if message_count == 0 {
                0
            } else {
                message_count - 1 // Currently streaming, last message height may change
            }
        }
        else {
            message_count // All heights valid
        }
    }

    /// Update cache metadata after rebuilding
    pub fn update_metadata(&mut self, message_count: usize, content_width: u16) {
        self.message_count = message_count;
        self.content_width = content_width;
    }

    /// Rebuild prefix heights (cumulative sums) for O(log n) hit testing
    pub fn rebuild_prefix_heights(&mut self) {
        // prefix_heights[i] = sum of heights[0..=i]
        // Example: heights = [3, 5, 4] → prefix_heights = [3, 8, 12]
        self.prefix_heights = self.heights.iter().scan(0u16, |acc, &h| {
            *acc += h; // update running total
            Some(*acc) // yield current total
        }).collect::<Vec<u16>>()
    }

    /// Calculate which segments are visible in the viewport (with buffer for smooth scrolling).
    /// Returns a Range of segment indices that should be rendered.
    pub fn visible_range(&self, scroll_offset: u16, viewport_height: u16) -> std::ops::Range<usize> {
        // Add buffer zone (0.5x viewport on each side = 2x total render area)
        // This handles partial visibility at boundaries and improves scroll smoothness
        let buffer = viewport_height / 2;

        let buffered_start = scroll_offset.saturating_sub(buffer);
        let buffered_end = scroll_offset.saturating_add(viewport_height).saturating_add(buffer);

        let start = self.prefix_heights.partition_point(|&end| end <= buffered_start);
        // Find items whose START is < buffered_end (not just END < buffered_end)
        // prefix_heights[i] = end of item i = start of item i+1
        // So we add 1 to include items that start before buffered_end but end after
        let end = self.prefix_heights.partition_point(|&end| end < buffered_end)
            .saturating_add(1)
            .min(self.prefix_heights.len());

        start..end
    }
}

/// TUI-specific presentation state (not part of core business logic)
pub struct TuiState {
    pub scroll_state: ScrollViewState,
    pub has_unseen_content: bool,
    pub hovered_index: Option<usize>,
    pub input_buffer: String,
    /// Cached layout measurements for rendering and hit testing
    pub layout: LayoutCache,
    /// When true, auto-scroll to bottom on new content (chat-style behavior)
    pub stick_to_bottom: bool,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            scroll_state: ScrollViewState::default(),
            has_unseen_content: false,
            hovered_index: None,
            input_buffer: String::new(),
            layout: LayoutCache::new(),
            stick_to_bottom: true, // Start at bottom
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

        // Wait for first event (with timeout for background task responsiveness)
        let first_event = poll_event();

        // Process first event + drain ALL pending events before next draw
        // This batches rapid input (like paste) into a single redraw
        let mut should_quit = false;
        for event in first_event.into_iter().chain(std::iter::from_fn(poll_event_immediate)) {
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
                    tui.stick_to_bottom = false; // User scrolled up, disable auto-scroll
                }
                TuiEvent::ScrollDown => {
                    tui.scroll_state.scroll_down();
                    // Don't re-enable stick_to_bottom here; user must press End
                }
                TuiEvent::ScrollPageUp => {
                    tui.scroll_state.scroll_page_up();
                    tui.stick_to_bottom = false;
                }
                TuiEvent::ScrollPageDown => {
                    tui.scroll_state.scroll_page_down();
                }
                TuiEvent::ScrollToBottom => {
                    tui.scroll_state.scroll_to_bottom();
                    tui.stick_to_bottom = true; // Re-enable auto-scroll
                }
                TuiEvent::MouseMove(_col, row) => {
                    let frame_area = terminal.get_frame().area();
                    let scroll_offset = tui.scroll_state.offset().y;
                    tui.hovered_index = ui::hit_test_message(
                        row,
                        frame_area,
                        scroll_offset,
                        &tui.layout.prefix_heights,
                    );
                }

                // Core events - pass to core::update
                TuiEvent::Quit => {
                    let effect = update(&mut app, Action::Quit);
                    if effect == Effect::Quit {
                        should_quit = true;
                    }
                }
                TuiEvent::Submit => {
                    // Don't allow submitting while a response is streaming
                    if app.is_loading {
                        continue;
                    }
                    let message = std::mem::take(&mut tui.input_buffer);
                    let effect = update(&mut app, Action::Submit(message));
                    if effect == Effect::SpawnRequest {
                        spawn_request(&app, tx.clone());
                    }
                }
                TuiEvent::CycleEffort => {
                    app.effort = app.effort.next();
                    app.status_message = format!("Reasoning: {}", app.effort.label());
                }
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
    let context = app.context.clone();
    let effort = app.effort;

    // Channel for the stream chunks (StreamChunk)
    let (str_tx, str_rx) = mpsc::channel();

    // Clone tx for error handling before moving tx into forwarding task
    let tx_error = tx.clone();
    let tx_forward = tx;

    // Spawn async task to drive the stream
    // Note: We don't send ResponseDone here on error - the blocking task below
    // always sends ResponseDone when the channel closes, avoiding duplicates.
    tokio::spawn(async move {
        if let Err(e) = stream_completion(&context, effort, str_tx).await {
            info!("Stream error: {}", e);
            // Send error as a content chunk; ResponseDone is sent by forwarding task
            let _ = tx_error.send(Action::ResponseChunk(format!("\n[Error: {}]", e)));
        }
    });
    tokio::task::spawn_blocking(move || {
        let mut forwarded_count = 0usize;
        let mut total_content_len = 0usize;
        while let Ok(chunk) = str_rx.recv() {
            forwarded_count += 1;
            match chunk {
                StreamChunk::Content(c) => {
                    total_content_len += c.len();
                    debug!("Forwarding Action::ResponseChunk (len={}, total={})", c.len(), total_content_len);
                    let _ = tx_forward.send(Action::ResponseChunk(c));
                }
                StreamChunk::Thinking(t) => {
                    debug!("Forwarding Action::ThinkingChunk (len={})", t.len());
                    let _ = tx_forward.send(Action::ThinkingChunk(t));
                }
            }
        }
        info!("Forwarding complete: {} actions, {} content bytes", forwarded_count, total_content_len);
        let _ = tx_forward.send(Action::ResponseDone);
    });
}