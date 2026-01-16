use crossterm::event::{self, Event, KeyCode, MouseEventKind};

/// TUI-specific input events
pub enum TuiEvent {
    // Core actions (passed to core::update)
    Quit,
    Submit,

    // TUI-local events (handled directly in TUI)
    InputChar(char),
    Backspace,
    ScrollUp,
    ScrollDown,
    MouseMove(u16, u16),
}

pub fn poll_event() -> Option<TuiEvent> {
    if event::poll(std::time::Duration::from_millis(100)).unwrap() {
        match event::read().unwrap() {
            Event::Key(key_event) => {
                match key_event.code {
                    KeyCode::Char(c) => Some(TuiEvent::InputChar(c)),
                    KeyCode::Backspace => Some(TuiEvent::Backspace),
                    KeyCode::Enter => Some(TuiEvent::Submit),
                    KeyCode::Esc => Some(TuiEvent::Quit),
                    KeyCode::Up => Some(TuiEvent::ScrollUp),
                    KeyCode::Down => Some(TuiEvent::ScrollDown),
                    _ => None,
                }
            }
            Event::Mouse(mouse_event) => {
                match mouse_event.kind {
                    MouseEventKind::Moved => {
                        Some(TuiEvent::MouseMove(mouse_event.column, mouse_event.row))
                    }
                    MouseEventKind::ScrollUp => Some(TuiEvent::ScrollUp),
                    MouseEventKind::ScrollDown => Some(TuiEvent::ScrollDown),
                    _ => None,
                }
            }
            _ => None,
        }
    } else {
        None
    }
}