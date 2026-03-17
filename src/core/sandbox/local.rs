//! # Local Sandbox
//!
//! Direct process execution with environment filtering. No filesystem or network
//! restrictions - just prevents accidental env var leakage from the host process.
//! Used as fallback when Docker isn't available.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use super::{truncate_output, ExecError, ExecOutput, Sandbox};

/// Safe environment variables to pass through to child processes.
const SAFE_ENV_VARS: &[&str] = &["PATH", "HOME", "TERM", "LANG", "USER", "SHELL"];

pub struct LocalSandbox {
    working_dir: PathBuf,
    max_output_bytes: usize,
}

impl LocalSandbox {
    pub fn new(working_dir: PathBuf, max_output_bytes: usize) -> Self {
        Self {
            working_dir,
            max_output_bytes,
        }
    }
}

#[async_trait]
impl Sandbox for LocalSandbox {
    async fn execute(&self, command: &str, timeout: Duration) -> Result<ExecOutput, ExecError> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

        let mut cmd = tokio::process::Command::new(&shell);
        cmd.arg("-c")
            .arg(command)
            .current_dir(&self.working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env_clear();

        // Whitelist safe env vars
        for var in SAFE_ENV_VARS {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }

        let child = cmd
            .spawn()
            .map_err(|e| ExecError::SpawnFailed(format!("Failed to spawn '{}': {}", shell, e)))?;

        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let (stdout, truncated_out) = truncate_output(&output.stdout, self.max_output_bytes);
                let (stderr, truncated_err) = truncate_output(&output.stderr, self.max_output_bytes);
                let truncated = truncated_out || truncated_err;

                Ok(ExecOutput {
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    truncated,
                })
            }
            Ok(Err(e)) => Err(ExecError::SpawnFailed(format!("Process error: {e}"))),
            Err(_) => Err(ExecError::Timeout(timeout)),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn test_sandbox() -> LocalSandbox {
        LocalSandbox::new(std::env::temp_dir(), 100_000)
    }

    #[tokio::test]
    async fn test_echo() {
        let sb = test_sandbox();
        let out = sb
            .execute("echo hello world", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "hello world");
        assert_eq!(out.exit_code, 0);
        assert!(!out.truncated);
    }

    #[tokio::test]
    async fn test_env_is_filtered() {
        // Set a custom env var that should NOT leak through
        unsafe { std::env::set_var("NAVI_TEST_SECRET", "leaked") };
        let sb = test_sandbox();
        let out = sb
            .execute("echo $NAVI_TEST_SECRET", Duration::from_secs(5))
            .await
            .unwrap();
        // Should be empty because env_clear filters it out
        assert_eq!(out.stdout.trim(), "");
        unsafe { std::env::remove_var("NAVI_TEST_SECRET") };
    }

    #[tokio::test]
    async fn test_truncation() {
        let sb = LocalSandbox::new(std::env::temp_dir(), 50);
        let out = sb
            .execute(
                "python3 -c 'print(\"x\" * 200)' 2>/dev/null || echo $(head -c 200 /dev/zero | tr '\\0' 'x')",
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert!(out.truncated);
        assert!(out.stdout.contains("truncated"));
    }

    #[tokio::test]
    async fn test_timeout() {
        let sb = test_sandbox();
        let result = sb
            .execute("sleep 60", Duration::from_millis(100))
            .await;
        assert!(matches!(result, Err(ExecError::Timeout(_))));
    }

    #[tokio::test]
    async fn test_exit_code() {
        let sb = test_sandbox();
        let out = sb
            .execute("exit 42", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(out.exit_code, 42);
    }

    #[tokio::test]
    async fn test_stderr() {
        let sb = test_sandbox();
        let out = sb
            .execute("echo err_msg >&2", Duration::from_secs(5))
            .await
            .unwrap();
        assert!(out.stderr.contains("err_msg"));
        assert!(out.stdout.trim().is_empty());
    }
}
