use crate::api::Source;
use crate::core::state::App;

use ratatui::{Frame};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph};

pub fn draw_ui(frame: &mut Frame, app: &App) {
    use Constraint::{Length, Min};
    let layout = Layout::vertical([Length(1), Min(0), Length(3)]);
    let [title_area, main_area, input_area] = layout.areas(frame.area());

    // Title bar - always rendered
    let title_text = if app.status_message.is_empty() {
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

fn draw_context_area(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::Wrap;
    let mut y_offset: u16 = 0;

    for segment in &app.context.items {
        let role_string = format_role(&segment.source);
        let paragraph = Paragraph::new(segment.content.as_str())
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(role_string));

        let inner_width = area.width.saturating_sub(2); // Account for borders
        let height = (paragraph.line_count(inner_width)) as u16; // +2 for top and bottom borders

        if y_offset + height > area.height {
            break; // No more space to render additional segments
        }

        let segment_area = Rect {
            x: area.x,
            y: area.y + y_offset,
            width: area.width,
            height,
        };

        frame.render_widget(paragraph, segment_area);
        y_offset += height;
    }

    let _ = (frame, area, app); // Remove this line when implementing
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
    fn test_draw_ui() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new("test-model".to_string());
        terminal
            .draw(|f| {
                draw_ui(f, &app);
            })
            .unwrap();
        // If no panic occurs, we assume the drawing was successful.
    }

    #[test]
    fn test_draw_context_area() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new("test-model".to_string());
        terminal
            .draw(|f| {
                let area = Rect {
                    x: 0,
                    y: 1,
                    width: 80,
                    height: 20,
                };
                draw_context_area(f, area, &app);
            })
            .unwrap();
        // If no panic occurs, we assume the drawing was successful.
    }
}