use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

/// TUI-specific input events
pub enum TuiEvent {
    // Raw input events (mode system decides semantics)
    Escape,    // Esc key — mode-dependent (switch to Cursor or no-op)
    ForceQuit, // Ctrl+C — always quits regardless of mode
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
    CursorHome,      // Start of current line
    CursorEnd,       // End of current line
    CursorWordLeft,  // Alt+Left — move to previous word boundary
    CursorWordRight, // Alt+Right — move to next word boundary

    // Emacs-style editing
    DeleteWordBackward, // Ctrl+W / Alt+Backspace — delete word before cursor
    DeleteWordForward,  // Alt+D — delete word after cursor
    KillToLineStart,    // Ctrl+U — kill text from cursor to line start
    KillToLineEnd,      // Ctrl+K — kill text from cursor to line end
    Yank,               // Ctrl+Y — yank (paste) from kill buffer

    // Scrolling (message list only)
    ScrollUp,   // Mouse wheel only (arrow keys now move cursor)
    ScrollDown, // Mouse wheel only (arrow keys now move cursor)
    ScrollPageUp,
    ScrollPageDown,

    MouseMove(u16, u16),
    MouseClick(u16, u16), // Left click — col, row
    CycleEffort,          // Ctrl+R to cycle reasoning effort
    Resize,               // Terminal resized — triggers redraw
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
                log::debug!(
                    "Key event: {:?} with modifiers {:?}",
                    key_event.code,
                    key_event.modifiers
                );
                match (key_event.modifiers, key_event.code) {
                    // Force quit (always works regardless of mode)
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => Some(TuiEvent::ForceQuit),
                    // Ctrl+R cycles reasoning effort
                    (KeyModifiers::CONTROL, KeyCode::Char('r')) => Some(TuiEvent::CycleEffort),
                    // Ctrl+J inserts newline (ASCII LF; Ctrl+Enter sends this in most terminals)
                    (KeyModifiers::CONTROL, KeyCode::Char('j')) => Some(TuiEvent::InputChar('\n')),

                    // Emacs-style bindings (must precede wildcard Char arm)
                    (KeyModifiers::CONTROL, KeyCode::Char('a')) => Some(TuiEvent::CursorHome),
                    (KeyModifiers::CONTROL, KeyCode::Char('e')) => Some(TuiEvent::CursorEnd),
                    (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
                        Some(TuiEvent::DeleteWordBackward)
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('u')) => Some(TuiEvent::KillToLineStart),
                    (KeyModifiers::CONTROL, KeyCode::Char('k')) => Some(TuiEvent::KillToLineEnd),
                    (KeyModifiers::CONTROL, KeyCode::Char('y')) => Some(TuiEvent::Yank),
                    (KeyModifiers::ALT, KeyCode::Char('d')) => Some(TuiEvent::DeleteWordForward),
                    (m, KeyCode::Backspace) if m.contains(KeyModifiers::ALT) => {
                        Some(TuiEvent::DeleteWordBackward)
                    }

                    // Regular key handling
                    (_, KeyCode::Char(c)) => Some(TuiEvent::InputChar(c)),
                    (_, KeyCode::Backspace) => Some(TuiEvent::Backspace),
                    (_, KeyCode::Delete) => Some(TuiEvent::Delete),
                    // Ctrl+Enter / Shift+Enter insert newline (must precede wildcard Enter)
                    (m, KeyCode::Enter) if m.contains(KeyModifiers::CONTROL) => {
                        Some(TuiEvent::InputChar('\n'))
                    }
                    (m, KeyCode::Enter) if m.contains(KeyModifiers::SHIFT) => {
                        Some(TuiEvent::InputChar('\n'))
                    }
                    (_, KeyCode::Enter) => Some(TuiEvent::Submit),
                    (_, KeyCode::Esc) => Some(TuiEvent::Escape),
                    // Alt+Arrow for word navigation (must precede wildcard arrow arms)
                    (m, KeyCode::Left) if m.contains(KeyModifiers::ALT) => {
                        Some(TuiEvent::CursorWordLeft)
                    }
                    (m, KeyCode::Right) if m.contains(KeyModifiers::ALT) => {
                        Some(TuiEvent::CursorWordRight)
                    }
                    // Arrow keys move cursor
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
            Event::Mouse(mouse_event) => match mouse_event.kind {
                MouseEventKind::Moved => {
                    Some(TuiEvent::MouseMove(mouse_event.column, mouse_event.row))
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    Some(TuiEvent::MouseClick(mouse_event.column, mouse_event.row))
                }
                MouseEventKind::ScrollUp => Some(TuiEvent::ScrollUp),
                MouseEventKind::ScrollDown => Some(TuiEvent::ScrollDown),
                _ => None,
            },
            Event::Paste(data) => Some(TuiEvent::Paste(data)),
            Event::Resize(_, _) => Some(TuiEvent::Resize),
            _ => None,
        }
    } else {
        None
    }
}
