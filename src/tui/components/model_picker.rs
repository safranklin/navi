//! # Model Picker Component
//!
//! Full-screen overlay for switching models at runtime. Opened with Ctrl+M.
//! Supports cross-provider switching (OpenRouter ↔ LM Studio).
//!
//! Follows the persistent state + transient wrapper pattern:
//! - `ModelPickerState` lives in `TuiState`
//! - `ModelPicker` is created each frame with borrowed state

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph};
use ratatui::Frame;

use crate::core::config::ModelEntry;
use crate::tui::event::TuiEvent;

/// Persistent state for the model picker overlay.
pub struct ModelPickerState {
    pub models: Vec<ModelEntry>,
    pub selected: usize,
    pub list_state: ListState,
}

impl ModelPickerState {
    pub fn new(models: Vec<ModelEntry>) -> Self {
        let mut list_state = ListState::default();
        if !models.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            models,
            selected: 0,
            list_state,
        }
    }

    /// Handle a key event, returning a ModelPickerEvent if the overlay should act.
    pub fn handle_event(&mut self, event: &TuiEvent) -> Option<ModelPickerEvent> {
        match event {
            TuiEvent::Escape => Some(ModelPickerEvent::Dismiss),
            TuiEvent::CursorUp => {
                if !self.models.is_empty() {
                    self.selected = self.selected.saturating_sub(1);
                    self.list_state.select(Some(self.selected));
                }
                None
            }
            TuiEvent::CursorDown => {
                if !self.models.is_empty() {
                    self.selected = (self.selected + 1).min(self.models.len() - 1);
                    self.list_state.select(Some(self.selected));
                }
                None
            }
            TuiEvent::Submit => self
                .models
                .get(self.selected)
                .map(|model| ModelPickerEvent::Select(model.clone())),
            _ => None,
        }
    }
}

/// Events emitted by the model picker.
pub enum ModelPickerEvent {
    Select(ModelEntry),
    Dismiss,
}

/// Transient render wrapper for the model picker overlay.
pub struct ModelPicker<'a> {
    state: &'a mut ModelPickerState,
    current_model: &'a str,
}

impl<'a> ModelPicker<'a> {
    pub fn new(state: &'a mut ModelPickerState, current_model: &'a str) -> Self {
        Self {
            state,
            current_model,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let overlay = centered_rect(70, 60, area);

        // Clear underlying content
        frame.render_widget(Clear, overlay);

        let help_text = " Enter Select  Esc Back ";

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Models ")
            .title_alignment(Alignment::Left)
            .title_bottom(Line::from(help_text).centered())
            .padding(Padding::horizontal(1));

        if self.state.models.is_empty() {
            let empty = Paragraph::new(
                "No models configured.\nAdd [[models]] entries to ~/.navi/config.toml",
            )
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(block);
            frame.render_widget(empty, overlay);
            return;
        }

        // Build list items
        let items: Vec<ListItem> = self
            .state
            .models
            .iter()
            .enumerate()
            .map(|(i, model)| {
                let is_active = model.name == self.current_model;
                let provider_tag = format!("[{}]", model.provider);
                let active_marker = if is_active { " *" } else { "" };

                // Calculate available space for model name
                let inner_width = overlay.width.saturating_sub(4) as usize; // borders + padding
                let fixed_width = provider_tag.len() + 2 + active_marker.len();
                let desc = model
                    .description
                    .as_deref()
                    .map(|d| format!("  {d}"))
                    .unwrap_or_default();
                let name_width = inner_width
                    .saturating_sub(fixed_width)
                    .saturating_sub(desc.len());
                let name = truncate_str(&model.name, name_width);
                let padded_name = format!("{:<width$}", name, width = name_width);

                let style = if i == self.state.selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else if is_active {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Gray)
                };

                let provider_color = match model.provider.as_str() {
                    "lmstudio" => Color::Green,
                    _ => Color::Yellow,
                };

                let mut spans = vec![
                    Span::styled(
                        provider_tag,
                        if i == self.state.selected {
                            style
                        } else {
                            Style::default().fg(provider_color)
                        },
                    ),
                    Span::styled("  ", style),
                    Span::styled(padded_name, style),
                ];

                if !desc.is_empty() {
                    spans.push(Span::styled(
                        desc,
                        if i == self.state.selected {
                            style
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ));
                }

                if !active_marker.is_empty() {
                    spans.push(Span::styled(active_marker, style));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items).block(block);

        frame.render_stateful_widget(list, overlay, &mut self.state.list_state);
    }
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
