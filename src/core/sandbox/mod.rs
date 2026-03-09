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
