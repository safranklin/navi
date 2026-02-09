//! # Landing Page Component
//!
//! Displays an animated ASCII art sequence when the conversation is empty.
//!

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::style::{Style, Color};
use ratatui::layout::Alignment;
use crate::tui::component::Component;
use crate::tui::components::logo::Logo;

pub struct LandingPage {
    frame_index: usize,
}

impl LandingPage {
    pub fn new(frame_index: usize) -> Self {
        Self { frame_index }
    }
}

impl Component for LandingPage {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        use ratatui::text::{Line, Span};
        use ratatui::style::Modifier;
        use ratatui::layout::{Layout, Constraint, Flex};

        // --- Prepare Text for Layout Calculation ---
        let mut text_lines = Vec::new();
        // Spacer is handled by layout splitting now
        
        text_lines.push(Line::from(Span::styled(
            "Hey! Listen!",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        )));

        text_lines.push(Line::from(Span::styled(
            "Navi",
            Style::default().fg(Color::DarkGray)
        )));

        let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
        text_lines.push(Line::from(Span::styled(
            version_text,
            Style::default().fg(Color::DarkGray)
        )));
        
        // --- Calculate Layout ---
        // We want the Canvas to take up natural height of the fairy
        // And the text to be below it.
        // We want the whole group centered vertically.
        
        let canvas_height = Logo::required_height();
        
        // Calculate text height
        let text_height = text_lines.len() as u16;

        let vertical_layout = Layout::vertical([
            Constraint::Length(canvas_height),
            Constraint::Length(1), // Spacer
            Constraint::Length(text_height),
        ]).flex(Flex::Center)
        .split(area);
        
        // --- Render Canvas (Logo) ---
        Logo::render(frame, vertical_layout[0], self.frame_index);

        // --- Render Text ---
        let paragraph = Paragraph::new(text_lines)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, vertical_layout[2]);
    }
}
