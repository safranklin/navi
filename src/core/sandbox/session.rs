//! # Shell Session
//!
//! Persistent bash session that stays alive across commands. Instead of spawning
//! a new process per command (~180ms for Docker exec, ~1-5ms local), we keep a
//! shell running and send commands over stdin, reading output between sentinel
//! markers.
//!
//! ## Protocol
//!
//! Each command is framed with unique sentinels:
//! ```text
//! echo "___NAVI_START_{uuid}___"
//! {command} 2>&1
//! __navi_exit=$?
//! echo ""
//! echo "___NAVI_END_{uuid}_${__navi_exit}___"
//! ```
//!
//! - UUID per command prevents sentinel collisions with user output
//! - `2>&1` merges stderr into stdout (one stream to read)
//! - Exit code embedded in the end sentinel
//! - Empty echo before end ensures sentinel starts on a new line

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use uuid::Uuid;

use super::ExecError;

/// A persistent shell process that accepts commands over stdin.
pub struct ShellSession {
    child: Child,
    stdin: ChildStdin,
    stdout_reader: BufReader<ChildStdout>,
}

/// Result from a session command execution.
pub struct SessionOutput {
    pub stdout: String,
    pub exit_code: i32,
}

impl ShellSession {
    /// Spawn a new shell session.
    ///
    /// The shell process runs with the given working directory and environment.
    /// stderr is merged into stdout via the command framing protocol, so the
    /// session only reads a single output stream.
    pub async fn spawn(
        shell: &str,
        working_dir: &Path,
        env: Vec<(String, String)>,
    ) -> Result<Self, ExecError> {
        let mut cmd = tokio::process::Command::new(shell);
        cmd.current_dir(working_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()) // not read, but must not block
            .env_clear();

        for (key, val) in &env {
            cmd.env(key, val);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| ExecError::SpawnFailed(format!("Failed to spawn '{shell}': {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ExecError::SpawnFailed("Failed to capture stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ExecError::SpawnFailed("Failed to capture stdout".into()))?;

        let mut session = Self {
            child,
            stdin,
            stdout_reader: BufReader::new(stdout),
        };

        // Redirect stderr to stdout globally for this shell session.
        // This way we only need to read one stream and stderr from any
        // command appears in the output.
        session
            .stdin
            .write_all(b"exec 2>&1\n")
            .await
            .map_err(|e| ExecError::SpawnFailed(format!("Failed to redirect stderr: {e}")))?;
        session
            .stdin
            .flush()
            .await
            .map_err(|e| ExecError::SpawnFailed(format!("Failed to flush stderr redirect: {e}")))?;

        Ok(session)
    }

    /// Execute a command in the persistent session.
    ///
    /// Wraps the command with sentinel markers, writes it to stdin, then reads
    /// stdout line-by-line until the end sentinel appears. The exit code is
    /// parsed from the end sentinel.
    pub async fn execute(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<SessionOutput, ExecError> {
        let id = Uuid::new_v4().to_string().replace('-', "");
        let start_sentinel = format!("___NAVI_START_{id}___");
        let end_prefix = format!("___NAVI_END_{id}_");

        // Frame the command with sentinels.
        // stderr is already merged into stdout via `exec 2>&1` at session startup.
        let framed = format!(
            "echo \"{start_sentinel}\"\n\
             {command}\n\
             __navi_exit=$?\n\
             echo \"\"\n\
             echo \"{end_prefix}${{__navi_exit}}___\"\n"
        );

        // Write to stdin
        self.stdin
            .write_all(framed.as_bytes())
            .await
            .map_err(|e| ExecError::SpawnFailed(format!("Failed to write to session stdin: {e}")))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| ExecError::SpawnFailed(format!("Failed to flush session stdin: {e}")))?;

        // Read until end sentinel, with timeout
        let read_result = tokio::time::timeout(timeout, async {
            let mut output_lines = Vec::new();
            let mut started = false;
            let mut line = String::new();

            loop {
                line.clear();
                let bytes_read = self
                    .stdout_reader
                    .read_line(&mut line)
                    .await
                    .map_err(|e| ExecError::SpawnFailed(format!("Failed to read stdout: {e}")))?;

                if bytes_read == 0 {
                    return Err(ExecError::SpawnFailed(
                        "Shell session ended unexpectedly (EOF)".into(),
                    ));
                }

                let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');

                // Wait for start sentinel
                if !started {
                    if trimmed == start_sentinel {
                        started = true;
                    }
                    continue;
                }

                // Check for end sentinel
                if let Some(rest) = trimmed.strip_prefix(&end_prefix) {
                    if let Some(code_str) = rest.strip_suffix("___") {
                        let exit_code = code_str.parse::<i32>().unwrap_or(-1);
                        // Remove trailing empty line that we added before end sentinel
                        if output_lines.last().map(|l: &String| l.is_empty()).unwrap_or(false) {
                            output_lines.pop();
                        }
                        return Ok(SessionOutput {
                            stdout: output_lines.join("\n"),
                            exit_code,
                        });
                    }
                }

                output_lines.push(trimmed.to_string());
            }
        })
        .await;

        match read_result {
            Ok(result) => result,
            Err(_) => Err(ExecError::Timeout(timeout)),
        }
    }

    /// Check if the shell process is still running.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Kill the session process.
    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn safe_env() -> Vec<(String, String)> {
        ["PATH", "HOME", "TERM", "LANG", "USER", "SHELL"]
            .iter()
            .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
            .collect()
    }

    async fn test_session() -> ShellSession {
        ShellSession::spawn("bash", &std::env::temp_dir(), safe_env())
            .await
            .expect("Failed to spawn test session")
    }

    #[tokio::test]
    async fn test_basic_echo() {
        let mut session = test_session().await;
        let result = session
            .execute("echo hello world", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(result.stdout.trim(), "hello world");
        assert_eq!(result.exit_code, 0);
        session.kill().await;
    }

    #[tokio::test]
    async fn test_state_persistence_cd() {
        let mut session = test_session().await;

        session
            .execute("cd /tmp", Duration::from_secs(5))
            .await
            .unwrap();

        let result = session
            .execute("pwd", Duration::from_secs(5))
            .await
            .unwrap();

        // /tmp might be a symlink (e.g., /private/tmp on macOS)
        let actual = std::fs::canonicalize(result.stdout.trim()).unwrap_or_default();
        let expected = std::fs::canonicalize("/tmp").unwrap_or_default();
        assert_eq!(actual, expected);
        session.kill().await;
    }

    #[tokio::test]
    async fn test_state_persistence_env_var() {
        let mut session = test_session().await;

        session
            .execute("export FOO=bar", Duration::from_secs(5))
            .await
            .unwrap();

        let result = session
            .execute("echo $FOO", Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(result.stdout.trim(), "bar");
        session.kill().await;
    }

    #[tokio::test]
    async fn test_exit_code_propagation() {
        let mut session = test_session().await;

        let result = session
            .execute("exit 42", Duration::from_secs(5))
            .await;

        // After `exit 42`, the shell dies. The session should detect EOF.
        // This is expected - `exit` kills the shell process.
        // In a real scenario, we'd restart the session.
        assert!(result.is_err() || result.unwrap().exit_code == 42);
        session.kill().await;
    }

    #[tokio::test]
    async fn test_nonzero_exit_code() {
        let mut session = test_session().await;

        let result = session
            .execute("false", Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(result.exit_code, 1);
        session.kill().await;
    }

    #[tokio::test]
    async fn test_timeout() {
        let mut session = test_session().await;

        let result = session
            .execute("sleep 60", Duration::from_millis(100))
            .await;

        assert!(matches!(result, Err(ExecError::Timeout(_))));
        // Session is dead after timeout - that's expected
    }

    #[tokio::test]
    async fn test_multiline_output() {
        let mut session = test_session().await;

        let result = session
            .execute("echo line1; echo line2; echo line3", Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(result.stdout, "line1\nline2\nline3");
        assert_eq!(result.exit_code, 0);
        session.kill().await;
    }

    #[tokio::test]
    async fn test_stderr_merged_into_stdout() {
        let mut session = test_session().await;

        let result = session
            .execute("echo out; echo err >&2; echo out2", Duration::from_secs(5))
            .await
            .unwrap();

        // stderr is merged via 2>&1 in the framing, so "err" appears in stdout
        assert!(result.stdout.contains("err"));
        assert!(result.stdout.contains("out"));
        session.kill().await;
    }

    #[tokio::test]
    async fn test_sentinel_not_confused_by_navi_in_output() {
        let mut session = test_session().await;

        // Output containing "NAVI" shouldn't confuse the sentinel parsing
        let result = session
            .execute("echo '___NAVI_START_fake___'", Duration::from_secs(5))
            .await
            .unwrap();

        assert!(result.stdout.contains("___NAVI_START_fake___"));
        assert_eq!(result.exit_code, 0);
        session.kill().await;
    }

    #[tokio::test]
    async fn test_is_alive() {
        let mut session = test_session().await;
        assert!(session.is_alive());
        session.kill().await;
        // Give the process a moment to die
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!session.is_alive());
    }

    #[tokio::test]
    async fn test_multiple_sequential_commands() {
        let mut session = test_session().await;

        for i in 0..5 {
            let result = session
                .execute(&format!("echo cmd_{i}"), Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(result.stdout.trim(), format!("cmd_{i}"));
            assert_eq!(result.exit_code, 0);
        }
        session.kill().await;
    }
}
