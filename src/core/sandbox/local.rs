//! # Local Sandbox
//!
//! Direct process execution with environment filtering. No filesystem or network
//! restrictions - just prevents accidental env var leakage from the host process.
//! Used as fallback when Docker isn't available.
//!
//! Uses a persistent ShellSession for stateful command execution (cd, env vars
//! persist across calls). Falls back to spawn-per-command if the session dies.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::session::ShellSession;
use super::{truncate_output, ExecError, ExecOutput, Sandbox};

/// Safe environment variables to pass through to child processes.
const SAFE_ENV_VARS: &[&str] = &["PATH", "HOME", "TERM", "LANG", "USER", "SHELL"];

/// Build the safe environment variable list from the host.
fn safe_env() -> Vec<(String, String)> {
    SAFE_ENV_VARS
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect()
}

pub struct LocalSandbox {
    working_dir: PathBuf,
    max_output_bytes: usize,
    session: Mutex<Option<ShellSession>>,
}

impl LocalSandbox {
    pub fn new(working_dir: PathBuf, max_output_bytes: usize) -> Self {
        Self {
            working_dir,
            max_output_bytes,
            session: Mutex::new(None),
        }
    }

    /// Ensure the session is alive, spawning a new one if needed.
    async fn ensure_session(
        session_slot: &mut Option<ShellSession>,
        working_dir: &Path,
    ) -> Result<(), ExecError> {
        let needs_spawn = match session_slot.as_mut() {
            Some(s) => !s.is_alive(),
            None => true,
        };

        if needs_spawn {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            let new_session = ShellSession::spawn(&shell, working_dir, safe_env()).await?;
            *session_slot = Some(new_session);
        }

        Ok(())
    }
}

#[async_trait]
impl Sandbox for LocalSandbox {
    async fn execute(&self, command: &str, timeout: Duration) -> Result<ExecOutput, ExecError> {
        let mut session_guard = self.session.lock().await;

        // Try the persistent session first
        match Self::ensure_session(&mut session_guard, &self.working_dir).await {
            Ok(()) => {
                let session = session_guard.as_mut().unwrap();
                match session.execute(command, timeout).await {
                    Ok(output) => {
                        let raw = output.stdout.as_bytes();
                        let (stdout, truncated) = truncate_output(raw, self.max_output_bytes);
                        return Ok(ExecOutput {
                            stdout,
                            stderr: String::new(), // merged into stdout
                            exit_code: output.exit_code,
                            truncated,
                        });
                    }
                    Err(ExecError::Timeout(d)) => {
                        // Kill the dead session so next call spawns fresh
                        if let Some(s) = session_guard.as_mut() {
                            s.kill().await;
                        }
                        *session_guard = None;
                        return Err(ExecError::Timeout(d));
                    }
                    Err(_) => {
                        // Session died mid-command - kill it and retry once via fallback
                        if let Some(s) = session_guard.as_mut() {
                            s.kill().await;
                        }
                        *session_guard = None;
                    }
                }
            }
            Err(_) => {
                // Session spawn failed - fall through to one-shot
                *session_guard = None;
            }
        }

        // Fallback: spawn-per-command (original behavior)
        drop(session_guard);
        self.execute_oneshot(command, timeout).await
    }

    async fn restart(&self) -> Result<(), ExecError> {
        let mut session_guard = self.session.lock().await;
        if let Some(s) = session_guard.as_mut() {
            s.kill().await;
        }
        *session_guard = None;
        // Lazily recreated on next execute()
        Ok(())
    }
}

impl LocalSandbox {
    /// One-shot execution: spawn a new process per command.
    /// Used as fallback when the persistent session fails.
    async fn execute_oneshot(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<ExecOutput, ExecError> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

        let mut cmd = tokio::process::Command::new(&shell);
        cmd.arg("-c")
            .arg(command)
            .current_dir(&self.working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env_clear();

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
                let (stdout, truncated_out) =
                    truncate_output(&output.stdout, self.max_output_bytes);
                let (stderr, truncated_err) =
                    truncate_output(&output.stderr, self.max_output_bytes);
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
            .execute("false", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(out.exit_code, 1);
    }

    #[tokio::test]
    async fn test_stderr_in_stdout() {
        let sb = test_sandbox();
        let out = sb
            .execute("echo err_msg >&2", Duration::from_secs(5))
            .await
            .unwrap();
        // With persistent session, stderr is merged into stdout
        assert!(out.stdout.contains("err_msg"));
    }

    #[tokio::test]
    async fn test_state_persists_across_calls() {
        let sb = test_sandbox();

        sb.execute("cd /tmp", Duration::from_secs(5))
            .await
            .unwrap();

        let out = sb
            .execute("pwd", Duration::from_secs(5))
            .await
            .unwrap();

        let actual = std::fs::canonicalize(out.stdout.trim()).unwrap_or_default();
        let expected = std::fs::canonicalize("/tmp").unwrap_or_default();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_env_var_persists_across_calls() {
        let sb = test_sandbox();

        sb.execute("export NAVI_TEST_PERSIST=hello", Duration::from_secs(5))
            .await
            .unwrap();

        let out = sb
            .execute("echo $NAVI_TEST_PERSIST", Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(out.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_session_restart() {
        let sb = test_sandbox();

        // Set state
        sb.execute("export NAVI_RESTART_TEST=before", Duration::from_secs(5))
            .await
            .unwrap();

        // Restart
        sb.restart().await.unwrap();

        // State should be gone
        let out = sb
            .execute("echo $NAVI_RESTART_TEST", Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(out.stdout.trim(), "");
    }

    #[tokio::test]
    async fn test_session_recovers_after_timeout() {
        let sb = test_sandbox();

        // Timeout kills the session
        let result = sb
            .execute("sleep 60", Duration::from_millis(100))
            .await;
        assert!(matches!(result, Err(ExecError::Timeout(_))));

        // Next command should work (new session spawned)
        let out = sb
            .execute("echo recovered", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "recovered");
    }
}
