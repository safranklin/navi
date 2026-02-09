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

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Paragraph};
use crate::inference::Effort;
use crate::tui::component::{Component, EventHandler};
use crate::tui::event::TuiEvent;

use cursor::CursorState;
use text_wrap::{
    inner_width, wrap_options, wrap_line_count,
    prev_char_boundary, next_char_boundary,
    VERTICAL_OVERHEAD, MAX_VISIBLE_LINES
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
    /// Cursor and scroll tracking
    cursor: CursorState,
}

impl InputBox {
    /// Create a new InputBox with initial state
    pub fn new(effort: Effort) -> Self {
        Self {
            buffer: String::new(),
            effort,
            cursor: CursorState::new(),
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
        self.cursor.last_content_width = area.width;
        self.cursor.update_scroll_offset(&self.buffer, area.width);

        let title = format!("Input (Reasoning: {})", self.effort.label());
        let visible_text = self.get_visible_text(area.width);

        let block = Block::bordered()
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(title);

        let input = Paragraph::new(visible_text)
            .block(block)
            .style(ratatui::style::Style::default().fg(ratatui::style::Color::Green));

        frame.render_widget(input, area);
        self.render_scrollbar(frame, area);

        let (cursor_x, cursor_y) = self.cursor.screen_pos(&self.buffer, area);
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

impl EventHandler for InputBox {
    type Event = InputEvent;

    fn handle_event(&mut self, event: &TuiEvent) -> Option<Self::Event> {
        match event {
            TuiEvent::InputChar(c) => {
                self.buffer.insert(self.cursor.pos, *c);
                self.cursor.pos += c.len_utf8();
                Some(InputEvent::ContentChanged)
            }
            TuiEvent::Paste(text) => {
                self.buffer.insert_str(self.cursor.pos, text);
                self.cursor.pos += text.len();
                Some(InputEvent::ContentChanged)
            }
            TuiEvent::Backspace => {
                if self.cursor.pos > 0 {
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
                    Some(InputEvent::Submit(text))
                } else {
                    None
                }
            }
            TuiEvent::CursorUp => {
                self.cursor.move_vertically(&self.buffer, -1, self.cursor.last_content_width)
                    .then_some(InputEvent::ContentChanged)
            }
            TuiEvent::CursorDown => {
                self.cursor.move_vertically(&self.buffer, 1, self.cursor.last_content_width)
                    .then_some(InputEvent::ContentChanged)
            }
            TuiEvent::CycleEffort => Some(InputEvent::CycleEffort),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

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

        assert!(input.buffer.is_empty(), "Buffer should be cleared after submit");
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

    #[test]
    fn test_render_shows_effort() {
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut input = InputBox::new(Effort::High);

        terminal.draw(|f| {
            input.render(f, f.area());
        }).unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer.content().iter().map(|c| c.symbol()).collect::<String>();

        assert!(text.contains("Reasoning: High"));
    }
}
