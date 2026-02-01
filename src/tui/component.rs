//! # Component Trait and Infrastructure
//!
//! Defines the component architecture pattern for the TUI layer.
//!
//! ## Architecture Overview
//!
//! This module implements a **hybrid architecture**:
//! - **Core layer**: Elm-style (Action/Effect/update) for business logic
//! - **TUI layer**: React-style components for UI composition
//!
//! Components are the building blocks of the TUI. They encapsulate:
//! - Props (configuration data passed from parent)
//! - Local presentation state (e.g., input buffer, hover state)
//! - Event handling logic (keyboard, mouse)
//! - Rendering logic
//!
//! ## Trait Composition Pattern
//!
//! Components use **two separate traits** following Rust's composition idiom:
//!
//! ### 1. `Component` - For Rendering
//!
//! All components implement this trait. It defines a uniform rendering interface:
//!
//! ```rust,ignore
//! pub struct TitleBar {
//!     pub model_name: String,        // Props stored in struct
//!     pub status_message: String,
//!     pub has_unseen_content: bool,
//! }
//!
//! impl Component for TitleBar {
//!     fn render(&self, frame: &mut Frame, area: Rect) {
//!         // Read from self.model_name, etc.
//!     }
//! }
//! ```
//!
//! ### 2. `EventHandler` - For Interaction
//!
//! Only interactive components implement this trait:
//!
//! ```rust,ignore
//! pub struct InputBox {
//!     pub buffer: String,  // State
//! }
//!
//! impl Component for InputBox {
//!     fn render(&self, frame: &mut Frame, area: Rect) { /* ... */ }
//! }
//!
//! impl EventHandler for InputBox {
//!     type Event = InputEvent;
//!     fn handle_event(&mut self, event: &TuiEvent) -> Option<Self::Event> {
//!         // Handle keyboard input, emit Submit event, etc.
//!     }
//! }
//! ```
//!
//! ## Design Rationale
//!
//! ### Why Two Traits Instead of One?
//!
//! This follows Rust's philosophy of **small, composable traits**:
//!
//! - `Iterator` vs `DoubleEndedIterator`
//! - `Read` vs `Write` vs `Seek`
//! - `Clone` vs `Copy`
//!
//! **Benefits:**
//! 1. **Explicit capabilities**: Type system tells you if a component handles events
//! 2. **No unused methods**: Stateless components don't have empty `handle_event()` stubs
//! 3. **Flexible composition**: Future traits like `Focusable`, `Scrollable` can be added
//! 4. **Generic code**: Can write functions that require only the traits they need
//!
//! ```rust,ignore
//! // Only requires rendering
//! fn render_component(c: &impl Component, frame: &mut Frame, area: Rect) {
//!     c.render(frame, area);
//! }
//!
//! // Requires both rendering and event handling
//! fn interactive_component(c: &mut (impl Component + EventHandler)) {
//!     // Can call both render() and handle_event()
//! }
//! ```
//!
//! ### Props vs State
//!
//! Components store two kinds of data:
//!
//! **Props (from parent)**: Configuration or external state
//! - Updated by recreating the component or mutating fields
//! - Example: `title_bar.model_name = "gpt-4".to_string()`
//!
//! **State (internal)**: Component-local presentation state
//! - Updated via `handle_event()` or internal logic
//! - Example: `input_box.buffer.push('a')`
//!
//! **Rule:** If the data comes from `App` (core state), it's a prop. If it's
//! TUI-specific and temporary, it's internal state.
//!
//! ### State Ownership Rules
//!
//! State lives in components when it's **TUI-specific presentation state**:
//! - ✅ Input buffer (user hasn't submitted yet)
//! - ✅ Hover index (visual-only state)
//! - ✅ Layout cache (rendering optimization)
//!
//! State lives in the core `App` when it's **domain state**:
//! - ✅ Conversation context (business data)
//! - ✅ Effort level (user preference that affects API calls)
//! - ✅ Model name (configuration)
//!
//! **Why this matters:** Keeping TUI state separate means we can swap out the
//! TUI for a web interface without changing core business logic.
//!
//! ## Event Flow
//!
//! Components follow React-style unidirectional data flow:
//!
//! ```text
//! Terminal Event → EventHandler.handle_event() → ComponentEvent
//!                         ↓
//!                    Main Loop
//!                         ↓
//!                  Action (Elm core)
//!                         ↓
//!                   update() → Effect
//!                         ↓
//!             Update component props from new App state
//!                         ↓
//!                Component.render() with updated props
//! ```
//!
//! Components emit high-level semantic events (e.g., `InputEvent::Submit`),
//! not low-level terminal events. The main loop translates these to core `Action`s.
//!
//! **Why:** This keeps components decoupled from business logic. An InputBox
//! doesn't know what "submit" means to the application—it just emits the event.

use ratatui::Frame;
use ratatui::layout::Rect;
use crate::tui::event::TuiEvent;

