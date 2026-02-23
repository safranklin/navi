//! # InputBox Component
//!
//! Handles user input and displays the current reasoning effort level.
//!
//! ## Responsibilities
//!
//! - Capture text input
//! - Handle editing (backspace, delete, cursor movement, paste)
//! - Handle submission (Enter)
//! - Handle effort cycling (Ctrl+R)
//! - Display current input buffer and effort state
//!
//! ## State Management
//!
//! The buffer is internal state. The effort level is a prop from the application state.
//! Cursor position and scroll state are encapsulated in `CursorState`.

mod cursor;
mod text_wrap;

use crate::inference::Effort;
use crate::tui::component::{Component, EventHandler};
use crate::tui::event::TuiEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Paragraph};

use cursor::CursorState;
use text_wrap::{
    MAX_VISIBLE_LINES, VERTICAL_OVERHEAD, inner_width, next_char_boundary, next_word_boundary,
    prev_char_boundary, prev_word_boundary, wrap_line_count, wrap_options,
};

/// High-level events emitted by the InputBox
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// User submitted the text (Enter pressed)
    Submit(String),
    /// User requested to cycle effort level (Ctrl+R)
    CycleEffort,
    /// Text content changed (optional, if parent needs to know)
    ContentChanged,
}

/// Stores the last killed (cut) text for later yanking (paste).
///
/// Emacs-style kill commands (`Ctrl+U`, `Ctrl+K`, `Ctrl+W`) store their
/// deleted text here. `Ctrl+Y` yanks (pastes) it back.
/// Session input history with draft preservation.
///
/// When navigating with Up/Down at the input boundary, cycles through
/// previously submitted messages. The current buffer is saved as a "draft"
/// so it's restored when navigating back to the newest position.
pub(crate) struct InputHistory {
    /// Past submissions, newest last.
    entries: Vec<String>,
    /// Current navigation position: None = at draft (newest), Some(i) = viewing entries[i].
    index: Option<usize>,
    /// Saved buffer content when the user starts navigating history.
    draft: String,
}

impl InputHistory {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: None,
            draft: String::new(),
        }
    }

    /// Record a submitted message in history.
    fn push(&mut self, entry: String) {
        // Don't push duplicates of the last entry
        if self.entries.last().is_some_and(|last| last == &entry) {
            return;
        }
        self.entries.push(entry);
        self.index = None;
        self.draft.clear();
    }

    /// Navigate to the previous (older) entry. Returns the text to display,
    /// or None if there's no history to navigate to.
    fn navigate_up(&mut self, current_buffer: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        let new_index = match self.index {
            None => {
                // First navigation: save draft and go to newest entry
                self.draft = current_buffer.to_owned();
                self.entries.len() - 1
            }
            Some(0) => return None, // Already at oldest entry
            Some(i) => i - 1,
        };

        self.index = Some(new_index);
        Some(&self.entries[new_index])
    }

    /// Navigate to the next (newer) entry or back to the draft.
    /// Returns the text to display, or None if already at the draft.
    fn navigate_down(&mut self) -> Option<&str> {
        match self.index {
            None => None, // Already at draft
            Some(i) if i + 1 >= self.entries.len() => {
                // Past the newest entry → restore draft
                self.index = None;
                Some(&self.draft)
            }
            Some(i) => {
                self.index = Some(i + 1);
                Some(&self.entries[i + 1])
            }
        }
    }

    /// Reset navigation state (called on any edit to the buffer).
    fn reset_navigation(&mut self) {
        self.index = None;
        self.draft.clear();
    }

    #[cfg(test)]
    fn is_navigating(&self) -> bool {
        self.index.is_some()
    }
}

pub(crate) struct KillBuffer {
    content: String,
}

impl KillBuffer {
    fn new() -> Self {
        Self {
            content: String::new(),
        }
    }

    /// Store text into the kill buffer, replacing any previous content.
    fn store(&mut self, text: String) {
        self.content = text;
    }

    /// Retrieve the stored text for yanking.
    pub(crate) fn yank(&self) -> &str {
        &self.content
    }

    fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

/// Text input component with effort indicator.
///
/// # Props
///
/// - `effort`: Current reasoning effort level (from App state)
///
/// # State
///
/// - `buffer`: Current text being typed
/// - `cursor`: Cursor position, scroll offset, and cached width (see `CursorState`)
pub struct InputBox {
    /// Text buffer (Internal State)
    pub buffer: String,
    /// Current effort level (Prop)
    pub effort: Effort,
    /// Whether the input is visually dimmed (Prop — true in Cursor mode)
    pub dimmed: bool,
    /// Cursor and scroll tracking
    cursor: CursorState,
    /// Emacs-style kill buffer for Ctrl+U/K/W → Ctrl+Y
    pub(crate) kill_buffer: KillBuffer,
    /// Session input history for Up/Down navigation
    pub(crate) history: InputHistory,
}

impl InputBox {
    /// Pastes larger than this are replaced with a placeholder to avoid flooding the input.
    const LARGE_PASTE_THRESHOLD: usize = 1000;

    /// Create a new InputBox with initial state
    pub fn new(effort: Effort) -> Self {
        Self {
            buffer: String::new(),
            effort,
            dimmed: false,
            cursor: CursorState::new(),
            kill_buffer: KillBuffer::new(),
            history: InputHistory::new(),
        }
    }

    /// Calculate required height for current buffer content, clamped to viewport limits.
    /// Returns value in range [1 + VERTICAL_OVERHEAD, MAX_VISIBLE_LINES + VERTICAL_OVERHEAD].
    pub fn calculate_height(&self, content_width: u16) -> u16 {
        let width = inner_width(content_width);
        let content_lines = wrap_line_count(&self.buffer, width);
        let visible_lines = content_lines.min(MAX_VISIBLE_LINES);
        visible_lines + VERTICAL_OVERHEAD
    }

    /// Get the visible text based on current scroll offset.
    /// When scroll_offset > 0, only returns the visible lines.
    fn get_visible_text(&self, content_width: u16) -> String {
        if self.cursor.scroll_offset == 0 {
            return self.buffer.clone();
        }

        let width = inner_width(content_width);
        if width == 0 {
            return String::new();
        }

        let lines = textwrap::wrap(&self.buffer, wrap_options(width));

        let start = self.cursor.scroll_offset as usize;
        let end = (start + MAX_VISIBLE_LINES as usize).min(lines.len());

        lines[start..end].join("\n")
    }

    /// Render scrollbar when content exceeds visible area
    fn render_scrollbar(&self, frame: &mut Frame, area: Rect) {
        use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

        let width = inner_width(area.width);
        let total_lines = wrap_line_count(&self.buffer, width);

        if total_lines <= MAX_VISIBLE_LINES {
            return;
        }

        // ScrollbarState content_length is max scrollable position, not total items
        let max_scroll = total_lines.saturating_sub(MAX_VISIBLE_LINES);

        let mut scrollbar_state = ScrollbarState::default()
            .content_length(max_scroll as usize)
            .position(self.cursor.scroll_offset as usize);

        let scrollbar_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y + 1,
            width: 1,
            height: area.height.saturating_sub(2),
        };

        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}

impl Component for InputBox {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Modifier, Style};

        self.cursor.last_content_width = area.width;
        self.cursor.update_scroll_offset(&self.buffer, area.width);

        let title = format!("Input (Reasoning: {})", self.effort.label());
        let visible_text = self.get_visible_text(area.width);

        let mut style = Style::default().fg(ratatui::style::Color::Green);
        if self.dimmed {
            style = style.add_modifier(Modifier::DIM);
        }

        let block = Block::bordered()
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(style)
            .title(title)
            .title_style(style);

        let input = Paragraph::new(visible_text).block(block).style(style);

        frame.render_widget(input, area);
        self.render_scrollbar(frame, area);

