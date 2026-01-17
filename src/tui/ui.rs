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
}

/// Calculate segment height without building full Paragraph (for non-visible segments)
fn calculate_segment_height(segment: &ModelSegment, content_width: u16) -> u16 {
    let content = segment.content.trim();
    let paragraph = Paragraph::new(content)
        .block(Block::bordered())
        .wrap(Wrap { trim: true });
    let inner_width = content_width.saturating_sub(2);
    paragraph.line_count(inner_width) as u16
}

impl<'a> RenderedSegment<'a> {
    /// Create a rendered segment (paragraph only, height comes from cache)
    fn new(segment: &'a ModelSegment, is_hovered: bool) -> Self {
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

        RenderedSegment { paragraph }
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

    // Phase 1: Update heights for ALL segments (needed for total_height and visible range)
    let reusable = tui.layout.reusable_count(num_items, content_width, app.is_loading);

    // Ensure heights vec has correct size, calculating only what's needed
    tui.layout.heights.truncate(reusable.min(tui.layout.heights.len()));
    for (index, seg) in app.context.items.iter().enumerate().skip(tui.layout.heights.len()) {
        let height = if index < reusable && index < tui.layout.heights.len() {
            tui.layout.heights[index] // Use cached (shouldn't happen after truncate, but defensive)
        } else {
            calculate_segment_height(seg, content_width)
        };
        tui.layout.heights.push(height);
    }
    tui.layout.rebuild_prefix_heights();
    tui.layout.update_metadata(num_items, content_width);

    let total_height: u16 = tui.layout.heights.iter().sum();

    // Phase 2: Calculate visible range using prefix heights
    let scroll_offset = tui.scroll_state.offset().y;
    let visible_range = tui.layout.visible_range(scroll_offset, area.height);

    // Phase 3: Build RenderedSegments ONLY for visible segments
    let visible_segments: Vec<RenderedSegment> = visible_range.clone()
        .map(|index| {
            let seg = &app.context.items[index];
            let is_last = index == num_items.saturating_sub(1);
            let is_hovered = tui.hovered_index == Some(index) && !(is_last && app.is_loading);
            RenderedSegment::new(seg, is_hovered)
        })
        .collect();

    // Phase 4: Render only visible segments at their correct positions
    let mut scroll_view = ScrollView::new(Size::new(content_width, total_height))
        .vertical_scrollbar_visibility(ScrollbarVisibility::Always)
        .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

    // Calculate starting y_offset for first visible segment
    let mut y_offset: u16 = if visible_range.start > 0 {
        tui.layout.prefix_heights[visible_range.start - 1]
    } else {
        0
    };

    for (i, segment) in visible_segments.iter().enumerate() {
        let index = visible_range.start + i;
        let height = tui.layout.heights[index];
        let segment_rect = Rect::new(0, y_offset, content_width, height);
        scroll_view.render_widget(segment.paragraph.clone(), segment_rect);
        y_offset += height;
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

/// Hit test: given a screen Y coordinate, find which message index (if any) is at that position.
/// Uses binary search on prefix_heights for O(log n) performance.
pub fn hit_test_message(
    screen_y: u16,
    frame_area: Rect,
    scroll_offset_y: u16,
    prefix_heights: &[u16],
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

    // Binary search: find first segment whose end position is > content_y
    // partition_point returns the first index where predicate is false
    let idx = prefix_heights.partition_point(|&end_y| end_y <= content_y);

    if idx < prefix_heights.len() {
        Some(idx)
    } else {
        None // Below all content
    }
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
    fn test_segment_height_includes_borders() {
        let segment = ModelSegment {
            source: Source::User,
            content: "Single line".to_string(),
        };

        let height = calculate_segment_height(&segment, 80);

        // 1 line of content + 2 for borders = 3
        assert_eq!(height, 3);
    }

    #[test]
    fn test_segment_height_trims_content() {
        let segment = ModelSegment {
            source: Source::Model,
            content: "\n\n   Trim me   \n\n".to_string(),
        };

        let height = calculate_segment_height(&segment, 80);

        // "Trim me" is 1 line. + 2 for borders = 3.
        assert_eq!(height, 3);
    }
}