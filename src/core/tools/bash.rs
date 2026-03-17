//! # Bash Tool
//!
//! Executes shell commands via the Sandbox abstraction. BashTool is a thin adapter
//! between the Tool trait and whatever Sandbox backend is active (Docker or local).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::sandbox::Sandbox;

use super::{Tool, ToolError, ToolPermission};

pub struct BashTool {
    sandbox: Arc<dyn Sandbox>,
    timeout: Duration,
}

impl BashTool {
    pub fn new(sandbox: Arc<dyn Sandbox>, timeout: Duration) -> Self {
        Self { sandbox, timeout }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct BashArgs {
    /// The shell command to execute.
    pub command: String,
}

#[derive(Debug, Serialize)]
pub struct BashOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub truncated: bool,
}

#[async_trait]
impl Tool for BashTool {
    const NAME: &'static str = "bash";
    const DESCRIPTION: &'static str = "\
        Executes a shell command and returns stdout, stderr, exit code, and a truncated flag. \
        Commands run sandboxed to the project's working directory with a 120-second timeout. \
        Output is capped at 100KB per stream; if exceeded, the output is smart-truncated \
        (first ~200 lines + last ~200 lines with a separator showing omitted count) \
        and the truncated field is true. Errors at the end of output are always visible. \
        This is your primary tool for all file operations and system interaction. \
        Optimize for context window - never dump entire files when you only need part of them: \
        `head -n 50 file` for the top, `tail -n 20 file` for the end, \
        `sed -n '10,25p' file` for a specific range, \
        `grep -n 'pattern' file` to find lines, `grep -C 3 'pattern' file` for context, \
        `wc -l file` to check size before reading, \
        `find . -name '*.rs' | head -20` to explore structure, \
        `cat file | head -100` as a safe preview. \
        Chain commands with pipes and use `&&` for sequential operations.";
    const PERMISSION: ToolPermission = ToolPermission::Prompt;

    type Args = BashArgs;
    type Output = BashOutput;

    async fn call(&self, args: BashArgs) -> Result<BashOutput, ToolError> {
        match self.sandbox.execute(&args.command, self.timeout).await {
            Ok(out) => Ok(BashOutput {
                stdout: out.stdout,
                stderr: out.stderr,
                exit_code: out.exit_code,
                truncated: out.truncated,
            }),
            Err(e) => Err(ToolError(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sandbox::LocalSandbox;

    fn test_bash_tool() -> BashTool {
        let sandbox = Arc::new(LocalSandbox::new(std::env::temp_dir(), 100_000));
        BashTool::new(sandbox, Duration::from_secs(10))
    }

    #[tokio::test]
    async fn test_echo_stdout() {
        let tool = test_bash_tool();
        let result = tool
            .call(BashArgs {
                command: "echo hello world".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(result.stdout.trim(), "hello world");
        assert_eq!(result.exit_code, 0);
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn test_exit_code_nonzero() {
        let tool = test_bash_tool();
        let result = tool
            .call(BashArgs {
                command: "exit 42".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    async fn test_stderr_capture() {
        let tool = test_bash_tool();
        let result = tool
            .call(BashArgs {
                command: "echo error_msg >&2".to_string(),
            })
            .await
            .unwrap();
        assert!(result.stderr.contains("error_msg"));
        assert!(result.stdout.trim().is_empty());
    }

    #[tokio::test]
    async fn test_timeout_returns_error() {
        let sandbox = Arc::new(LocalSandbox::new(std::env::temp_dir(), 100_000));
        let tool = BashTool::new(sandbox, Duration::from_millis(100));
        let result = tool
            .call(BashArgs {
                command: "sleep 60".to_string(),
            })
            .await;
        let err = result.unwrap_err();
        assert!(err.0.contains("timed out"), "got: {}", err.0);
    }

    #[tokio::test]
    async fn test_working_directory() {
        let dir = std::env::temp_dir();
        let sandbox = Arc::new(LocalSandbox::new(dir.clone(), 100_000));
        let tool = BashTool::new(sandbox, Duration::from_secs(5));
        let result = tool
            .call(BashArgs {
                command: "pwd".to_string(),
            })
            .await
            .unwrap();
        let expected = std::fs::canonicalize(&dir).unwrap();
        let actual = std::fs::canonicalize(result.stdout.trim()).unwrap();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_schema_has_command_field() {
        let schema = schemars::schema_for!(BashArgs);
        let value = serde_json::to_value(schema).unwrap();
        let props = value.get("properties").expect("should have properties");
        assert!(props.get("command").is_some());
    }

    #[tokio::test]
    async fn test_truncation_flag() {
        let sandbox = Arc::new(LocalSandbox::new(std::env::temp_dir(), 50));
        let tool = BashTool::new(sandbox, Duration::from_secs(5));
        let result = tool
            .call(BashArgs {
                command: "printf '%0.sx' $(seq 1 200)".to_string(),
            })
            .await
            .unwrap();
        assert!(result.truncated);
        assert!(result.stdout.contains("truncated"));
    }
}
