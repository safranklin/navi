//! # InputBox Component
//!
//! Handles user input and displays the current reasoning effort level.
//!
//! ## Responsibilities
//!
//! - Capture text input
//! - Handle basic editing (backspace, paste)
//! - Handle submission (Enter)
//! - Handle effort cycling (Ctrl+R)
//! - Display current input buffer and effort state
//!
//! ## State Management
//!
//! The buffer is internal state. The effort level is a prop from the application state.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Paragraph};
use crate::inference::Effort;
use crate::tui::component::{Component, EventHandler};
use crate::tui::event::TuiEvent;

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
pub struct InputBox {
    /// Text buffer (Internal State)
    pub buffer: String,
    /// Current effort level (Prop)
    pub effort: Effort,
}

impl InputBox {
    /// Create a new InputBox with initial state
    pub fn new(effort: Effort) -> Self {
        Self {
            buffer: String::new(),
            effort,
        }
    }
}

impl Component for InputBox {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Show effort in the title or separately?
        // Current design (ui.rs) showed "Input" title.
        // We can improve it to "Input (Effort: Medium)" or similar.
        let title = format!("Input (Reasoning: {})", self.effort.label());
        
        let input = Paragraph::new(self.buffer.as_str())
            .block(Block::bordered().title(title));
        
        frame.render_widget(input, area);
    }
}

impl EventHandler for InputBox {
    type Event = InputEvent;

    fn handle_event(&mut self, event: &TuiEvent) -> Option<Self::Event> {
        match event {
            TuiEvent::InputChar(c) => {
                self.buffer.push(*c);
                Some(InputEvent::ContentChanged)
            }
            TuiEvent::Paste(text) => {
                self.buffer.push_str(text);
                Some(InputEvent::ContentChanged)
            }
            TuiEvent::Backspace => {
                self.buffer.pop();
                Some(InputEvent::ContentChanged)
            }
            TuiEvent::Submit => {
                if !self.buffer.trim().is_empty() {
                    let text = std::mem::take(&mut self.buffer);
                    Some(InputEvent::Submit(text))
                } else {
                    None
                }
            }
            TuiEvent::CycleEffort => {
                Some(InputEvent::CycleEffort)
            }
            // Ignore other events (scroll, mouse, etc.)
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
