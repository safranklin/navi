use ratatui::layout::Rect;
use ratatui::Frame;

/// A reusable UI component.
///
/// Components in this architecture follow the React pattern:
/// - They receive data via props (struct fields).
/// - They may hold internal state (via `&mut State` fields).
/// - They render to a `Frame` within a given `Rect`.
///
/// # Mutability
///
/// The `render` method takes `&mut self` to allow components to:
/// 1. Update internal caches (e.g. layout calculations).
/// 2. Manage presentation state (e.g. scroll offsets) during rendering.
///
/// This aligns with Ratatui's `StatefulWidget` pattern.
pub trait Component {
    /// Render the component into the given area.
    ///
    /// Takes `&mut self` to allow updating internal presentation state
    /// or caches during the render pass.
    fn render(&mut self, frame: &mut Frame, area: Rect);
}

/// A component that handles terminal events.
pub trait EventHandler {
    /// The type of high-level event this component emits.
    type Event;

    /// Handle a low-level `TuiEvent` and optionally return a high-level event.
    fn handle_event(&mut self, event: &super::event::TuiEvent) -> Option<Self::Event>;
}