/// Core rendering abstraction for all UI components.
///
/// Every component implements this trait to provide a uniform rendering interface.
/// This enables generic rendering code and consistent component composition.
///
/// # Props Pattern
///
/// Components store their "props" (configuration/external state) as struct fields:
///
/// ```rust,ignore
/// pub struct TitleBar {
///     pub model_name: String,        // Props from parent
///     pub status_message: String,
///     pub has_unseen_content: bool,
/// }
///
/// impl Component for TitleBar {
///     fn render(&self, frame: &mut Frame, area: Rect) {
///         // Access props via self.model_name, self.status_message, etc.
///         let title = Paragraph::new(self.model_name.as_str());
///         frame.render_widget(title, area);
///     }
/// }
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// // Update props by mutating fields (cheap for Copy types, clone for String)
/// title_bar.model_name = app.model_name.clone();
/// title_bar.status_message = app.status_message.clone();
///
/// // Render with updated props
/// title_bar.render(frame, title_area);
/// ```
///
/// # Design: Why store props in struct instead of passing as parameters?
///
/// With trait-based rendering, the signature must be fixed. We can't have:
/// ```rust,ignore
/// title_bar.render(frame, area, model_name, status); // Can't vary params per component
/// ```
///
/// Storing props in the struct gives us flexibility while maintaining a uniform interface.
#[allow(dead_code)] // Used starting in Phase 2 (TitleBar, Message components)
pub trait Component {
    /// Render the component to the terminal.
    ///
    /// # Arguments
    ///
    /// - `frame`: Ratatui frame for rendering widgets
    /// - `area`: The rectangular region to render into
    ///
    /// # Implementation Notes
    ///
    /// Read props from `self` (e.g., `self.model_name`) and render accordingly.
    /// This method should be pure—no side effects, no state mutation, just rendering.
    fn render(&self, frame: &mut Frame, area: Rect);
}

/// Event handling abstraction for interactive components.
///
/// Only components that respond to user input implement this trait. Stateless
/// display components (like `TitleBar`) don't need event handling.
///
/// # Design: Separate Trait
///
/// Following Rust's trait composition pattern (like `Iterator` + `DoubleEndedIterator`),
/// we keep event handling separate from rendering. This means:
///
/// - Type system knows which components are interactive: `impl Component + EventHandler`
/// - No empty `handle_event()` stubs for stateless components
/// - Can write generic code requiring only the traits needed
///
/// # Event Emission
///
/// Components emit **high-level semantic events**, not low-level terminal events:
///
/// ```rust,ignore
/// pub enum InputEvent {
///     Submit(String),      // Not "Enter key pressed"
///     CharEntered(char),   // Not "Key event received"
/// }
/// ```
///
/// The main loop translates these to core `Action`s.
///
/// # Example
///
/// ```rust,ignore
/// pub struct InputBox {
///     pub buffer: String,  // Internal state
/// }
///
/// impl Component for InputBox {
///     fn render(&self, frame: &mut Frame, area: Rect) {
///         let input = Paragraph::new(self.buffer.as_str());
///         frame.render_widget(input, area);
///     }
/// }
///
/// impl EventHandler for InputBox {
///     type Event = InputEvent;
///
///     fn handle_event(&mut self, event: &TuiEvent) -> Option<Self::Event> {
///         match event {
///             TuiEvent::InputChar(c) => {
///                 self.buffer.push(*c);
///                 Some(InputEvent::CharEntered(*c))
///             }
///             TuiEvent::Submit => {
///                 let text = std::mem::take(&mut self.buffer);
///                 Some(InputEvent::Submit(text))
///             }
///             _ => None // Ignore irrelevant events
///         }
///     }
/// }
/// ```
#[allow(dead_code)] // Used starting in Phase 3 (InputBox, MessageList components)
pub trait EventHandler {
    /// High-level events this component emits.
    ///
    /// These should be semantic events (e.g., `Submit`, `SelectionChanged`),
    /// not raw terminal events (e.g., `KeyPress`). The main loop translates
    /// component events into core domain `Action`s.
    type Event;

    /// Handle a terminal input event.
    ///
    /// Returns `Some(Event)` if the component handled the event and wants to
    /// emit a high-level event to its parent. Returns `None` if the event
    /// was ignored or handled entirely internally.
    ///
    /// # Design: Why return Option?
    ///
    /// Components filter irrelevant events. An `InputBox` doesn't care about
    /// scroll or mouse events—returning `None` tells the parent to handle it.
    ///
    /// This is more efficient than forcing the parent to pattern match on
    /// a "NoOp" event variant.
    ///
    /// # Mutation
    ///
    /// Components can mutate internal state here (e.g., update text buffer,
    /// change hover index). Props should generally not be mutated—those come
    /// from the parent and are updated via field assignment.
    fn handle_event(&mut self, event: &TuiEvent) -> Option<Self::Event>;
}
