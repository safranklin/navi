//! # Actions
//!
//! Everything that can happen in Navi becomes an `Action`.
//! User presses Enter? That's `Action::SubmitMessage`.
//! API responds? That's `Action::ResponseReceived(segment)`.
//!
//! The `update()` function takes the current state and an action,
//! then returns the new state. No side effects here. I/O happens elsewhere.
//!
//! ```text
//! State + Action  →  update()  →  New State
//! ```
//!
//! This makes everything testable: `assert_eq!(update(state, action), expected)`.
//! And debuggable: log every action, replay the exact session.
