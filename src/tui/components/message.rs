use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, Padding, Paragraph, Widget, Wrap};

use crate::inference::{ContextSegment, Source, UsageStats};
use crate::tui::component::Component;

/// Horizontal padding (per side) between the border and text content.
const CONTENT_PAD_H: u16 = 1;
/// Total horizontal space consumed by borders (1 left + 1 right) and padding.
const HORIZONTAL_OVERHEAD: u16 = 2 + CONTENT_PAD_H * 2;
/// Total vertical space consumed by borders (1 top + 1 bottom).
const VERTICAL_OVERHEAD: u16 = 2;

/// Pulse intensity threshold above which the border transitions from normal to BOLD.
const PULSE_BOLD_THRESHOLD: f32 = 0.6;
/// Pulse intensity threshold above which the border transitions from DIM to normal.
const PULSE_NORMAL_THRESHOLD: f32 = 0.2;

/// A stateless component that renders a single chat message with source-based styling.
///
/// # Design
///
/// `Message` is a **transient component**: it's created fresh each frame with the data
/// it needs to render. It holds no mutable state—the `is_selected` flag is passed in
/// from the parent `MessageList` which tracks selection state persistently.
///
/// # Styling
///
/// Each message source gets distinct visual treatment:
/// - **User** (cyan): Messages from the human
/// - **Model** (green): Responses from the AI
/// - **Directive** (yellow): System instructions
/// - **Thinking** (dark gray, italic): Model reasoning traces
///
/// Selected messages are rendered at normal brightness; unselected messages are dimmed.
///
/// # Height Calculation
///
/// The [`calculate_height`](Self::calculate_height) method predicts rendered height
/// using `Paragraph::line_count` on the same styled content used for rendering.
/// This enables the parent `MessageList` to calculate scroll positions without
/// actually rendering each message.
#[derive(Clone, Copy)]
pub struct Message<'a> {
    /// The message content and metadata to render
    pub segment: &'a ContextSegment,
    /// Whether this message is currently selected (hover or keyboard navigation)
    pub is_selected: bool,
    /// Current pulse intensity (0.0 to 1.0) for active generation animation
    pub pulse_intensity: f32,
    /// Optional usage stats to display on the bottom border
    pub stats: Option<&'a UsageStats>,
}

impl<'a> Message<'a> {
    /// Creates a new Message component for rendering.
    ///
    /// This is typically called within `MessageList::render()` for each visible segment.
    pub fn new(
        segment: &'a ContextSegment,
        is_selected: bool,
        pulse_intensity: f32,
        stats: Option<&'a UsageStats>,
    ) -> Self {
        Self {
            segment,
            is_selected,
            pulse_intensity,
            stats,
        }
    }

    /// Calculate the height required for this message given a width.
    ///
    /// Uses `Paragraph::line_count` to predict height from the same styled
    /// content we'd actually render — no separate wrapping library to keep in sync.
    pub fn calculate_height(segment: &ContextSegment, width: u16) -> u16 {
        let content_width = width.saturating_sub(HORIZONTAL_OVERHEAD);
        if content_width == 0 {
            return 1;
        }

        let content = segment.content.trim();
        if content.is_empty() {
            return VERTICAL_OVERHEAD;
        }

        let paragraph = build_paragraph(content, &segment.source);
        let lines = paragraph.line_count(content_width) as u16;
        lines.max(1) + VERTICAL_OVERHEAD
    }
}

/// Build the paragraph for a message — markdown for User/Model, plain for others.
fn build_paragraph<'a>(content: &'a str, source: &Source) -> Paragraph<'a> {
    match source {
        Source::User | Source::Model => {
            let base_fg = match source {
                Source::User => Color::Green,
                Source::Model => Color::Blue,
                _ => unreachable!(),
            };
            let text = crate::tui::markdown::render(content, base_fg);
            // trim: false to preserve indentation in code blocks
            Paragraph::new(text).wrap(Wrap { trim: false })
        }
        _ => {
            let style = source_style(source);
            Paragraph::new(Text::raw(content))
                .style(style)
                .wrap(Wrap { trim: true })
        }
    }
}

/// Get the base style for a message source.
fn source_style(source: &Source) -> Style {
    match source {
        Source::Directive => Style::default().fg(Color::Yellow),
        Source::User => Style::default().fg(Color::Green),
        Source::Model => Style::default().fg(Color::Blue),
        Source::Thinking | Source::Status => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    }
}

