use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};
use ratatui::Frame;

use crate::inference::{ContextSegment, Source};
use crate::tui::component::Component;

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
    /// Current pulse intensity (0.0 to 1.0) for active generation animation
    pub pulse_intensity: f32,
}

impl<'a> Message<'a> {
    /// Creates a new Message component for rendering.
    ///
    /// This is typically called within `MessageList::render()` for each visible segment.
    pub fn new(segment: &'a ContextSegment, is_hovered: bool, pulse_intensity: f32) -> Self {
        Self {
            segment,
            is_hovered,
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
        let content_width = width.saturating_sub(2); // Subtract border padding
        if content_width == 0 {
            return 1;
        }

        let content = segment.content.trim();
        if content.is_empty() { return 2; }

        let options = textwrap::Options::new(content_width as usize)
            .break_words(true) // Ratatui breaks words by default in Wrap { trim: true }
            .word_separator(textwrap::WordSeparator::AsciiSpace); // Matches standard behavior

        let lines = textwrap::wrap(content, options);
        (lines.len() as u16).max(1) + 2 // Content lines + 2 for borders
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
        };

        let style = match self.segment.source {
            Source::Directive => Style::default().fg(Color::Yellow),
            Source::User => Style::default().fg(Color::Green),
            Source::Model => Style::default().fg(Color::Blue),
            Source::Thinking => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        };

        // Hover effect overlays background
        let (style, mut border_style) = if self.is_hovered {
            (style.bg(Color::DarkGray), style)
        } else {
            (style, style.add_modifier(Modifier::DIM))
        };

        // Pulse animation if generating
        if self.pulse_intensity > 0.0 {
             // 3-stage breathing effect
             if self.pulse_intensity > 0.6 {
                 border_style = border_style.add_modifier(Modifier::BOLD).fg(Color::White);
             } else if self.pulse_intensity > 0.2 {
                 border_style = border_style.fg(Color::Gray);
             }
        }

        let content = self.segment.content.trim();
        let paragraph = Paragraph::new(content)
            .block(Block::bordered()
                .title(role)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(border_style)
                .title_style(border_style))
            .style(style)
            .wrap(Wrap { trim: true });
        
        paragraph.render(area, buf);
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
        // Empty content should still have 2 lines for top/bottom borders
        assert_eq!(Message::calculate_height(&segment, 80), 2);
    }

    #[test]
    fn calculate_height_whitespace_only_treated_as_empty() {
        let segment = make_segment(Source::User, "   \n\t  ");
        // Whitespace-only content is trimmed to empty
        assert_eq!(Message::calculate_height(&segment, 80), 2);
    }

    #[test]
    fn calculate_height_zero_width_returns_minimum() {
        let segment = make_segment(Source::User, "Hello world");
        // Width 0 means no room for content
        assert_eq!(Message::calculate_height(&segment, 0), 1);
    }

    #[test]
    fn calculate_height_width_equals_border_returns_minimum() {
        let segment = make_segment(Source::User, "Hello world");
        // Width 2 exactly covers borders, leaving content_width = 0
        assert_eq!(Message::calculate_height(&segment, 2), 1);
    }

    #[test]
    fn calculate_height_single_line_fits() {
        let segment = make_segment(Source::User, "Hello");
        // 5 chars + 2 border = need width 7+ for single line
        // With width 80, content_width = 78, easily fits
        assert_eq!(Message::calculate_height(&segment, 80), 3); // 1 line + 2 borders
    }

    #[test]
    fn calculate_height_wraps_at_width_boundary() {
        let segment = make_segment(Source::User, "Hello world");
        // "Hello world" = 11 chars
        // Width 7 means content_width = 5
        // "Hello" (5) on line 1, "world" (5) on line 2
        assert_eq!(Message::calculate_height(&segment, 7), 4); // 2 lines + 2 borders
    }

    #[test]
    fn calculate_height_breaks_long_words() {
        let segment = make_segment(Source::User, "abcdefghij");
        // "abcdefghij" = 10 chars, no spaces
        // Width 6 means content_width = 4
        // With break_words(true): "abcd" | "efgh" | "ij" = 3 lines
        assert_eq!(Message::calculate_height(&segment, 6), 5); // 3 lines + 2 borders
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
            Source::Thinking => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        }
    }
}
