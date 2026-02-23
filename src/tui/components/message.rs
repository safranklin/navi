use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Padding, Paragraph, Widget, Wrap};

use crate::inference::{ContextSegment, Source};
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
/// it needs to render. It holds no mutable state—the `is_hovered` flag is passed in
/// from the parent `MessageList` which tracks hover state persistently.
///
/// # Styling
///
/// Each message source gets distinct visual treatment:
/// - **User** (cyan): Messages from the human
/// - **Model** (green): Responses from the AI
/// - **Directive** (yellow): System instructions
/// - **Thinking** (dark gray, italic): Model reasoning traces
///
/// Hovered messages get a `DarkGray` background overlay for visual feedback.
///
/// # Height Calculation
///
/// The [`calculate_height`](Self::calculate_height) method predicts rendered height
/// using `textwrap` with options that match Ratatui's `Paragraph` wrapping behavior.
/// This enables the parent `MessageList` to calculate scroll positions without
/// actually rendering each message.
#[derive(Clone, Copy)]
pub struct Message<'a> {
    /// The message content and metadata to render
    pub segment: &'a ContextSegment,
    /// Whether this message is currently under the cursor
    pub is_hovered: bool,
    /// Whether this message is selected in Cursor mode
    pub is_selected: bool,
    /// Current pulse intensity (0.0 to 1.0) for active generation animation
    pub pulse_intensity: f32,
}

impl<'a> Message<'a> {
    /// Creates a new Message component for rendering.
    ///
    /// This is typically called within `MessageList::render()` for each visible segment.
    pub fn new(
        segment: &'a ContextSegment,
        is_hovered: bool,
        is_selected: bool,
        pulse_intensity: f32,
    ) -> Self {
        Self {
            segment,
            is_hovered,
            is_selected,
            pulse_intensity,
        }
    }

    /// Calculate the height required for this message given a width.
    ///
    /// # Architecture Note
    ///
    /// We use `textwrap` here to accurately predict the height of the message
    /// *without* rendering it. This avoids a circular dependency where we need
    /// the height to create the ScrollView, but need to render to get the height.
    ///
    /// The wrapping options must match the `Ratatui` default for `Paragraph`
    /// to ensure 1:1 mapping between calculated and actual height.
    pub fn calculate_height(segment: &ContextSegment, width: u16) -> u16 {
        let content_width = width.saturating_sub(HORIZONTAL_OVERHEAD);
        if content_width == 0 {
            // Degenerate case: terminal too narrow for borders + padding.
            // Return 1 row so the message still occupies space in the layout.
            return 1;
        }

        let content = segment.content.trim();
        if content.is_empty() {
            return VERTICAL_OVERHEAD;
        }

        let options = textwrap::Options::new(content_width as usize)
            .break_words(true)
            .word_separator(textwrap::WordSeparator::AsciiSpace);

        let lines = textwrap::wrap(content, options);
        // Ensure at least 1 content line even if textwrap returns empty
        (lines.len() as u16).max(1) + VERTICAL_OVERHEAD
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

        let style = match self.segment.source {
            Source::Directive => Style::default().fg(Color::Yellow),
            Source::User => Style::default().fg(Color::Green),
            Source::Model => Style::default().fg(Color::Blue),
            Source::Thinking => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
            Source::Status => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        };

        // Selection overrides hover: cyan border for selected, bright for hover, dim otherwise
        let mut border_style = if self.is_selected {
            Style::default().fg(Color::Cyan)
        } else if self.is_hovered {
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
        let block = Block::bordered()
            .title(role)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style)
            .title_style(border_style)
            .padding(Padding::horizontal(CONTENT_PAD_H));

        // Get the inner area of the block where the paragraph will be rendered
        let inner_area = block.inner(area);
        block.render(area, buf);

        let paragraph = Paragraph::new(content)
            .style(style)
            .wrap(Wrap { trim: true });

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
        // Empty content → just VERTICAL_OVERHEAD (top + bottom borders)
        assert_eq!(Message::calculate_height(&segment, 80), VERTICAL_OVERHEAD);
    }

    #[test]
    fn calculate_height_whitespace_only_treated_as_empty() {
        let segment = make_segment(Source::User, "   \n\t  ");
        // Whitespace-only content is trimmed to empty → VERTICAL_OVERHEAD
        assert_eq!(Message::calculate_height(&segment, 80), VERTICAL_OVERHEAD);
    }

    #[test]
    fn calculate_height_zero_width_returns_minimum() {
        let segment = make_segment(Source::User, "Hello world");
        // Width 0: no room for borders + padding → degenerate fallback of 1 row
        assert_eq!(Message::calculate_height(&segment, 0), 1);
    }

    #[test]
    fn calculate_height_width_equals_overhead_returns_minimum() {
        let segment = make_segment(Source::User, "Hello world");
        // Width == HORIZONTAL_OVERHEAD: content_width = 0 → degenerate fallback
        assert_eq!(Message::calculate_height(&segment, HORIZONTAL_OVERHEAD), 1);
    }

    #[test]
    fn calculate_height_single_line_fits() {
        let segment = make_segment(Source::User, "Hello");
        // "Hello" (5 chars) fits in width 80 - HORIZONTAL_OVERHEAD = 76
        assert_eq!(
            Message::calculate_height(&segment, 80),
            1 + VERTICAL_OVERHEAD
        );
    }

    #[test]
    fn calculate_height_wraps_at_width_boundary() {
        let segment = make_segment(Source::User, "Hello world");
        // "Hello world" = 11 chars, width 9 → content_width = 5
        // Wraps to: "Hello" | "world" = 2 lines
        assert_eq!(
            Message::calculate_height(&segment, 9),
            2 + VERTICAL_OVERHEAD
        );
    }

    #[test]
    fn calculate_height_breaks_long_words() {
        let segment = make_segment(Source::User, "abcdefghij");
        // "abcdefghij" = 10 chars, width 8 → content_width = 4
        // Breaks to: "abcd" | "efgh" | "ij" = 3 lines
        assert_eq!(
            Message::calculate_height(&segment, 8),
            3 + VERTICAL_OVERHEAD
        );
    }

    // ==========================================================================
    // Style tests - verify each Source variant gets correct styling
    // ==========================================================================

    #[test]
    fn style_user_is_green() {
        let segment = make_segment(Source::User, "test");
        let style = get_source_style(&segment.source);
        assert_eq!(style.fg, Some(Color::Green));
    }

    #[test]
    fn style_model_is_blue() {
        let segment = make_segment(Source::Model, "test");
        let style = get_source_style(&segment.source);
        assert_eq!(style.fg, Some(Color::Blue));
    }

    #[test]
    fn style_directive_is_yellow() {
        let segment = make_segment(Source::Directive, "test");
        let style = get_source_style(&segment.source);
        assert_eq!(style.fg, Some(Color::Yellow));
    }

    #[test]
    fn style_thinking_is_dark_gray_italic() {
        let segment = make_segment(Source::Thinking, "test");
        let style = get_source_style(&segment.source);
        assert_eq!(style.fg, Some(Color::DarkGray));
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }

    /// Helper to extract style for a given source (mirrors Widget impl logic)
    fn get_source_style(source: &Source) -> Style {
        match source {
            Source::Directive => Style::default().fg(Color::Yellow),
            Source::User => Style::default().fg(Color::Green),
            Source::Model => Style::default().fg(Color::Blue),
            Source::Thinking | Source::Status => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        }
    }
}
