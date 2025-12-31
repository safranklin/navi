use crate::api::{ModelSegment, Source};
use crate::core::state::App;

use ratatui::{Frame};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph};

pub fn draw_ui(frame: &mut Frame, app: &App) {
    use Constraint::{Length, Min};
    let layout = Layout::vertical([Length(1), Min(0), Length(3)]);
    let [title_area, main_area, input_area] = layout.areas(frame.area());

    let input = Paragraph::new(app.input_buffer.as_str()).block(Block::bordered().title("Input"));
    frame.render_widget(Span::raw(format!("Navi Interface (model: {})", app.model_name)), title_area);
    draw_context_area(frame, main_area, app);
    frame.render_widget(input, input_area);
}

fn draw_context_area(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::{List, ListItem};

    let items: Vec<ListItem> = app
        .context
        .items
        .iter()
        .map(|seg| ListItem::new(format_context_item(seg)))
        .collect();

    let chat = List::new(items).block(Block::bordered().title("Chat"));
    frame.render_widget(chat, area);
}

fn format_context_item(segment: &ModelSegment) -> String {
    // Map source to display prefix
    let role_str = match segment.source {
        Source::User => "user",
        Source::Model => "navi",
        Source::Directive => "system",
    };

    let content = &segment.content;
    format!("{}> {}", role_str, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    #[test]
    fn test_format_context_item() {
        let user_segment = ModelSegment {
            source: Source::User,
            content: String::from("Hello, model!"),
        };
        let model_segment = ModelSegment {
            source: Source::Model,
            content: String::from("Hello, user!"),
        };
        let directive_segment = ModelSegment {
            source: Source::Directive,
            content: String::from("You are a helpful assistant."),
        };
        assert_eq!(
            format_context_item(&user_segment),
            "user> Hello, model!"
        );
        assert_eq!(
            format_context_item(&model_segment),
            "navi> Hello, user!"
        );
        assert_eq!(
            format_context_item(&directive_segment),
            "system> You are a helpful assistant."
        );
    }

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