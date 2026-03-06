//! # TitleBar Component
//!
//! Single-line status bar: navi branding, loading spinner, model (provider),
//! session title, and session token count.

use crate::tui::component::Component;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Braille circle-worm spinner frames (standard CLI pattern).
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub struct TitleBar<'a> {
    model_name: &'a str,
    provider_name: &'a str,
    is_loading: bool,
    spinner_frame: usize,
    session_title: &'a str,
    session_total_tokens: u32,
}

impl<'a> TitleBar<'a> {
    pub fn new(
        model_name: &'a str,
        provider_name: &'a str,
        is_loading: bool,
        spinner_frame: usize,
        session_title: &'a str,
        session_total_tokens: u32,
    ) -> Self {
        Self {
            model_name,
            provider_name,
            is_loading,
            spinner_frame,
            session_title,
            session_total_tokens,
        }
    }
}

/// Format a token count compactly: "1.2k" for >= 1000, raw number otherwise.
fn format_tokens(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

impl Component for TitleBar<'_> {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let sep = Span::styled(" │ ", Style::default().fg(Color::DarkGray));

        // -- Left side: navi + spinner + model (provider) --
        let mut left: Vec<Span> = Vec::new();

        left.push(Span::styled(
            "navi",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ));

        if self.is_loading {
            let ch = SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()];
            left.push(Span::styled(
                format!(" {ch}"),
                Style::default().fg(Color::Blue),
            ));
        }

        left.push(sep.clone());

        left.push(Span::styled(
            self.model_name,
            Style::default().fg(Color::White),
        ));

        if !self.provider_name.is_empty() {
            left.push(Span::styled(
                format!(" ({})", self.provider_name),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // -- Right side: session title + session tokens --
        let mut right: Vec<Span> = Vec::new();

        if !self.session_title.is_empty() {
            right.push(Span::styled(
                self.session_title,
                Style::default().fg(Color::DarkGray),
            ));
        }

        if self.session_total_tokens > 0 {
            if !right.is_empty() {
                right.push(sep);
            }
            right.push(Span::styled(
                format!("{} tokens", format_tokens(self.session_total_tokens)),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // -- Compose: left + padding + right --
        let left_width: usize = left.iter().map(|s| s.width()).sum();
        let right_width: usize = right.iter().map(|s| s.width()).sum();
        let gap = (area.width as usize).saturating_sub(left_width + right_width);

        let mut spans = left;
        spans.push(Span::raw(" ".repeat(gap)));
        spans.extend(right);

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(width: u16, bar: &mut TitleBar) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| bar.render(f, f.area())).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
    }

    fn bar<'a>(
        model: &'a str,
        provider: &'a str,
        loading: bool,
        title: &'a str,
        tokens: u32,
    ) -> TitleBar<'a> {
        TitleBar::new(model, provider, loading, 0, title, tokens)
    }

    #[test]
    fn test_basic_rendering() {
        let mut b = bar("gpt-4", "openrouter", false, "", 0);
        let text = render(80, &mut b);
        assert!(text.contains("navi"));
        assert!(text.contains("gpt-4"));
        assert!(text.contains("(openrouter)"));
    }

    #[test]
    fn test_spinner_shown_when_loading() {
        let mut b = bar("gpt-4", "", true, "", 0);
        let text = render(80, &mut b);
        assert!(SPINNER_FRAMES.iter().any(|&ch| text.contains(ch)));
    }

    #[test]
    fn test_spinner_hidden_when_not_loading() {
        let mut b = bar("gpt-4", "", false, "", 0);
        let text = render(80, &mut b);
        assert!(!SPINNER_FRAMES.iter().any(|&ch| text.contains(ch)));
    }

    #[test]
    fn test_provider_shown() {
        let mut b = bar("claude-sonnet", "openrouter", false, "", 0);
        let text = render(80, &mut b);
        assert!(text.contains("(openrouter)"));
    }

    #[test]
    fn test_empty_provider_hidden() {
        let mut b = bar("gpt-4", "", false, "", 0);
        let text = render(80, &mut b);
        assert!(!text.contains("()"));
    }

    #[test]
    fn test_session_title_shown() {
        let mut b = bar("gpt-4", "", false, "My Chat", 0);
        let text = render(80, &mut b);
        assert!(text.contains("My Chat"));
    }

    #[test]
    fn test_empty_session_title_hidden() {
        let mut b = bar("gpt-4", "", false, "", 0);
        let text = render(80, &mut b);
        // Empty title should not add extra spacing or artifacts
        assert!(text.contains("navi"));
    }

    #[test]
    fn test_session_tokens_shown() {
        let mut b = bar("gpt-4", "", false, "", 2500);
        let text = render(80, &mut b);
        assert!(text.contains("2.5k tokens"));
    }

    #[test]
    fn test_zero_tokens_hidden() {
        let mut b = bar("gpt-4", "", false, "", 0);
        let text = render(80, &mut b);
        assert!(!text.contains("tokens"));
    }

    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(42), "42");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn test_format_tokens_thousands() {
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(23456), "23.5k");
    }

    #[test]
    fn test_format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }
}