        // Only show cursor when the input is focused (not dimmed)
        if !self.dimmed {
            let (cursor_x, cursor_y) = self.cursor.screen_pos(&self.buffer, area);
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

impl EventHandler for InputBox {
    type Event = InputEvent;

    fn handle_event(&mut self, event: &TuiEvent) -> Option<Self::Event> {
        match event {
            TuiEvent::InputChar(c) => {
                self.history.reset_navigation();
                self.buffer.insert(self.cursor.pos, *c);
                self.cursor.pos += c.len_utf8();
                Some(InputEvent::ContentChanged)
            }
            TuiEvent::Paste(text) => {
                self.history.reset_navigation();
                if text.len() > Self::LARGE_PASTE_THRESHOLD {
                    let placeholder = format!("[pasted {} chars]", text.len());
                    self.buffer.insert_str(self.cursor.pos, &placeholder);
                    self.cursor.pos += placeholder.len();
                } else {
                    self.buffer.insert_str(self.cursor.pos, text);
                    self.cursor.pos += text.len();
                }
                Some(InputEvent::ContentChanged)
            }
            TuiEvent::Backspace => {
                if self.cursor.pos > 0 {
                    self.history.reset_navigation();
                    let prev = prev_char_boundary(&self.buffer, self.cursor.pos);
                    self.buffer.drain(prev..self.cursor.pos);
                    self.cursor.pos = prev;
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::Delete => {
                if self.cursor.pos < self.buffer.len() {
                    self.history.reset_navigation();
                    let next = next_char_boundary(&self.buffer, self.cursor.pos);
                    self.buffer.drain(self.cursor.pos..next);
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::CursorLeft => {
                if self.cursor.pos > 0 {
                    self.cursor.pos = prev_char_boundary(&self.buffer, self.cursor.pos);
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::CursorRight => {
                if self.cursor.pos < self.buffer.len() {
                    self.cursor.pos = next_char_boundary(&self.buffer, self.cursor.pos);
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::CursorHome => {
                let line_start = self.buffer[..self.cursor.pos]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                (self.cursor.pos != line_start).then(|| {
                    self.cursor.pos = line_start;
                    InputEvent::ContentChanged
                })
            }
            TuiEvent::CursorEnd => {
                let line_end = self.buffer[self.cursor.pos..]
                    .find('\n')
                    .map(|i| self.cursor.pos + i)
                    .unwrap_or(self.buffer.len());
                (self.cursor.pos != line_end).then(|| {
                    self.cursor.pos = line_end;
                    InputEvent::ContentChanged
                })
            }
            TuiEvent::Submit => {
                if !self.buffer.trim().is_empty() {
                    let text = std::mem::take(&mut self.buffer);
                    self.cursor.reset();
                    self.history.push(text.clone());
                    Some(InputEvent::Submit(text))
                } else {
                    None
                }
            }
            TuiEvent::CursorUp => {
                if self
                    .cursor
                    .move_vertically(&self.buffer, -1, self.cursor.last_content_width)
                {
                    Some(InputEvent::ContentChanged)
                } else if let Some(text) = self.history.navigate_up(&self.buffer) {
                    // At top of input — navigate to older history entry
                    self.buffer = text.to_owned();
                    self.cursor.pos = self.buffer.len();
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::CursorDown => {
                if self
                    .cursor
                    .move_vertically(&self.buffer, 1, self.cursor.last_content_width)
                {
                    Some(InputEvent::ContentChanged)
                } else if let Some(text) = self.history.navigate_down() {
                    // At bottom of input — navigate to newer history entry or draft
                    self.buffer = text.to_owned();
                    self.cursor.pos = self.buffer.len();
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::CursorWordLeft => {
                let new_pos = prev_word_boundary(&self.buffer, self.cursor.pos);
                (new_pos != self.cursor.pos).then(|| {
                    self.cursor.pos = new_pos;
                    InputEvent::ContentChanged
                })
            }
            TuiEvent::CursorWordRight => {
                let new_pos = next_word_boundary(&self.buffer, self.cursor.pos);
                (new_pos != self.cursor.pos).then(|| {
                    self.cursor.pos = new_pos;
                    InputEvent::ContentChanged
                })
            }
            TuiEvent::DeleteWordBackward => {
                if self.cursor.pos > 0 {
                    self.history.reset_navigation();
                    let boundary = prev_word_boundary(&self.buffer, self.cursor.pos);
                    let killed: String = self.buffer.drain(boundary..self.cursor.pos).collect();
                    self.kill_buffer.store(killed);
                    self.cursor.pos = boundary;
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::DeleteWordForward => {
                if self.cursor.pos < self.buffer.len() {
                    self.history.reset_navigation();
                    let boundary = next_word_boundary(&self.buffer, self.cursor.pos);
                    let killed: String = self.buffer.drain(self.cursor.pos..boundary).collect();
                    self.kill_buffer.store(killed);
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::KillToLineStart => {
                if self.cursor.pos > 0 {
                    self.history.reset_navigation();
                    let line_start = self.buffer[..self.cursor.pos]
                        .rfind('\n')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let killed: String = self.buffer.drain(line_start..self.cursor.pos).collect();
                    self.kill_buffer.store(killed);
                    self.cursor.pos = line_start;
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::KillToLineEnd => {
                if self.cursor.pos < self.buffer.len() {
                    self.history.reset_navigation();
                    let line_end = self.buffer[self.cursor.pos..]
                        .find('\n')
                        .map(|i| self.cursor.pos + i)
                        .unwrap_or(self.buffer.len());
                    let killed: String = self.buffer.drain(self.cursor.pos..line_end).collect();
                    self.kill_buffer.store(killed);
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::Yank => {
                if !self.kill_buffer.is_empty() {
                    self.history.reset_navigation();
                    let text = self.kill_buffer.yank().to_owned();
                    self.buffer.insert_str(self.cursor.pos, &text);
                    self.cursor.pos += text.len();
                    Some(InputEvent::ContentChanged)
                } else {
                    None
                }
            }
            TuiEvent::CycleEffort => Some(InputEvent::CycleEffort),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_input_box_new() {
        let input = InputBox::new(Effort::Medium);
        assert!(input.buffer.is_empty());
        assert_eq!(input.effort, Effort::Medium);
    }

    #[test]
    fn test_handle_input() {
        let mut input = InputBox::new(Effort::Low);

        let res = input.handle_event(&TuiEvent::InputChar('a'));
        assert_eq!(res, Some(InputEvent::ContentChanged));
        assert_eq!(input.buffer, "a");

        let res = input.handle_event(&TuiEvent::InputChar('b'));
        assert_eq!(res, Some(InputEvent::ContentChanged));
        assert_eq!(input.buffer, "ab");

        let res = input.handle_event(&TuiEvent::Backspace);
        assert_eq!(res, Some(InputEvent::ContentChanged));
        assert_eq!(input.buffer, "a");
    }

    #[test]
    fn test_submit() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello".to_string();

        let res = input.handle_event(&TuiEvent::Submit);
        match res {
            Some(InputEvent::Submit(text)) => assert_eq!(text, "hello"),
            _ => panic!("Expected Submit event"),
        }

        assert!(
            input.buffer.is_empty(),
            "Buffer should be cleared after submit"
        );
    }

    #[test]
    fn test_cycle_effort_event() {
        let mut input = InputBox::new(Effort::Low);
        let res = input.handle_event(&TuiEvent::CycleEffort);
        assert_eq!(res, Some(InputEvent::CycleEffort));
        // InputBox handles the event emission, but doesn't change its own effort prop
        // (that happens via prop update from parent)
        assert_eq!(input.effort, Effort::Low);
    }

    // -- KillBuffer ----------------------------------------------------------

    #[test]
    fn test_kill_buffer_initially_empty() {
        let kb = KillBuffer::new();
        assert!(kb.is_empty());
        assert_eq!(kb.yank(), "");
    }

    #[test]
    fn test_kill_buffer_store_and_yank() {
        let mut kb = KillBuffer::new();
        kb.store("hello world".to_string());
        assert!(!kb.is_empty());
        assert_eq!(kb.yank(), "hello world");
    }

    #[test]
    fn test_kill_buffer_overwrite() {
        let mut kb = KillBuffer::new();
        kb.store("first".to_string());
        kb.store("second".to_string());
        assert_eq!(kb.yank(), "second");
    }

    // -- Emacs bindings -------------------------------------------------------

    #[test]
    fn test_cursor_word_left() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello world".to_string();
        input.cursor.pos = 11; // end

        input.handle_event(&TuiEvent::CursorWordLeft);
        assert_eq!(input.cursor.pos, 6); // start of "world"

        input.handle_event(&TuiEvent::CursorWordLeft);
        assert_eq!(input.cursor.pos, 0); // start of "hello"
    }

    #[test]
    fn test_cursor_word_right() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello world".to_string();
        input.cursor.pos = 0;

        input.handle_event(&TuiEvent::CursorWordRight);
        assert_eq!(input.cursor.pos, 5); // end of "hello"

        input.handle_event(&TuiEvent::CursorWordRight);
        assert_eq!(input.cursor.pos, 11); // end of "world"
    }

    #[test]
    fn test_delete_word_backward() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello world".to_string();
        input.cursor.pos = 11;

        input.handle_event(&TuiEvent::DeleteWordBackward);
        assert_eq!(input.buffer, "hello ");
        assert_eq!(input.cursor.pos, 6);
        assert_eq!(input.kill_buffer.yank(), "world");
    }

    #[test]
    fn test_delete_word_forward() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello world".to_string();
        input.cursor.pos = 0;

        input.handle_event(&TuiEvent::DeleteWordForward);
        assert_eq!(input.buffer, " world");
        assert_eq!(input.cursor.pos, 0);
        assert_eq!(input.kill_buffer.yank(), "hello");
    }

    #[test]
    fn test_kill_to_line_start() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello world".to_string();
        input.cursor.pos = 7;

        input.handle_event(&TuiEvent::KillToLineStart);
        assert_eq!(input.buffer, "orld");
        assert_eq!(input.cursor.pos, 0);
        assert_eq!(input.kill_buffer.yank(), "hello w");
    }

    #[test]
    fn test_kill_to_line_start_multiline() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "line one\nline two".to_string();
        input.cursor.pos = 14; // mid "two"

        input.handle_event(&TuiEvent::KillToLineStart);
        assert_eq!(input.buffer, "line one\ntwo");
        assert_eq!(input.cursor.pos, 9); // right after newline
        assert_eq!(input.kill_buffer.yank(), "line ");
    }

    #[test]
    fn test_kill_to_line_end() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello world".to_string();
        input.cursor.pos = 5;

        input.handle_event(&TuiEvent::KillToLineEnd);
        assert_eq!(input.buffer, "hello");
        assert_eq!(input.cursor.pos, 5);
        assert_eq!(input.kill_buffer.yank(), " world");
    }

    #[test]
    fn test_kill_to_line_end_multiline() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "line one\nline two".to_string();
        input.cursor.pos = 4; // mid "one"

        input.handle_event(&TuiEvent::KillToLineEnd);
        assert_eq!(input.buffer, "line\nline two");
        assert_eq!(input.cursor.pos, 4);
        assert_eq!(input.kill_buffer.yank(), " one");
    }

    #[test]
    fn test_yank() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello world".to_string();
        input.cursor.pos = 11;

        // Kill "world", then yank it back
        input.handle_event(&TuiEvent::DeleteWordBackward);
        assert_eq!(input.buffer, "hello ");

        input.handle_event(&TuiEvent::Yank);
        assert_eq!(input.buffer, "hello world");
        assert_eq!(input.cursor.pos, 11);
    }

    #[test]
    fn test_yank_empty_kill_buffer() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello".to_string();
        input.cursor.pos = 5;

        let result = input.handle_event(&TuiEvent::Yank);
        assert_eq!(result, None); // Nothing to yank
        assert_eq!(input.buffer, "hello");
    }

    #[test]
    fn test_kill_yank_roundtrip() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello beautiful world".to_string();
        input.cursor.pos = 15; // end of "beautiful"

        // Kill "beautiful" backwards
        input.handle_event(&TuiEvent::DeleteWordBackward);
        assert_eq!(input.buffer, "hello  world");

        // Move to start
        input.cursor.pos = 0;

        // Yank at start
        input.handle_event(&TuiEvent::Yank);
        assert_eq!(input.buffer, "beautifulhello  world");
    }

    #[test]
    fn test_word_nav_at_boundaries() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "hello".to_string();

        // CursorWordLeft at pos 0 should be no-op
        input.cursor.pos = 0;
        let result = input.handle_event(&TuiEvent::CursorWordLeft);
        assert_eq!(result, None);
        assert_eq!(input.cursor.pos, 0);

        // CursorWordRight at end should be no-op
        input.cursor.pos = 5;
        let result = input.handle_event(&TuiEvent::CursorWordRight);
        assert_eq!(result, None);
        assert_eq!(input.cursor.pos, 5);
    }

    // -- InputHistory ---------------------------------------------------------

    #[test]
    fn test_history_empty_navigation() {
        let mut history = InputHistory::new();
        assert_eq!(history.navigate_up("draft"), None);
        assert_eq!(history.navigate_down(), None);
    }

    #[test]
    fn test_history_push_and_navigate() {
        let mut history = InputHistory::new();
        history.push("first".to_string());
        history.push("second".to_string());

        assert_eq!(history.navigate_up("current"), Some("second"));
        assert_eq!(history.navigate_up("current"), Some("first"));
        assert_eq!(history.navigate_up("current"), None); // at oldest

        assert_eq!(history.navigate_down(), Some("second"));
        assert_eq!(history.navigate_down(), Some("current")); // draft restored
        assert_eq!(history.navigate_down(), None); // already at draft
    }

    #[test]
    fn test_history_draft_preservation() {
        let mut history = InputHistory::new();
        history.push("old message".to_string());

        // Start with a partially typed draft
        assert_eq!(history.navigate_up("my draft"), Some("old message"));
        // Come back to draft
        assert_eq!(history.navigate_down(), Some("my draft"));
    }

    #[test]
    fn test_history_reset_on_edit() {
        let mut history = InputHistory::new();
        history.push("entry".to_string());

        history.navigate_up("draft");
        assert!(history.is_navigating());

        history.reset_navigation();
        assert!(!history.is_navigating());
    }

    #[test]
    fn test_history_no_duplicate_pushes() {
        let mut history = InputHistory::new();
        history.push("same".to_string());
        history.push("same".to_string());

        assert_eq!(history.navigate_up(""), Some("same"));
        assert_eq!(history.navigate_up(""), None); // only one entry
    }

    #[test]
    fn test_history_integration_with_input() {
        let mut input = InputBox::new(Effort::Low);

        // Submit two messages
        input.buffer = "first message".to_string();
        input.handle_event(&TuiEvent::Submit);
        input.buffer = "second message".to_string();
        input.handle_event(&TuiEvent::Submit);

        // Type a new draft
        input.handle_event(&TuiEvent::InputChar('d'));

        // Navigate up (cursor at top of single-line input → hits history)
        input.handle_event(&TuiEvent::CursorUp);
        assert_eq!(input.buffer, "second message");

        input.handle_event(&TuiEvent::CursorUp);
        assert_eq!(input.buffer, "first message");

        // Navigate back down
        input.handle_event(&TuiEvent::CursorDown);
        assert_eq!(input.buffer, "second message");

        input.handle_event(&TuiEvent::CursorDown);
        assert_eq!(input.buffer, "d"); // draft restored
    }

    #[test]
    fn test_history_edit_resets_navigation() {
        let mut input = InputBox::new(Effort::Low);
        input.buffer = "message".to_string();
        input.handle_event(&TuiEvent::Submit);

        // Navigate to history
        input.handle_event(&TuiEvent::CursorUp);
        assert_eq!(input.buffer, "message");

        // Edit resets navigation
        input.handle_event(&TuiEvent::InputChar('x'));
        assert!(!input.history.is_navigating());
    }

    // -- Paste improvements ---------------------------------------------------

    #[test]
    fn test_large_paste_shows_placeholder() {
        let mut input = InputBox::new(Effort::Low);
        let large_text = "x".repeat(1500);

        input.handle_event(&TuiEvent::Paste(large_text));
        assert_eq!(input.buffer, "[pasted 1500 chars]");
    }

    #[test]
    fn test_normal_paste_inserts_text() {
        let mut input = InputBox::new(Effort::Low);
        input.handle_event(&TuiEvent::Paste("hello world".to_string()));
        assert_eq!(input.buffer, "hello world");
    }

    // -- Rendering -----------------------------------------------------------

    #[test]
    fn test_render_shows_effort() {
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut input = InputBox::new(Effort::High);

        terminal
            .draw(|f| {
                input.render(f, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();

        assert!(text.contains("Reasoning: High"));
    }
}
