use crate::api::{Source, ModelSegment};
use crate::core::state::App;
use crate::tui::TuiState;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect, Size};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph, Wrap};
use tui_scrollview::{ScrollView, ScrollbarVisibility};

struct RenderedSegment<'a> {
    paragraph: Paragraph<'a>,
    height: u16,
}

impl<'a> RenderedSegment<'a> {
    fn new(segment: &'a ModelSegment, window_area: Rect, is_hovered: bool) -> Self {
        let role = format_role(&segment.source);
        let base_style = get_role_style(&segment.source);

        // Apply hover styling: background highlight + non-dim border
        let (style, border_style) = if is_hovered {
            let hover_style = base_style.bg(Color::DarkGray);
            let hover_border = base_style; // Non-dim border when hovered
            (hover_style, hover_border)
        } else {
            let normal_border = base_style.add_modifier(Modifier::DIM);
            (base_style, normal_border)
        };

        let content = segment.content.trim();
        let paragraph = Paragraph::new(content)
            .block(Block::bordered()
                .title(role)
                .border_style(border_style)
                .title_style(border_style))
            .style(style)
            .wrap(Wrap { trim: true });

        let inner_width = window_area.width.saturating_sub(2);
        let height = paragraph.line_count(inner_width) as u16;

        RenderedSegment { paragraph, height }
    }
}

pub fn draw_ui(frame: &mut Frame, app: &App, tui: &mut TuiState) {
    use Constraint::{Length, Min};
    let layout = Layout::vertical([Length(1), Min(0), Length(3)]);
    let [title_area, main_area, input_area] = layout.areas(frame.area());

    // Main area - show error OR chat
    if let Some(error_msg) = &app.error {
        draw_error_view(frame, main_area, error_msg);
    } else {
        draw_context_area(frame, main_area, app, tui);
    }

    // Title bar
    let title_text = if tui.has_unseen_content {
        format!("Navi Interface (model: {}) | {} | ↓ New", app.model_name, app.status_message)
    } else if app.status_message.is_empty() {
        format!("Navi Interface (model: {})", app.model_name)
    } else {
        format!("Navi Interface (model: {}) | {}", app.model_name, app.status_message)
    };
    frame.render_widget(Span::raw(title_text), title_area);

    // Input area
    let input = Paragraph::new(tui.input_buffer.as_str())
        .block(Block::bordered().title("Input"));
    frame.render_widget(input, input_area);
}

fn draw_error_view(frame: &mut Frame, area: Rect, error_msg: &str) {
    use ratatui::layout::Alignment;

    let error_paragraph = Paragraph::new(error_msg)
        .block(Block::bordered().title("ERROR"))
        .alignment(Alignment::Center);

    frame.render_widget(error_paragraph, area);
}

fn draw_context_area(frame: &mut Frame, area: Rect, app: &App, tui: &mut TuiState) {
    let content_width = area.width.saturating_sub(1);
    let num_items = app.context.items.len();

    // Build segments and cache heights
    let segments: Vec<RenderedSegment> = app.context.items
        .iter()
        .enumerate()
        .map(|(index, seg)| {
            // Hover is active if: this is the hovered index AND NOT (last message while loading)
            let is_last = index == num_items.saturating_sub(1);
            let is_hovered = tui.hovered_index == Some(index) && !(is_last && app.is_loading);
            RenderedSegment::new(seg, area, is_hovered)
        })
        .collect();

    // Cache heights for hit testing
    tui.segment_heights = segments.iter().map(|s| s.height).collect();

    let total_height: u16 = tui.segment_heights.iter().sum();

    let mut scroll_view = ScrollView::new(Size::new(content_width, total_height))
        .vertical_scrollbar_visibility(ScrollbarVisibility::Always)
        .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

    // Render all segments
    let mut y_offset: u16 = 0;
    for segment in &segments {
        let segment_rect = Rect::new(0, y_offset, content_width, segment.height);
        scroll_view.render_widget(segment.paragraph.clone(), segment_rect);
        y_offset += segment.height;
    }

    frame.render_stateful_widget(scroll_view, area, &mut tui.scroll_state);

    // Update unseen content indicator
    let current_offset = tui.scroll_state.offset().y;
    let visible_height = area.height;

    if total_height <= visible_height {
        tui.has_unseen_content = false;
    } else {
        let max_scroll = total_height.saturating_sub(visible_height);
        tui.has_unseen_content = current_offset < max_scroll;
    }
}

/// Hit test: given a screen Y coordinate, find which message index (if any) is at that position
pub fn hit_test_message(
    screen_y: u16,
    frame_area: Rect,
    scroll_offset_y: u16,
    segment_heights: &[u16],
) -> Option<usize> {
    use Constraint::{Length, Min};

    // Calculate layout to find main_area
    let layout = Layout::vertical([Length(1), Min(0), Length(3)]);
    let [_title_area, main_area, _input_area] = layout.areas(frame_area);

    // Check if mouse is within the main content area
    if screen_y < main_area.y || screen_y >= main_area.y + main_area.height {
        return None;
    }

    // Convert screen Y to content Y (accounting for scroll)
    let content_y = (screen_y - main_area.y) + scroll_offset_y;

    // Walk through cached heights to find which segment contains content_y
    let mut accumulated_height: u16 = 0;
    for (index, &height) in segment_heights.iter().enumerate() {
        accumulated_height += height;
        if content_y < accumulated_height {
            return Some(index);
        }
    }

    None // Below all content
}

fn format_role(source: &Source) -> &'static str {
    match source {
        Source::User => "user",
        Source::Model => "navi",
        Source::Directive => "system",
        Source::Thinking => "thought",
    }
}

fn get_role_style(source: &Source) -> Style {
    match source {
        Source::Directive => Style::default().fg(Color::Yellow),
        Source::User => Style::default().fg(Color::Cyan),
        Source::Model => Style::default().fg(Color::Green),
        Source::Thinking => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
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
        assert_eq!(format_role(&Source::Thinking), "thought");
    }

    #[test]
    fn test_draw_ui() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new("test-model".to_string());
        let mut tui = TuiState::new();
        terminal
            .draw(|f| {
                draw_ui(f, &app, &mut tui);
            })
            .unwrap();
    }

    #[test]
    fn test_rendered_segment_height_includes_borders() {
        let segment = ModelSegment {
            source: Source::User,
            content: "Single line".to_string(),
        };
        let area = Rect { x: 0, y: 0, width: 80, height: 100 };

        let rendered = RenderedSegment::new(&segment, area, false);

        // 1 line of content + 2 for borders = 3
        assert_eq!(rendered.height, 3);
    }

    #[test]
    fn test_rendered_segment_trims_content() {
        let segment = ModelSegment {
            source: Source::Model,
            content: "\n\n   Trim me   \n\n".to_string(),
        };
        let area = Rect { x: 0, y: 0, width: 80, height: 100 };

        let rendered = RenderedSegment::new(&segment, area, false);

        // "Trim me" is 1 line. + 2 for borders = 3.
        assert_eq!(rendered.height, 3);
    }
}