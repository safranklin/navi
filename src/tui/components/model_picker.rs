//! # Model Picker Component
//!
//! Full-screen overlay for switching models at runtime. Opened with Ctrl+M.
//! Supports cross-provider switching (OpenRouter ↔ LM Studio).
//!
//! Models come from two sources:
//! - **Pinned models**: from config `[[models]]` entries, shown instantly
//! - **Fetched models**: from provider APIs, loaded async after picker opens
//!
//! Includes a search bar that filters by substring match across name,
//! provider, and description (case-insensitive).

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph};

use crate::core::config::ModelEntry;
use crate::tui::event::TuiEvent;

// ============================================================================
// State
// ============================================================================

/// Async fetch status for dynamically discovered models.
#[derive(Debug)]
pub enum FetchStatus {
    Loading,
    Loaded,
}

/// Persistent state for the model picker overlay.
pub struct ModelPickerState {
    /// Models from config `[[models]]` entries — shown immediately.
    pub pinned_models: Vec<ModelEntry>,
    /// Models fetched from provider APIs — populated async.
    pub fetched_models: Vec<ModelEntry>,
    /// Current fetch status.
    pub fetch_status: FetchStatus,
    /// Search query typed by the user.
    pub query: String,
    /// Indices into the combined model list that match the current query.
    /// Combined list: `0..pinned.len()` = pinned, `pinned.len()..` = fetched.
    pub filtered_indices: Vec<usize>,
    /// Selection index within `filtered_indices`.
    pub selected: usize,
    /// Ratatui list widget state (tracks visible selection).
    pub list_state: ListState,
}

impl ModelPickerState {
    pub fn new(pinned_models: Vec<ModelEntry>) -> Self {
        let total = pinned_models.len();
        let filtered_indices: Vec<usize> = (0..total).collect();
        let mut list_state = ListState::default();
        if !filtered_indices.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            pinned_models,
            fetched_models: Vec::new(),
            fetch_status: FetchStatus::Loading,
            query: String::new(),
            filtered_indices,
            selected: 0,
            list_state,
        }
    }

    /// Returns the total number of models in the combined list.
    fn total_models(&self) -> usize {
        self.pinned_models.len() + self.fetched_models.len()
    }

    /// Returns a reference to a model by its combined-list index.
    fn get_model(&self, combined_idx: usize) -> Option<&ModelEntry> {
        let pinned_len = self.pinned_models.len();
        if combined_idx < pinned_len {
            self.pinned_models.get(combined_idx)
        } else {
            self.fetched_models.get(combined_idx - pinned_len)
        }
    }

    /// Called when async fetch completes. Deduplicates against pinned models.
    pub fn set_fetched_models(&mut self, models: Vec<ModelEntry>) {
        // Dedup: exclude any fetched model whose name+provider matches a pinned model
        self.fetched_models = models
            .into_iter()
            .filter(|m| !self.pinned_models.contains(m))
            .collect();
        self.fetch_status = FetchStatus::Loaded;
        self.rebuild_filter();
    }

    /// Rebuilds `filtered_indices` based on the current query.
    /// Linear scan over all models — fine for ~300 models.
    fn rebuild_filter(&mut self) {
        let query_lower = self.query.to_lowercase();
        self.filtered_indices = (0..self.total_models())
            .filter(|&i| {
                if query_lower.is_empty() {
                    return true;
                }
                let Some(model) = self.get_model(i) else {
                    return false;
                };
                model.name.to_lowercase().contains(&query_lower)
                    || model.provider.to_lowercase().contains(&query_lower)
                    || model
                        .description
                        .as_deref()
                        .is_some_and(|d| d.to_lowercase().contains(&query_lower))
            })
            .collect();

        // Reset selection to 0 (or None if empty)
        self.selected = 0;
        if self.filtered_indices.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
    }

    /// Handle a key event, returning a ModelPickerEvent if the overlay should act.
    pub fn handle_event(&mut self, event: &TuiEvent) -> Option<ModelPickerEvent> {
        match event {
            TuiEvent::Escape => {
                // First Escape clears search; second dismisses picker
                if !self.query.is_empty() {
                    self.query.clear();
                    self.rebuild_filter();
                    None
                } else {
                    Some(ModelPickerEvent::Dismiss)
                }
            }
            TuiEvent::CursorUp => {
                if !self.filtered_indices.is_empty() {
                    self.selected = self.selected.saturating_sub(1);
                    self.list_state.select(Some(self.selected));
                }
                None
            }
            TuiEvent::CursorDown => {
                if !self.filtered_indices.is_empty() {
                    self.selected = (self.selected + 1).min(self.filtered_indices.len() - 1);
                    self.list_state.select(Some(self.selected));
                }
                None
            }
            TuiEvent::Submit => {
                let combined_idx = self.filtered_indices.get(self.selected)?;
                self.get_model(*combined_idx)
                    .map(|model| ModelPickerEvent::Select(model.clone()))
            }
            TuiEvent::InputChar(c) => {
                self.query.push(*c);
                self.rebuild_filter();
                None
            }
            TuiEvent::Backspace => {
                self.query.pop();
                self.rebuild_filter();
                None
            }
            _ => None,
        }
    }
}

