use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};

/// TUI-specific input events
pub enum TuiEvent {
    // Core actions (passed to core::update)
    Quit,
    Submit,

    // TUI-local events (handled directly in TUI)
    InputChar(char),
    Paste(String), // Bracketed paste - preserves newlines
    Backspace,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToBottom, // End key - also re-enables stick-to-bottom
    MouseMove(u16, u16),
    CycleEffort, // Ctrl+R to cycle reasoning effort
}

/// Poll for an event with timeout (blocks up to 100ms)
pub fn poll_event() -> Option<TuiEvent> {
    poll_event_timeout(std::time::Duration::from_millis(100))
}

/// Poll for an event without blocking (returns immediately)
pub fn poll_event_immediate() -> Option<TuiEvent> {
    poll_event_timeout(std::time::Duration::ZERO)
}

fn poll_event_timeout(timeout: std::time::Duration) -> Option<TuiEvent> {
    if event::poll(timeout).unwrap() {
        match event::read().unwrap() {
            Event::Key(key_event) => {
                // Debug: log all key events to see what the terminal sends
                log::debug!("Key event: {:?} with modifiers {:?}", key_event.code, key_event.modifiers);
                match (key_event.modifiers, key_event.code) {
                    // Ctrl+R cycles reasoning effort
                    (KeyModifiers::CONTROL, KeyCode::Char('r')) => Some(TuiEvent::CycleEffort),
                    // Ctrl+J inserts newline (ASCII LF; Ctrl+Enter sends this in most terminals)
                    (KeyModifiers::CONTROL, KeyCode::Char('j')) => Some(TuiEvent::InputChar('\n')),
                    // Regular key handling
                    (_, KeyCode::Char(c)) => Some(TuiEvent::InputChar(c)),
                    (_, KeyCode::Backspace) => Some(TuiEvent::Backspace),
                    (_, KeyCode::Enter) => Some(TuiEvent::Submit),
                    (_, KeyCode::Esc) => Some(TuiEvent::Quit),
                    (_, KeyCode::Up) => Some(TuiEvent::ScrollUp),
                    (_, KeyCode::Down) => Some(TuiEvent::ScrollDown),
                    (_, KeyCode::PageUp) => Some(TuiEvent::ScrollPageUp),
                    (_, KeyCode::PageDown) => Some(TuiEvent::ScrollPageDown),
                    (_, KeyCode::End) => Some(TuiEvent::ScrollToBottom),
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
            Event::Paste(data) => Some(TuiEvent::Paste(data)),
            _ => None,
        }
    } else {
        None
    }
}