// Implement Widget for easy usage in ScrollView
impl<'a> Widget for Message<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let role = match self.segment.source {
            Source::User => "user",
            Source::Model => "navi",
            Source::Directive => "system",
            Source::Thinking => "thought",
            Source::Status => "navi",
        };

        let style = source_style(&self.segment.source);

        // Selected = source color at normal brightness (lightened from default dim)
        let mut border_style = if self.is_selected {
            style
        } else {
            style.add_modifier(Modifier::DIM)
        };

        // Pulse animation if generating
        // Three-phase breathing: DIM → normal → BOLD using the source's own color
        if self.pulse_intensity > PULSE_BOLD_THRESHOLD {
            border_style = border_style
                .remove_modifier(Modifier::DIM)
                .add_modifier(Modifier::BOLD);
        } else if self.pulse_intensity > PULSE_NORMAL_THRESHOLD {
            border_style = border_style.remove_modifier(Modifier::DIM);
        }

        let content = self.segment.content.trim();

        // Render the block into `area`, then the paragraph into the inner rect.
        let mut block = Block::bordered()
            .title(role)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style)
            .title_style(border_style)
            .padding(Padding::horizontal(CONTENT_PAD_H));

        // Add stats on the bottom-right border, matching the border color
        if let Some(stats) = self.stats {
            let summary = stats.display_summary();
            if summary != "Response complete." {
                block = block.title_bottom(
                    ratatui::text::Line::from(format!(" {summary} "))
                        .right_aligned()
                        .style(border_style),
                );
            }
        }

        // Get the inner area of the block where the paragraph will be rendered
        let inner_area = block.inner(area);
        block.render(area, buf);

        let paragraph = build_paragraph(content, &self.segment.source);
        paragraph.render(inner_area, buf);
    }
}

/// Component trait implementation.
///
/// Note: `Message` is stateless, so the `&mut self` required by the trait is a no-op.
/// We implement Component for API consistency with other components, but the actual
/// rendering is delegated to the [`Widget`] implementation.
impl<'a> Component for Message<'a> {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Delegate to Widget implementation
        frame.render_widget(*self, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a segment with given source and content
    fn make_segment(source: Source, content: &str) -> ContextSegment {
        ContextSegment {
            source,
            content: content.to_string(),
        }
    }

    // ==========================================================================
    // calculate_height tests
    // ==========================================================================

    #[test]
    fn calculate_height_empty_content_returns_border_height() {
        let segment = make_segment(Source::User, "");
        assert_eq!(Message::calculate_height(&segment, 80), VERTICAL_OVERHEAD);
    }

    #[test]
    fn calculate_height_whitespace_only_treated_as_empty() {
        let segment = make_segment(Source::User, "   \n\t  ");
        assert_eq!(Message::calculate_height(&segment, 80), VERTICAL_OVERHEAD);
    }

    #[test]
    fn calculate_height_zero_width_returns_minimum() {
        let segment = make_segment(Source::User, "Hello world");
        assert_eq!(Message::calculate_height(&segment, 0), 1);
    }

    #[test]
    fn calculate_height_width_equals_overhead_returns_minimum() {
        let segment = make_segment(Source::User, "Hello world");
        assert_eq!(Message::calculate_height(&segment, HORIZONTAL_OVERHEAD), 1);
    }

    #[test]
    fn calculate_height_single_line_fits() {
        let segment = make_segment(Source::User, "Hello");
        assert_eq!(
            Message::calculate_height(&segment, 80),
            1 + VERTICAL_OVERHEAD
        );
    }

    #[test]
    fn calculate_height_thinking_uses_plain_text() {
        let segment = make_segment(Source::Thinking, "just thinking...");
        // Plain text, no markdown parsing — should be 1 line + overhead
        assert_eq!(
            Message::calculate_height(&segment, 80),
            1 + VERTICAL_OVERHEAD
        );
    }

    #[test]
    fn calculate_height_markdown_heading() {
        let segment = make_segment(Source::Model, "# Big Title\n\nSome body text");
        let height = Message::calculate_height(&segment, 80);
        // Heading + blank line + body = at least 3 content lines + overhead
        assert!(
            height >= 3 + VERTICAL_OVERHEAD,
            "expected >= {}, got {}",
            3 + VERTICAL_OVERHEAD,
            height
        );
    }

    #[test]
    fn calculate_height_code_block_preserves_lines() {
        let segment = make_segment(Source::Model, "```\nline1\nline2\nline3\n```");
        let height = Message::calculate_height(&segment, 80);
        // 3 code lines at minimum + overhead (fences may add more)
        assert!(
            height >= 3 + VERTICAL_OVERHEAD,
            "expected >= {}, got {}",
            3 + VERTICAL_OVERHEAD,
            height
        );
    }

    // ==========================================================================
    // Style tests
    // ==========================================================================

    #[test]
    fn style_user_is_green() {
        assert_eq!(source_style(&Source::User).fg, Some(Color::Green));
    }

    #[test]
    fn style_model_is_blue() {
        assert_eq!(source_style(&Source::Model).fg, Some(Color::Blue));
    }

    #[test]
    fn style_directive_is_yellow() {
        assert_eq!(source_style(&Source::Directive).fg, Some(Color::Yellow));
    }

    #[test]
    fn style_thinking_is_dark_gray_italic() {
        let style = source_style(&Source::Thinking);
        assert_eq!(style.fg, Some(Color::DarkGray));
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }
}
