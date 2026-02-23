//! # TitleBar Component
//!
//! Top status bar showing application state and notifications.
//!
//! ## Responsibilities
//!
//! - Display current model name
//! - Display status messages (e.g., "Loading...", "Reasoning: Extended")
//! - Show "↓ New" indicator when there's unseen content below scroll
//!
//! ## Design Decisions
//!
//! ### Stateless Component
//!
//! TitleBar is purely presentational—it receives all data as props and has no
//! internal state. This makes it trivial to test and reason about:
//!
//! ```rust,ignore
//! let title_bar = TitleBar {
//!     model_name: "gpt-4".to_string(),
//!     status_message: "Loading...".to_string(),
//!     has_unseen_content: true,
//! };
//! title_bar.render(frame, area);
//! ```
//!
//! ### Props-in-Struct Pattern
//!
//! Rather than passing props as render() parameters, we store them as struct
//! fields. This is necessary for trait-based polymorphism—the Component trait
//! requires a fixed render() signature:
//!
//! ```rust,ignore
//! fn render_component<C: Component>(c: &C, frame: &mut Frame, area: Rect) {
//!     c.render(frame, area);  // Works for any component
//! }
//! ```
//!
//! ### State Ownership
//!
//! All three props come from different sources:
//! - `model_name`: Core App state (configuration)
//! - `status_message`: Core App state (computed from effort level, errors, etc.)
//! - `has_unseen_content`: TUI state (scroll position indicator)
//!
//! The TitleBar doesn't care where they come from—it just renders what it's given.
//! This decoupling makes it reusable.
//!
//! ## Conditional Formatting
//!
//! The title text changes based on state:
//!
//! 1. **Unseen content**: `"Navi Interface (model: gpt-4) | Loading... | ↓ New"`
//! 2. **Status message**: `"Navi Interface (model: gpt-4) | Loading..."`
//! 3. **Default**: `"Navi Interface (model: gpt-4)"`
//!
//! This priority order ensures the most important information is always visible,
//! even on narrow terminals.

use crate::tui::component::Component;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;

/// Top status bar component showing model name, status, and notifications.
///
/// # Props
///
/// All fields are "props" (configuration from parent):
/// - `model_name`: The current LLM model (e.g., "gpt-4", "claude-sonnet-3.5")
/// - `status_message`: Transient status (e.g., "Loading...", "Reasoning: Extended")
/// - `has_unseen_content`: Whether there's content below current scroll position
///
/// # Example
///
/// ```rust,ignore
/// let mut title_bar = TitleBar {
///     model_name: app.model_name.clone(),
///     status_message: app.status_message.clone(),
///     has_unseen_content: tui.has_unseen_content,
/// };
///
/// // Later, update props when state changes
/// title_bar.model_name = new_model.clone();
/// title_bar.render(frame, title_area);
/// ```
#[allow(dead_code)] // Used in Phase 4 (integration with main loop)
pub struct TitleBar {
    /// Current model name (e.g., "gpt-4")
    pub model_name: String,
    /// Status message (e.g., "Loading...", "Reasoning: Extended")
    pub status_message: String,
    /// Whether there's content below the current scroll position
    pub has_unseen_content: bool,
}

impl TitleBar {
    /// Create a new TitleBar with the given props.
    ///
    /// # Design: Why provide a constructor?
    ///
    /// While fields are public and you *could* construct with struct literal syntax,
    /// providing `new()` gives us a stable API. If we later add internal state or
    /// validation, existing code won't break.
    #[allow(dead_code)] // Used in Phase 4 (integration with main loop)
    pub fn new(model_name: String, status_message: String, has_unseen_content: bool) -> Self {
        Self {
            model_name,
            status_message,
            has_unseen_content,
        }
    }
}

impl Component for TitleBar {
    /// Render the title bar as a single line with conditional formatting.
    ///
    /// # Layout
    ///
    /// The title bar is always a single line (height 1). It shows:
    /// - Model name (always visible)
    /// - Status message (if present)
    /// - "↓ New" indicator (if has_unseen_content)
    ///
    /// # Design: Why not use a Block widget?
    ///
    /// We use a plain Span rather than a Block because:
    /// 1. Title bar is always 1 line—no need for borders or padding
    /// 2. Span is lighter weight (no border rendering overhead)
    /// 3. Simpler to test (just check the text content)
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title_text = if self.has_unseen_content {
            format!(
                "Navi Interface (model: {}) | {} | ↓ New",
                self.model_name, self.status_message
            )
        } else if self.status_message.is_empty() {
            format!("Navi Interface (model: {})", self.model_name)
        } else {
            format!(
                "Navi Interface (model: {}) | {}",
                self.model_name, self.status_message
            )
        };

        frame.render_widget(Span::raw(title_text), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_title_bar_new() {
        let title_bar = TitleBar::new("gpt-4".to_string(), "Loading...".to_string(), false);

        assert_eq!(title_bar.model_name, "gpt-4");
        assert_eq!(title_bar.status_message, "Loading...");
        assert!(!title_bar.has_unseen_content);
    }

    #[test]
    fn test_title_bar_with_unseen_content() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut title_bar =
            TitleBar::new("gpt-4".to_string(), "Reasoning: Extended".to_string(), true);

        terminal
            .draw(|f| {
                title_bar.render(f, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();

        assert!(text.contains("Navi Interface"));
        assert!(text.contains("gpt-4"));
        assert!(text.contains("Reasoning: Extended"));
        assert!(text.contains("↓ New"));
    }

    #[test]
    fn test_title_bar_with_status_message() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut title_bar = TitleBar::new(
            "claude-sonnet-3.5".to_string(),
            "Thinking...".to_string(),
            false,
        );

        terminal
            .draw(|f| {
                title_bar.render(f, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();

        assert!(text.contains("Navi Interface"));
        assert!(text.contains("claude-sonnet-3.5"));
        assert!(text.contains("Thinking..."));
        assert!(!text.contains("↓ New"));
    }

    #[test]
    fn test_title_bar_default_no_status() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut title_bar = TitleBar::new("gpt-4".to_string(), "".to_string(), false);

        terminal
            .draw(|f| {
                title_bar.render(f, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();

        assert!(text.contains("Navi Interface"));
        assert!(text.contains("gpt-4"));
        assert!(!text.contains('|'));
        assert!(!text.contains("↓ New"));
    }

    #[test]
    fn test_title_bar_props_are_mutable() {
        let mut title_bar = TitleBar::new("gpt-4".to_string(), "".to_string(), false);

        // Simulate updating props when app state changes
        title_bar.model_name = "claude-opus".to_string();
        title_bar.status_message = "Reasoning: Extended".to_string();
        title_bar.has_unseen_content = true;

        assert_eq!(title_bar.model_name, "claude-opus");
        assert_eq!(title_bar.status_message, "Reasoning: Extended");
        assert!(title_bar.has_unseen_content);
    }
}
