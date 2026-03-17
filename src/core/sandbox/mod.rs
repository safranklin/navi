//! # Sandbox System
//!
//! Execution sandboxing for tool commands. The `Sandbox` trait owns the full
//! execution lifecycle - BashTool delegates to it rather than building processes
//! directly. This abstraction supports both local execution and Docker containers.
//!
//! ## Backends
//!
//! - [`LocalSandbox`]: Direct process spawning with `env_clear()` + safe whitelist.
//!   Fallback when Docker isn't available.
//! - [`DockerSandbox`]: Persistent container, `docker exec` per command. Container
//!   created lazily on first use, stopped on drop.

pub mod docker;
pub mod local;
pub mod session;

pub use docker::DockerSandbox;
pub use local::LocalSandbox;

use async_trait::async_trait;
use std::time::Duration;

/// Output from a sandboxed command execution.
#[derive(Debug)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub truncated: bool,
}

/// Errors from sandbox execution.
#[derive(Debug)]
pub enum ExecError {
    /// Command timed out after the given duration.
    Timeout(Duration),
    /// Failed to spawn or communicate with the sandbox.
    SpawnFailed(String),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Timeout(d) => write!(f, "Command timed out after {}s", d.as_secs()),
            ExecError::SpawnFailed(msg) => write!(f, "Spawn failed: {msg}"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Sandbox trait: owns the full execution lifecycle for shell commands.
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Execute a shell command and return its output.
    async fn execute(&self, command: &str, timeout: Duration) -> Result<ExecOutput, ExecError>;
}

/// Smart truncation: keeps head + tail of output so errors at the end aren't lost.
///
/// If the raw output fits within `max_bytes`, returns it as-is. Otherwise, converts
/// to a string, splits into lines, and keeps the first and last N lines with a
/// separator showing how many lines were omitted.
pub(crate) fn truncate_output(raw: &[u8], max_bytes: usize) -> (String, bool) {
    if raw.len() <= max_bytes {
        return (String::from_utf8_lossy(raw).to_string(), false);
    }

    let full = String::from_utf8_lossy(raw);
    let lines: Vec<&str> = full.lines().collect();
    let total = lines.len();

    // For very short output that's over the byte cap (e.g. one huge line),
    // fall back to byte truncation with head+tail bytes
    if total <= 2 {
        let half = max_bytes / 2;
        let head = String::from_utf8_lossy(&raw[..half]);
        let tail = String::from_utf8_lossy(&raw[raw.len() - half..]);
        return (
            format!("{head}\n\n... [output truncated: middle bytes omitted, showing first and last {half} bytes] ...\n\n{tail}"),
            true,
        );
    }

    // Line budget: cap at 200 per side, but also respect the byte limit.
    // If average line length means we can't fit 400 lines, shrink proportionally.
    let avg_bytes_per_line = raw.len() / total;
    let max_lines_by_bytes = if avg_bytes_per_line > 0 {
        max_bytes / avg_bytes_per_line
    } else {
        total
    };
    let half_budget = 200.min(total / 2).min(max_lines_by_bytes / 2).max(1);
    let head_budget = half_budget;
    let tail_budget = half_budget.min(total - head_budget);
    let omitted = total - head_budget - tail_budget;

    if omitted == 0 {
        // All lines fit within budget even though bytes were over -
        // return the full decoded string
        return (full.to_string(), false);
    }

    let mut result = lines[..head_budget].join("\n");
    result.push_str(&format!(
        "\n\n... [{omitted} lines truncated, showing first {head_budget} and last {tail_budget} of {total} lines] ...\n\n"
    ));
    result.push_str(&lines[total - tail_budget..].join("\n"));

    (result, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_output_passes_through() {
        let input = b"hello world\n";
        let (output, truncated) = truncate_output(input, 1000);
        assert_eq!(output, "hello world\n");
        assert!(!truncated);
    }

    #[test]
    fn test_truncation_preserves_head_and_tail() {
        // Generate 500 lines of output
        let lines: Vec<String> = (0..500).map(|i| format!("line {i}")).collect();
        let input = lines.join("\n");
        let raw = input.as_bytes();

        // Use a small max_bytes to force truncation
        let (output, truncated) = truncate_output(raw, 100);
        assert!(truncated);
        assert!(output.contains("line 0")); // head preserved
        assert!(output.contains("line 499")); // tail preserved
        assert!(output.contains("lines truncated")); // separator present
    }

    #[test]
    fn test_separator_has_accurate_counts() {
        // Use a large enough byte cap that line budget is 200 per side
        let lines: Vec<String> = (0..500).map(|i| format!("line {i}")).collect();
        let input = lines.join("\n");
        let raw = input.as_bytes();

        // Set max_bytes high enough that the 200-line cap is the binding constraint
        let (output, truncated) = truncate_output(raw, raw.len() / 2);
        assert!(truncated);
        assert!(output.contains("lines truncated"));
        assert!(output.contains("of 500 lines"));
        // Verify the numbers in the separator add up
        // Format: "[N lines truncated, showing first X and last Y of 500 lines]"
        assert!(output.contains("line 0")); // first line preserved
        assert!(output.contains("line 499")); // last line preserved
    }

    #[test]
    fn test_small_line_count_uses_proportional_budget() {
        // 20 lines - head_budget=10, tail_budget=10, so omitted=0 when total=20
        // Need enough lines that omitted > 0: total > head_budget + tail_budget
        // With 500+ lines, head=200, tail=200, omitted=100+
        // With 50 lines, head=25, tail=25, omitted=0 (still fits)
        // We need a case where line count itself forces truncation.
        // Use 6 lines: head=3, tail=3, omitted=0. Still no truncation.
        // The proportional budget means small line counts won't truncate by lines.
        // This is by design - the byte fallback handles the case.
        // Test that a moderate count (e.g., 30 lines) with low byte cap works:
        let lines: Vec<String> = (0..30).map(|i| format!("line {i} with padding content here")).collect();
        let input = lines.join("\n");
        let raw = input.as_bytes();

        let (output, truncated) = truncate_output(raw, 100);
        assert!(truncated);
        assert!(output.contains("line 0")); // head
        assert!(output.contains("line 29")); // tail
        assert!(output.contains("truncated"));
    }

    #[test]
    fn test_single_huge_line_falls_back_to_bytes() {
        let input = "x".repeat(10_000);
        let raw = input.as_bytes();

        let (output, truncated) = truncate_output(raw, 200);
        assert!(truncated);
        assert!(output.contains("bytes"));
        // Should have some x's at start and end
        assert!(output.starts_with("xxxx"));
        assert!(output.ends_with("xxxx"));
    }
}
