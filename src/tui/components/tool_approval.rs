//! # Tool Approval Component
//!
//! Modal overlay for approving or denying tool calls that require user permission.
//! Shows the tool name, pretty-printed arguments, and [Y]/[N] controls.
//!
//! Follows the same persistent state + transient wrapper pattern as SessionManager.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};

use crate::inference::ToolCall;
use crate::tui::event::TuiEvent;

/// Persistent state for the tool approval modal.
pub struct ToolApprovalState {
    /// The tool call currently being shown for approval.
    pub current: ToolCall,
}

impl ToolApprovalState {
    pub fn new(tool_call: ToolCall) -> Self {
        Self { current: tool_call }
    }

    pub fn handle_event(&mut self, event: &TuiEvent) -> Option<ToolApprovalEvent> {
        match event {
            TuiEvent::InputChar('y') | TuiEvent::InputChar('Y') => {
                Some(ToolApprovalEvent::Approved(self.current.call_id.clone()))
            }
            TuiEvent::InputChar('n') | TuiEvent::InputChar('N') | TuiEvent::Escape => {
                Some(ToolApprovalEvent::Denied(self.current.call_id.clone()))
            }
            _ => None,
        }
    }
}

/// Events emitted by the tool approval modal.
pub enum ToolApprovalEvent {
    Approved(String),
    Denied(String),
}

/// Transient render wrapper for the tool approval overlay.
pub struct ToolApproval<'a> {
    state: &'a ToolApprovalState,
}

impl<'a> ToolApproval<'a> {
    pub fn new(state: &'a ToolApprovalState) -> Self {
        Self { state }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let overlay = centered_rect(60, 40, area);
        frame.render_widget(Clear, overlay);

        let tc = &self.state.current;

        // Pretty-print the JSON arguments
        let pretty_args = serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| tc.arguments.clone());

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Tool Approval ")
            .title_alignment(Alignment::Left)
            .title_bottom(Line::from(" y Approve  n Deny ").centered())
            .padding(Padding::new(2, 2, 1, 1));

        let inner = block.inner(overlay);

        // Build content lines
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Tool: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &tc.name,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Arguments:",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        for line in pretty_args.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::White),
            )));
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });

        frame.render_widget(block, overlay);
        frame.render_widget(paragraph, inner);
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
