//! # ToolMessage Component
//!
//! Renders tool calls and tool results as bordered boxes with distinct styling.
//! Structurally different from `Message` — shows name + arguments/output
//! rather than role + content.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Padding, Paragraph, Widget, Wrap};

use crate::inference::{ToolCall, ToolResult};

/// Horizontal padding (per side) between the border and text content.
const CONTENT_PAD_H: u16 = 1;
/// Total horizontal space consumed by borders (1 left + 1 right) and padding.
const HORIZONTAL_OVERHEAD: u16 = 2 + CONTENT_PAD_H * 2;
/// Total vertical space consumed by borders (1 top + 1 bottom).
const VERTICAL_OVERHEAD: u16 = 2;

/// What kind of tool item to render.
pub enum ToolMessageKind<'a> {
    Call(&'a ToolCall),
    Result(&'a ToolResult, &'a str), // result + tool name for display
}

/// Renders a single tool call or tool result.
pub struct ToolMessage<'a> {
    pub kind: ToolMessageKind<'a>,
}

impl<'a> ToolMessage<'a> {
    pub fn new(kind: ToolMessageKind<'a>) -> Self {
        Self { kind }
    }

    /// Calculate the height required for this tool message given a width.
    pub fn calculate_height(&self, width: u16) -> u16 {
        let content_width = width.saturating_sub(HORIZONTAL_OVERHEAD);
        if content_width == 0 {
            return 1;
        }

        let content = self.content_text();
        let content = content.trim();
        if content.is_empty() {
            return VERTICAL_OVERHEAD;
        }

        let options = textwrap::Options::new(content_width as usize)
            .break_words(true)
            .word_separator(textwrap::WordSeparator::AsciiSpace);

        let lines = textwrap::wrap(content, options);
        (lines.len() as u16).max(1) + VERTICAL_OVERHEAD
    }

    fn content_text(&self) -> String {
        match &self.kind {
            ToolMessageKind::Call(tc) => tc.arguments.clone(),
            ToolMessageKind::Result(tr, _) => tr.output.clone(),
        }
    }

    fn title(&self) -> String {
        match &self.kind {
            ToolMessageKind::Call(tc) => format!("tool: {}", tc.name),
            ToolMessageKind::Result(_, name) => format!("result: {name}"),
        }
    }

    fn style(&self) -> Style {
        match &self.kind {
            ToolMessageKind::Call(_) => Style::default().fg(Color::Magenta),
            ToolMessageKind::Result(_, _) => Style::default().fg(Color::Cyan),
        }
    }
}

impl<'a> Widget for ToolMessage<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let style = self.style();
        let border_style = style.add_modifier(Modifier::DIM);
        let title = self.title();
        let content = self.content_text();
        let content = content.trim();

        let block = Block::bordered()
            .title(title)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style)
            .title_style(border_style)
            .padding(Padding::horizontal(CONTENT_PAD_H));

        let inner_area = block.inner(area);
        block.render(area, buf);

        let paragraph = Paragraph::new(content)
            .style(style)
            .wrap(Wrap { trim: true });

        paragraph.render(inner_area, buf);
    }
}
