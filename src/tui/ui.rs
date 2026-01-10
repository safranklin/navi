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

    fn render_coords(&self, y_offset: u16, area: &Rect) -> Rect {
        Rect {
            x: area.x,
            y: area.y + y_offset,
            width: area.width,
            height: self.height,
        }
    }
}

pub fn draw_ui(frame: &mut Frame, app: &mut App) {
    use Constraint::{Length, Min};
    let layout = Layout::vertical([Length(1), Min(0), Length(3)]);
    let [title_area, main_area, input_area] = layout.areas(frame.area());

    // Title bar - show "↓ New" indicator if there's unseen content
    let title_text = if app.has_unseen_content {
        format!("Navi Interface (model: {}) | {} | ↓ New", app.model_name, app.status_message)
    } else if app.status_message.is_empty() {
        format!("Navi Interface (model: {})", app.model_name)
    } else {
        format!("Navi Interface (model: {}) | {}", app.model_name, app.status_message)
    };
    frame.render_widget(Span::raw(title_text), title_area);

    // Main area - show error OR chat
    if let Some(error_msg) = &app.error {
        draw_error_view(frame, main_area, error_msg);
    } else {
        draw_context_area(frame, main_area, app);
    }

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
}

fn collect_visible_segments(
    segments: &[ModelSegment],
    area: Rect,
) -> Vec<RenderedSegment<'_>> {
    let mut visible = Vec::new();
    let mut accumulated_height: u16 = 0;

    for segment in segments.iter().rev() {
        let rendered_segment = RenderedSegment::new(segment, area);
        let segment_height = rendered_segment.height;

        if accumulated_height + segment_height > area.height {
            break; // No more space to add this segment
        }

        visible.push(rendered_segment);
        accumulated_height += segment_height;
    }

    visible.reverse();
    visible
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
    fn test_collect_visible_segments() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 10,
        };

        let segments = vec![
            ModelSegment {
                source: Source::User,
                content: "Short message".to_string(),
            },
            ModelSegment {
                source: Source::Model,
                content: "This is a longer message that should take up more space in the UI.".to_string(),
            },
            ModelSegment {
                source: Source::Directive,
                content: "System directive message".to_string(),
            },
        ];

        let visible_segments = collect_visible_segments(&segments, area);
        assert!(!visible_segments.is_empty());
    }

    #[test]
    fn test_collect_visible_segments_no_fit() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 2,
        };

        let segments = vec![
            ModelSegment {
                source: Source::User,
                content: "This message is way too long to fit".to_string(),
            },
        ];

        let visible_segments = collect_visible_segments(&segments, area);
        assert!(visible_segments.is_empty());
    }

    #[test]
    fn test_collect_visible_segments_exact_fit() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 3 + 3 + 4, // 3 segments with heights 3, 3, and 4 respectively
        };

        let segments = vec![
            ModelSegment {
                source: Source::User,
                content: "Msg1".to_string(), // 1 line (+ borders = 3)
            },
            ModelSegment {
                source: Source::Model,
                content: "Msg2".to_string(), // 1 line (+ borders = 3)
            },
            ModelSegment {
                source: Source::Directive,
                content: "Test Line 1\ntest line 2".to_string(), // 2 lines (+ borders = 4)
            },
        ];

        let visible_segments = collect_visible_segments(&segments, area);
        assert_eq!(visible_segments.len(), 3);
    }

    #[test]
    fn test_collect_visible_segments_empty() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 10,
        };

        let segments: Vec<ModelSegment> = vec![];

        let visible_segments = collect_visible_segments(&segments, area);
        assert!(visible_segments.is_empty());
    }

    #[test]
    fn test_collect_visible_segments_overflow() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 5,
        };

        let segments = vec![
            ModelSegment {
                source: Source::User,
                content: "First message".to_string(),
            },
            ModelSegment {
                source: Source::Model,
                content: "Second message that is a bit longer".to_string(),
            },
            ModelSegment {
                source: Source::Directive,
                content: "Third message\na\nb".to_string(),
            },
        ];

        let visible_segments = collect_visible_segments(&segments, area);
        assert_eq!(1, visible_segments.len());
        // Ensure the newest segment (3rd message) is kept, not older ones
        assert_eq!(visible_segments[0].segment.content, segments[2].content);
    }

    #[test]
    fn test_collect_visible_segments_preserves_order() {
        let area = Rect { x: 0, y: 0, width: 80, height: 6 };
        
        let segments = vec![
            ModelSegment { source: Source::User, content: "OLD".to_string() },
            ModelSegment { source: Source::Model, content: "MID".to_string() },
            ModelSegment { source: Source::Directive, content: "NEW".to_string() },
        ];

        let visible = collect_visible_segments(&segments, area);
        
        assert_eq!(visible.len(), 2);
        // Verify order: older visible first, newest last
        assert_eq!(visible[0].segment.content, "MID");
        assert_eq!(visible[1].segment.content, "NEW");
    }

    #[test]
    fn test_render_coords() {
        let segment = ModelSegment {
            source: Source::User,
            content: "Test".to_string(),
        };
        let area = Rect { x: 10, y: 20, width: 80, height: 100 };
        let rendered = RenderedSegment::new(&segment, area);
        
        let coords = rendered.render_coords(5, &area);
        
        assert_eq!(coords.x, 10);        // Same as area.x
        assert_eq!(coords.y, 25);        // area.y + y_offset
        assert_eq!(coords.width, 80);    // Same as area.width
        assert_eq!(coords.height, rendered.height);
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