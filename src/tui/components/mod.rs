//! # TUI Components
//!
//! This module contains all UI components for the terminal interface.
//!
//! ## Component Architecture
//!
//! Components in this directory follow two patterns:
//!
//! ### Stateless Components (Props-Based Rendering)
//!
//! Simple display components that receive all data as parameters:
//! - `TitleBar`: Top status bar showing model name and status
//! - `Message`: Individual conversation message rendering
//!
//! ### Stateful Components (Event-Driven)
//!
//! Components that manage local state and emit events:
//! - `InputBox`: Text input field with effort level indicator
//! - `MessageList`: Scrollable conversation view with layout caching
//!
//! ## Design Philosophy
//!
//! ### Composition Over Inheritance
//!
//! Components compose naturally. For example, `MessageList` renders multiple
//! `Message` components. This mirrors React's component model.
//!
//! ### Co-location of Concerns
//!
//! Each component file contains everything related to that component:
//! - State types
//! - Event types
//! - Rendering logic
//! - Event handling
//! - Tests
//!
//! **Why:** Makes components self-contained and easy to understand. You can
//! read one file to understand how a component works, rather than jumping
//! between multiple files.
//!
//! ### Props-Based Data Flow
//!
//! Components receive external data as "props" (function parameters), not by
//! directly accessing global state. This makes dependencies explicit and
//! components testable.
//!
//! **Example:**
//! ```rust,ignore
//! // Good: Dependencies are explicit
//! title_bar.render(frame, area, &app.model_name, &app.status_message);
//!
//! // Bad: Hidden dependency on global state
//! title_bar.render(frame, area); // reads from global App
//! ```
//!
//! ## Module Structure
//!
//! ```text
//! components/
//! ├── mod.rs           (this file)
//! ├── title_bar.rs     (Top status bar)
//! ├── message.rs       (Single message renderer)
//! ├── message_list.rs  (Scrollable message container)
//! └── input_box/       (Text input with effort indicator)
//! ```
//!
//! ## Migration Status
//!
//! Components will be migrated from the monolithic `ui.rs` module incrementally:
//!
//! - [ ] Phase 1: Infrastructure (component.rs, this file)
//! - [ ] Phase 2: TitleBar, Message (stateless)
//! - [ ] Phase 3: InputBox, MessageList (stateful)
//! - [ ] Phase 4: Integration (wire up in main loop)
//! - [ ] Phase 5: Cleanup (remove old code from ui.rs)

// Re-export components
mod title_bar;
#[allow(unused_imports)] // Used in Phase 4 (integration with main loop)
pub use title_bar::TitleBar;

pub mod input_box;
pub mod message;
pub use input_box::{InputBox, InputEvent};
pub mod message_list;
pub use message_list::{MessageList, MessageListState};
pub mod landing;
pub mod logo;
pub mod session_manager;
pub mod tool_message;
pub use landing::LandingPage;
pub use session_manager::{SessionManager, SessionManagerState};
