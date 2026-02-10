use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};

/// TUI-specific input events
pub enum TuiEvent {
    // Core actions (passed to core::update)
    Quit,
    Submit,

    // TUI-local events (handled directly in TUI)
    InputChar(char),
    Paste(String), // Bracketed paste - preserves newlines
    Backspace,
    Delete, // Delete character after cursor

    // Cursor movement
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
    CursorHome, // Start of current line
    CursorEnd,  // End of current line

    // Scrolling (message list only)
    ScrollUp,   // Mouse wheel only (arrow keys now move cursor)
    ScrollDown, // Mouse wheel only (arrow keys now move cursor)
    ScrollPageUp,
    ScrollPageDown,

    MouseMove(u16, u16),
    CycleEffort, // Ctrl+R to cycle reasoning effort
    Resize,      // Terminal resized — triggers redraw
}

/// Poll for an event without blocking (returns immediately)
pub fn poll_event_immediate() -> Option<TuiEvent> {
    poll_event_timeout(std::time::Duration::ZERO)
}

/// Poll for an event with a caller-specified timeout.
///
/// The main loop uses a dynamic timeout: short (~80ms) during animation for
/// responsive frame updates, long (~500ms) when idle to reduce CPU wakeups.
pub fn poll_event_timeout(timeout: std::time::Duration) -> Option<TuiEvent> {
    if event::poll(timeout).ok()? {
        match event::read().ok()? {
            Event::Key(key_event) => {
                if key_event.kind != KeyEventKind::Press {
                    return None;
                }
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
                    (_, KeyCode::Delete) => Some(TuiEvent::Delete),
                    (_, KeyCode::Enter) => Some(TuiEvent::Submit),
                    (_, KeyCode::Esc) => Some(TuiEvent::Quit),
                    // Arrow keys now move cursor (not scroll)
                    (_, KeyCode::Left) => Some(TuiEvent::CursorLeft),
                    (_, KeyCode::Right) => Some(TuiEvent::CursorRight),
                    (_, KeyCode::Up) => Some(TuiEvent::CursorUp),
                    (_, KeyCode::Down) => Some(TuiEvent::CursorDown),
                    (_, KeyCode::Home) => Some(TuiEvent::CursorHome),
                    (_, KeyCode::End) => Some(TuiEvent::CursorEnd),
                    // Page Up/Down for scrolling messages
                    (_, KeyCode::PageUp) => Some(TuiEvent::ScrollPageUp),
                    (_, KeyCode::PageDown) => Some(TuiEvent::ScrollPageDown),
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
            Event::Resize(_, _) => Some(TuiEvent::Resize),
            _ => None,
        }
    } else {
        None
    }
}