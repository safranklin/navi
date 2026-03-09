//! # Tool Permission System
//!
//! Controls whether a tool can auto-execute or requires user approval.
//! For v1, two levels: `Safe` (auto-execute) and `Prompt` (ask first).

/// Permission level for a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermission {
    /// Tool is safe to execute without user confirmation.
    Safe,
    /// Tool requires explicit user approval before execution.
    Prompt,
}
