//! # Session Manager Component
//!
//! Full-screen overlay for browsing, loading, and deleting saved sessions.
//! Opened with Ctrl+O, dismissed with Esc.
//!
//! Follows the persistent state + transient wrapper pattern:
//! - `SessionManagerState` lives in `TuiState`
//! - `SessionManager` is created each frame with borrowed state

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

use crate::core::session::SessionMeta;
use crate::tui::event::TuiEvent;

/// Persistent state for the session manager overlay.
pub struct SessionManagerState {
    pub sessions: Vec<SessionMeta>,
    pub selected: usize,
    pub confirm_delete: bool,
    pub list_state: ListState,
}

impl SessionManagerState {
    pub fn new(sessions: Vec<SessionMeta>) -> Self {
        let mut list_state = ListState::default();
        if !sessions.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            sessions,
            selected: 0,
            confirm_delete: false,
            list_state,
        }
    }

    /// Handle a key event, returning a SessionEvent if the overlay should act.
    pub fn handle_event(&mut self, event: &TuiEvent) -> Option<SessionEvent> {
        // Reset delete confirmation on any non-delete key
        let is_delete_key = matches!(event, TuiEvent::InputChar('d'));
        if !is_delete_key {
            self.confirm_delete = false;
        }

        match event {
            TuiEvent::Escape => Some(SessionEvent::Dismiss),
            TuiEvent::CursorUp => {
                if !self.sessions.is_empty() {
                    self.selected = self.selected.saturating_sub(1);
                    self.list_state.select(Some(self.selected));
                }
                None
            }
            TuiEvent::CursorDown => {
                if !self.sessions.is_empty() {
                    self.selected = (self.selected + 1).min(self.sessions.len() - 1);
                    self.list_state.select(Some(self.selected));
                }
                None
            }
            TuiEvent::Submit => self
                .sessions
                .get(self.selected)
                .map(|session| SessionEvent::Load(session.id.clone())),
            TuiEvent::InputChar('n') => Some(SessionEvent::CreateNew),
            TuiEvent::InputChar('d') => {
                if self.sessions.is_empty() {
                    return None;
                }
                if self.confirm_delete {
                    let id = self.sessions[self.selected].id.clone();
                    self.confirm_delete = false;
                    Some(SessionEvent::Delete(id))
                } else {
                    self.confirm_delete = true;
                    None
                }
            }
            _ => None,
        }
    }

    /// Remove a session from the local list after deletion.
    pub fn remove_session(&mut self, id: &str) {
        self.sessions.retain(|s| s.id != id);
        if self.sessions.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else {
            self.selected = self.selected.min(self.sessions.len() - 1);
            self.list_state.select(Some(self.selected));
        }
    }
}

/// Events emitted by the session manager.
pub enum SessionEvent {
    Load(String),
    CreateNew,
    Delete(String),
    Dismiss,
}

/// Transient render wrapper for the session manager overlay.
pub struct SessionManager<'a> {
    state: &'a mut SessionManagerState,
}

impl<'a> SessionManager<'a> {
    pub fn new(state: &'a mut SessionManagerState) -> Self {
        Self { state }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Center the overlay (80% width, 60% height, clamped)
        let overlay = centered_rect(80, 70, area);

        // Clear underlying content
        frame.render_widget(Clear, overlay);

        // Help bar text
        let help_text = if self.state.confirm_delete {
            " Press d again to confirm delete | Esc Cancel "
        } else {
            " n New  d Delete  Enter Open  Esc Back "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Sessions ")
            .title_alignment(Alignment::Left)
            .title_bottom(Line::from(help_text).centered())
            .padding(Padding::horizontal(1));

        if self.state.sessions.is_empty() {
            let empty = Paragraph::new("No saved sessions.")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center)
                .block(block);
            frame.render_widget(empty, overlay);
            return;
        }

        // Build list items
        let items: Vec<ListItem> = self
            .state
            .sessions
            .iter()
            .enumerate()
            .map(|(i, session)| {
                let date = format_timestamp(session.updated_at);
                let count = format!("{} msgs", session.message_count);

                // Calculate available space for title
                // Layout: "  Jan 15  <title>   12 msgs  "
                let inner_width = overlay.width.saturating_sub(4) as usize; // borders + padding
                let fixed_width = date.len() + 2 + count.len() + 2; // date + gaps + count
                let title_width = inner_width.saturating_sub(fixed_width);
                let title = truncate_str(&session.title, title_width);

                // Pad title to fill available space
                let padded_title = format!("{:<width$}", title, width = title_width);

                let style = if i == self.state.selected {
                    if self.state.confirm_delete {
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    }
                } else {
                    Style::default().fg(Color::Gray)
                };

                let line = Line::from(vec![
                    Span::styled(date, style),
                    Span::styled("  ", style),
                    Span::styled(padded_title, style),
                    Span::styled("  ", style),
                    Span::styled(count, style),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items).block(block);

        frame.render_stateful_widget(list, overlay, &mut self.state.list_state);
    }
}

/// Format a Unix timestamp as "Jan 15" style date.
fn format_timestamp(ts: i64) -> String {
    use chrono::{DateTime, Local, Utc};
    let dt: DateTime<Local> = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_default()
        .with_timezone(&Local);
    dt.format("%b %d").to_string()
}

/// Truncate a string to fit within `max_width` chars, adding "..." if needed.
fn truncate_str(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        ".".repeat(max_width)
    } else {
        format!("{}...", &s[..max_width - 3])
    }
}

/// Compute a centered rect using percentage of the outer rect.
fn centered_rect(percent_x: u16, percent_y: u16, outer: Rect) -> Rect {
    let [_, center_v, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(outer);
    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(center_v);
    center
}
