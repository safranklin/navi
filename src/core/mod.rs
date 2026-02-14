//! # Core Application Logic
//!
//! This module contains Navi's Business logic.
//! It knows nothing about any specific UI technology.
//!
//! ```text
//!                    ┌─────────────────────────┐
//!                    │         CORE            │
//!                    │  (this module)          │
//!                    │                         │
//!                    │  • State (app data)     │
//!                    │  • Action (events)      │
//!                    │  • update() (reducer)   │
//!                    │                         │
//!                    │  No I/O. No UI. Pure.   │
//!                    └───────────┬─────────────┘
//!                                │
//!            ┌───────────────────┼───────────────────┐
//!            ▼                   ▼                   ▼
//!     ┌────────────┐      ┌────────────┐      ┌────────────┐
//!     │    TUI     │      │   React    │      │    API     │
//!     │  Adapter   │      │  Adapter   │      │  (future)  │
//!     │ (ratatui)  │      │  (future)  │      │            │
//!     └────────────┘      └────────────┘      └────────────┘
//! ```
//!
//! ## Modules
//!
//! - [`state`]: The `App` struct — all application state in one place
//! - [`action`]: The `Action` enum — everything that can happen in the app

pub mod action;
pub mod state;
pub mod tools;

// Re-export commonly used types for convenience
// pub use action::Action;
// pub use state::App;
