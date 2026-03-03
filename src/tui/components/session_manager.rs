//! # Session Manager Component
//!
//! Full-screen overlay for browsing, loading, and deleting saved sessions.
//! Opened with Ctrl+O, dismissed with Esc.
//!
//! Follows the persistent state + transient wrapper pattern:
//! - `SessionManagerState` lives in `TuiState`
//! - `SessionManager` is created each frame with borrowed state

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph};

use crate::core::session::SessionMeta;
use crate::tui::event::TuiEvent;

/// Inline rename editing state.
pub struct RenameState {
    pub buffer: String,
    pub cursor: usize,
}

/// Persistent state for the session manager overlay.
pub struct SessionManagerState {
    pub sessions: Vec<SessionMeta>,
    pub selected: usize,
    pub confirm_delete: bool,
    pub list_state: ListState,
    pub rename: Option<RenameState>,
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
            rename: None,
        }
    }

    /// Handle a key event, returning a SessionEvent if the overlay should act.
    pub fn handle_event(&mut self, event: &TuiEvent) -> Option<SessionEvent> {
        // Rename mode intercepts all input
        if let Some(ref mut rs) = self.rename {
            match event {
                TuiEvent::Escape => {
                    self.rename = None;
                }
                TuiEvent::Submit => {
                    let new_title = rs.buffer.trim().to_string();
                    if !new_title.is_empty() {
                        let id = self.sessions[self.selected].id.clone();
                        self.sessions[self.selected].title = new_title.clone();
                        self.rename = None;
                        return Some(SessionEvent::Rename {
                            id,
                            new_title,
                        });
                    }
                    self.rename = None;
                }
                TuiEvent::InputChar(ch) => {
                    rs.buffer.insert(rs.cursor, *ch);
                    rs.cursor += ch.len_utf8();
                }
                TuiEvent::Backspace => {
                    if rs.cursor > 0 {
                        // Find previous char boundary
                        let prev = rs.buffer[..rs.cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        rs.buffer.drain(prev..rs.cursor);
                        rs.cursor = prev;
                    }
                }
                TuiEvent::CursorLeft => {
                    if rs.cursor > 0 {
                        rs.cursor = rs.buffer[..rs.cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                }
                TuiEvent::CursorRight => {
                    if rs.cursor < rs.buffer.len() {
                        rs.cursor += rs.buffer[rs.cursor..].chars().next().map_or(0, |c| c.len_utf8());
                    }
                }
                _ => {}
            }
            return None;
        }

        // Normal mode
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
            TuiEvent::InputChar('r') => {
                if !self.sessions.is_empty() {
                    let title = self.sessions[self.selected].title.clone();
                    let cursor = title.len();
                    self.rename = Some(RenameState {
                        buffer: title,
                        cursor,
                    });
                }
                None
            }
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
    Rename { id: String, new_title: String },
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
        let help_text = if self.state.rename.is_some() {
            " Enter Save  Esc Cancel "
        } else if self.state.confirm_delete {
            " Press d again to confirm delete | Esc Cancel "
        } else {
            " n New  r Rename  d Delete  Enter Open  Esc Back "
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

        // Calculate inner dimensions for title column
        let inner_width = overlay.width.saturating_sub(4) as usize; // borders + padding

        // Track cursor position for rename mode
        let mut rename_cursor_pos: Option<(u16, u16)> = None;

        // Build list items
        let items: Vec<ListItem> = self
            .state
            .sessions
            .iter()
            .enumerate()
            .map(|(i, session)| {
                let date = format_timestamp(session.updated_at);
                let count = format!("{} msgs", session.message_count);

                // Layout: "  Jan 15  <title>   12 msgs  "
                let fixed_width = date.len() + 2 + count.len() + 2; // date + gaps + count
                let title_width = inner_width.saturating_sub(fixed_width);

                let is_renaming = i == self.state.selected && self.state.rename.is_some();

                let (padded_title, style) = if is_renaming {
                    let rs = self.state.rename.as_ref().unwrap();
                    let display = truncate_str(&rs.buffer, title_width);
                    let padded = format!("{:<width$}", display, width = title_width);

                    // Compute cursor screen position:
                    // overlay.x + 1(border) + 1(padding) + date.len() + 2(gap) + cursor_in_title
                    let cursor_in_title = rs.cursor.min(title_width);
                    let cursor_x = overlay.x + 2 + date.len() as u16 + 2 + cursor_in_title as u16;
                    // overlay.y + 1(border) + row index (relative to list scroll)
                    let visible_row = i.saturating_sub(
                        self.state.list_state.offset(),
                    );
                    let cursor_y = overlay.y + 1 + visible_row as u16;
                    rename_cursor_pos = Some((cursor_x, cursor_y));

                    let style = Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED);
                    (padded, style)
                } else {
                    let title = truncate_str(&session.title, title_width);
                    let padded = format!("{:<width$}", title, width = title_width);

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
                    (padded, style)
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

        // Show cursor when renaming
        if let Some((cx, cy)) = rename_cursor_pos {
            frame.set_cursor_position((cx, cy));
        }
    }
}

/// Format a Unix timestamp as "Jan 15 14:30" style date+time.
fn format_timestamp(ts: i64) -> String {
    use chrono::{DateTime, Local, Utc};
    let dt: DateTime<Local> = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_default()
        .with_timezone(&Local);
    dt.format("%b %d %H:%M").to_string()
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
