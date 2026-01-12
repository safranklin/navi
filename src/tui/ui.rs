use crate::api::{Source, ModelSegment};
use crate::core::state::App;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect, Size};
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph, Wrap};
use tui_scrollview::{ScrollView, ScrollbarVisibility};

struct RenderedSegment<'a> {
    // 'a to tie this structs lifetime to the ModelSegment reference's lifetime to be explicit about lifetimes
    #[allow(dead_code)]
    segment: &'a ModelSegment,
    paragraph: Paragraph<'a>,
    height: u16,
}

// Implement a constructor for RenderedSegment to encapsulate the creation logic (and height calculations)
impl<'a> RenderedSegment<'a> {
    fn new(segment: &'a ModelSegment, window_area: Rect) -> Self {
        let role = format_role(&segment.source);
        let paragraph = Paragraph::new(segment.content.as_str())
            .block(Block::bordered().title(role))
            .wrap(Wrap { trim: true });

        let inner_width = window_area.width.saturating_sub(2); // Account for borders
        let height = (paragraph.line_count(inner_width)) as u16; // Calculate height based on content and width of viewport

        RenderedSegment {
            segment,
            paragraph,
            height,
        }
    }
}

pub fn draw_ui(frame: &mut Frame, app: &mut App) {
    use Constraint::{Length, Min};
    let layout = Layout::vertical([Length(1), Min(0), Length(3)]);
    let [title_area, main_area, input_area] = layout.areas(frame.area());

    // Main area - show error OR chat
    // Render this FIRST so we can update app.has_unseen_content based on scroll position
    if let Some(error_msg) = &app.error {
        draw_error_view(frame, main_area, error_msg);
    } else {
        draw_context_area(frame, main_area, app);
    }

    // Title bar - show "↓ New" indicator if there's unseen content
    let title_text = if app.has_unseen_content {
        format!("Navi Interface (model: {}) | {} | ↓ New", app.model_name, app.status_message)
    } else if app.status_message.is_empty() {
        format!("Navi Interface (model: {})", app.model_name)
    } else {
        format!("Navi Interface (model: {}) | {}", app.model_name, app.status_message)
    };
    frame.render_widget(Span::raw(title_text), title_area);

    // Input area - always rendered
    let input = Paragraph::new(app.input_buffer.as_str()).block(Block::bordered().title("Input"));
    frame.render_widget(input, input_area);
}

fn draw_error_view(frame: &mut Frame, area: Rect, error_msg: &str) {
    use ratatui::layout::Alignment;

    let error_paragraph = Paragraph::new(error_msg)
        .block(Block::bordered().title("ERROR"))
        .alignment(Alignment::Center);

    frame.render_widget(error_paragraph, area);
}

fn draw_context_area(frame: &mut Frame, area: Rect, app: &mut App) {
    // Leave 1 column for vertical scrollbar
    let content_width = area.width.saturating_sub(1);

    // Calculate total content height by summing all segment heights
    let total_height: u16 = app.context.items
        .iter()
        .map(|seg| RenderedSegment::new(seg, area).height)
        .sum();

    // Create a ScrollView with the total content size
    // - Vertical scrollbar always visible (for chat history)
    // - Horizontal scrollbar hidden (content wraps)
    let mut scroll_view = ScrollView::new(Size::new(content_width, total_height))
        .vertical_scrollbar_visibility(ScrollbarVisibility::Always)
        .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

    // Render ALL segments into the ScrollView's internal buffer
    let mut y_offset: u16 = 0;
    for segment in &app.context.items {
        let rendered = RenderedSegment::new(segment, area);
        let segment_rect = Rect::new(0, y_offset, content_width, rendered.height);
        scroll_view.render_widget(rendered.paragraph, segment_rect);
        y_offset += rendered.height;
    }

    // Let ScrollView render the visible portion using scroll_state
    frame.render_stateful_widget(scroll_view, area, &mut app.scroll_state);

    // Update "unseen content" indicator
    // Check if we are at the bottom of the content
    let current_offset = app.scroll_state.offset().y;
    let visible_height = area.height;
    
    if total_height <= visible_height {
        app.has_unseen_content = false;
    } else {
        let max_scroll = total_height.saturating_sub(visible_height);
        // If we are at or past the max scroll, we are at the bottom
        app.has_unseen_content = current_offset < max_scroll;
    }
}

/// Maps a Source to its display string
fn format_role(source: &Source) -> &'static str {
    match source {
        Source::User => "user",
        Source::Model => "navi",
        Source::Directive => "system",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    
    #[test]
    fn test_format_role() {
        assert_eq!(format_role(&Source::User), "user");
        assert_eq!(format_role(&Source::Model), "navi");
        assert_eq!(format_role(&Source::Directive), "system");
    }

    #[test]
    fn test_draw_ui() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("test-model".to_string());
        terminal
            .draw(|f| {
                draw_ui(f, &mut app);
            })
            .unwrap();
        // If no panic occurs, we assume the drawing was successful.
    }

    #[test]
    fn test_draw_context_area() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("test-model".to_string());
        terminal
            .draw(|f| {
                let area = Rect {
                    x: 0,
                    y: 1,
                    width: 80,
                    height: 20,
                };
                draw_context_area(f, area, &mut app);
            })
            .unwrap();
        // If no panic occurs, we assume the drawing was successful.
    }

    #[test]
    fn test_rendered_segment_height_includes_borders() {
        let segment = ModelSegment {
            source: Source::User,
            content: "Single line".to_string(),
        };
        let area = Rect { x: 0, y: 0, width: 80, height: 100 };
        
        let rendered = RenderedSegment::new(&segment, area);
        
        // 1 line of content + 2 for borders = 3
        assert_eq!(rendered.height, 3);
    }
}