/// Events emitted by the model picker.
pub enum ModelPickerEvent {
    Select(ModelEntry),
    Dismiss,
}

// ============================================================================
// Render
// ============================================================================

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

        let help_text = " Enter Select  Esc Back  Type to search ";

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Models ")
            .title_alignment(Alignment::Left)
            .title_bottom(Line::from(help_text).centered())
            .padding(Padding::horizontal(1));

        let inner = block.inner(overlay);
        frame.render_widget(block, overlay);

        // Layout: search bar, separator, model list
        let [search_area, _sep_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(inner);

        // Search bar
        let search_display = if self.state.query.is_empty() {
            Span::styled("Type to filter...", Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(
                format!("> {}_", self.state.query),
                Style::default().fg(Color::White),
            )
        };
        let search_line = if !self.state.query.is_empty() {
            Line::from(search_display)
        } else {
            Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::DarkGray)),
                search_display,
            ])
        };
        frame.render_widget(Paragraph::new(search_line), search_area);

        // Separator
        let sep = Paragraph::new("─".repeat(inner.width as usize))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(sep, _sep_area);

        // Empty state
        if self.state.filtered_indices.is_empty() {
            let msg = if self.state.total_models() == 0
                && matches!(self.state.fetch_status, FetchStatus::Loading)
            {
                "Fetching models..."
            } else if !self.state.query.is_empty() {
                "No models match your search."
            } else {
                "No models available.\nAdd [[models]] entries to ~/.navi/config.toml"
            };
            let empty = Paragraph::new(msg)
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(empty, list_area);
            return;
        }

        // Build list items from filtered indices
        let mut items: Vec<ListItem> = self
            .state
            .filtered_indices
            .iter()
            .enumerate()
            .filter_map(|(display_idx, &combined_idx)| {
                let model = self.state.get_model(combined_idx)?;
                let is_active = model.name == self.current_model;
                let is_pinned = combined_idx < self.state.pinned_models.len();
                let provider_tag = format!("[{}]", model.provider);
                let active_marker = if is_active { " *" } else { "" };
                let pin_marker = if is_pinned { " " } else { "" };

                // Calculate available space for model name
                let inner_width = list_area.width as usize;
                let fixed_width = provider_tag.len() + 2 + active_marker.len() + pin_marker.len();
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

                let style = if display_idx == self.state.selected {
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
                        if display_idx == self.state.selected {
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
                        if display_idx == self.state.selected {
                            style
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ));
                }

                if !active_marker.is_empty() {
                    spans.push(Span::styled(active_marker, style));
                }

                Some(ListItem::new(Line::from(spans)))
            })
            .collect();

        // Append loading indicator as last item
        if matches!(self.state.fetch_status, FetchStatus::Loading) {
            items.push(ListItem::new(Line::from(Span::styled(
                "  Fetching models...",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ))));
        }

        let list = List::new(items);
        frame.render_stateful_widget(list, list_area, &mut self.state.list_state);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pinned() -> Vec<ModelEntry> {
        vec![
            ModelEntry {
                name: "anthropic/claude-sonnet-4".to_string(),
                provider: "openrouter".to_string(),
                description: Some("Fast reasoning".to_string()),
            },
            ModelEntry {
                name: "qwen2.5-coder-32b".to_string(),
                provider: "lmstudio".to_string(),
                description: None,
            },
        ]
    }

    fn sample_fetched() -> Vec<ModelEntry> {
        vec![
            ModelEntry {
                name: "google/gemini-2.5-flash".to_string(),
                provider: "openrouter".to_string(),
                description: Some("Gemini Flash".to_string()),
            },
            ModelEntry {
                name: "meta-llama/llama-3.1-70b".to_string(),
                provider: "openrouter".to_string(),
                description: Some("Llama 3.1 70B".to_string()),
            },
        ]
    }

    #[test]
    fn test_new_picker_shows_pinned() {
        let picker = ModelPickerState::new(sample_pinned());
        assert_eq!(picker.filtered_indices.len(), 2);
        assert_eq!(picker.selected, 0);
        assert!(matches!(picker.fetch_status, FetchStatus::Loading));
    }

    #[test]
    fn test_set_fetched_models_deduplicates() {
        let mut picker = ModelPickerState::new(sample_pinned());
        let mut fetched = sample_fetched();
        // Add a duplicate of pinned model
        fetched.push(ModelEntry {
            name: "anthropic/claude-sonnet-4".to_string(),
            provider: "openrouter".to_string(),
            description: Some("Fast reasoning".to_string()),
        });

        picker.set_fetched_models(fetched);

        // Should have 2 pinned + 2 fetched (duplicate excluded)
        assert_eq!(picker.total_models(), 4);
        assert_eq!(picker.filtered_indices.len(), 4);
        assert!(matches!(picker.fetch_status, FetchStatus::Loaded));
    }

    #[test]
    fn test_search_filters_by_name() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        picker.query = "gemini".to_string();
        picker.rebuild_filter();

        assert_eq!(picker.filtered_indices.len(), 1);
        let model = picker.get_model(picker.filtered_indices[0]).unwrap();
        assert_eq!(model.name, "google/gemini-2.5-flash");
    }

    #[test]
    fn test_search_filters_by_provider() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        picker.query = "lmstudio".to_string();
        picker.rebuild_filter();

        assert_eq!(picker.filtered_indices.len(), 1);
        let model = picker.get_model(picker.filtered_indices[0]).unwrap();
        assert_eq!(model.name, "qwen2.5-coder-32b");
    }

    #[test]
    fn test_search_filters_by_description() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        picker.query = "llama".to_string();
        picker.rebuild_filter();

        assert_eq!(picker.filtered_indices.len(), 1);
        let model = picker.get_model(picker.filtered_indices[0]).unwrap();
        assert!(model.name.contains("llama"));
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        picker.query = "GEMINI".to_string();
        picker.rebuild_filter();

        assert_eq!(picker.filtered_indices.len(), 1);
    }

    #[test]
    fn test_empty_search_shows_all() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        picker.query = String::new();
        picker.rebuild_filter();

        assert_eq!(picker.filtered_indices.len(), 4);
    }

    #[test]
    fn test_escape_clears_search_first() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.query = "test".to_string();

        let event = picker.handle_event(&TuiEvent::Escape);
        assert!(event.is_none()); // First Escape clears search
        assert!(picker.query.is_empty());
    }

    #[test]
    fn test_escape_dismisses_when_search_empty() {
        let mut picker = ModelPickerState::new(sample_pinned());

        let event = picker.handle_event(&TuiEvent::Escape);
        assert!(matches!(event, Some(ModelPickerEvent::Dismiss)));
    }

    #[test]
    fn test_typing_updates_search() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        picker.handle_event(&TuiEvent::InputChar('g'));
        picker.handle_event(&TuiEvent::InputChar('e'));
        picker.handle_event(&TuiEvent::InputChar('m'));

        assert_eq!(picker.query, "gem");
        assert_eq!(picker.filtered_indices.len(), 1);
    }

    #[test]
    fn test_backspace_removes_char() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        picker.handle_event(&TuiEvent::InputChar('x'));
        assert_eq!(picker.filtered_indices.len(), 0);

        picker.handle_event(&TuiEvent::Backspace);
        assert_eq!(picker.query, "");
        assert_eq!(picker.filtered_indices.len(), 4);
    }

    #[test]
    fn test_cursor_navigation() {
        let mut picker = ModelPickerState::new(sample_pinned());
        assert_eq!(picker.selected, 0);

        picker.handle_event(&TuiEvent::CursorDown);
        assert_eq!(picker.selected, 1);

        picker.handle_event(&TuiEvent::CursorDown);
        // Can't go past last item
        assert_eq!(picker.selected, 1);

        picker.handle_event(&TuiEvent::CursorUp);
        assert_eq!(picker.selected, 0);

        picker.handle_event(&TuiEvent::CursorUp);
        // Can't go below 0
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn test_submit_selects_model() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.handle_event(&TuiEvent::CursorDown);

        let event = picker.handle_event(&TuiEvent::Submit);
        match event {
            Some(ModelPickerEvent::Select(model)) => {
                assert_eq!(model.name, "qwen2.5-coder-32b");
            }
            _ => panic!("Expected Select event"),
        }
    }

    #[test]
    fn test_empty_picker() {
        let picker = ModelPickerState::new(Vec::new());
        assert_eq!(picker.filtered_indices.len(), 0);
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn test_get_model_pinned_and_fetched() {
        let mut picker = ModelPickerState::new(sample_pinned());
        picker.set_fetched_models(sample_fetched());

        // Index 0 = first pinned
        assert_eq!(
            picker.get_model(0).unwrap().name,
            "anthropic/claude-sonnet-4"
        );
        // Index 1 = second pinned
        assert_eq!(picker.get_model(1).unwrap().name, "qwen2.5-coder-32b");
        // Index 2 = first fetched
        assert_eq!(picker.get_model(2).unwrap().name, "google/gemini-2.5-flash");
        // Index 3 = second fetched
        assert_eq!(
            picker.get_model(3).unwrap().name,
            "meta-llama/llama-3.1-70b"
        );
        // Out of bounds
        assert!(picker.get_model(4).is_none());
    }
}
