use crossterm::event::{self, Event, KeyCode};
use crate::core::action::Action;

pub fn poll_event() -> Option<Action> {
    if event::poll(std::time::Duration::from_millis(100)).unwrap() {
        if let Event::Key(key_event) = event::read().unwrap() {
            match key_event.code {
                KeyCode::Char(c) => Some(Action::InputChar(c)),
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Enter => Some(Action::Submit),
                KeyCode::Esc => Some(Action::Quit),
                KeyCode::Up => Some(Action::ScrollUp),
                KeyCode::Down => Some(Action::ScrollDown),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    }
}