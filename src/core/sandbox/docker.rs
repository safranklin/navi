//! # Docker Sandbox
//!
//! Executes commands inside a persistent Docker container with a persistent
//! shell session. The container is created lazily on first use and stopped
//! on drop. CWD is volume-mounted as /workspace.
//!
//! Uses a ShellSession attached via `docker exec -i` for single-digit ms
//! per-command overhead (vs ~180ms for exec-per-command). Falls back to
//! exec-per-command if the session fails.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex, OnceCell};

use super::session::ShellSession;
use super::{truncate_output, ExecError, ExecOutput, Sandbox};

pub struct DockerSandbox {
    container_id: OnceCell<String>,
    image: String,
    working_dir: PathBuf,
    max_output_bytes: usize,
    session: Mutex<Option<ShellSession>>,
}

impl DockerSandbox {
    pub fn new(working_dir: PathBuf, image: &str, max_output_bytes: usize) -> Self {
        Self {
            container_id: OnceCell::new(),
            image: image.to_string(),
            working_dir,
            max_output_bytes,
            session: Mutex::new(None),
        }
    }

    /// Check if Docker is available by running `docker info`.
    pub fn is_available() -> bool {
        std::process::Command::new("docker")
            .arg("info")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Create the container lazily and return its ID.
    async fn get_or_create_container(&self) -> Result<&str, ExecError> {
        self.container_id
            .get_or_try_init(|| async {
                let cwd = self.working_dir.to_string_lossy();
                let output = tokio::process::Command::new("docker")
                    .args([
                        "run",
                        "-d",
                        "--rm",
                        "-v",
                        &format!("{cwd}:/workspace"),
                        "-w",
                        "/workspace",
                        &self.image,
                        "tail",
                        "-f",
                        "/dev/null",
                    ])
                    .output()
                    .await
                    .map_err(|e| ExecError::SpawnFailed(format!("docker run failed: {e}")))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(ExecError::SpawnFailed(format!(
                        "docker run exited {}: {}",
                        output.status.code().unwrap_or(-1),
                        stderr.trim()
                    )));
                }

                let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if id.is_empty() {
                    return Err(ExecError::SpawnFailed(
                        "docker run returned empty container ID".to_string(),
                    ));
                }
                Ok(id)
            })
            .await
            .map(|s| s.as_str())
    }

    /// Attach a persistent shell session to the container via `docker exec -i`.
    async fn attach_session(container_id: &str) -> Result<ShellSession, ExecError> {
        let mut cmd = tokio::process::Command::new("docker");
        cmd.args(["exec", "-i", container_id, "bash"]);
        ShellSession::spawn_from_command(&mut cmd).await
    }

    /// Ensure the session is alive, attaching a new one if needed.
    async fn ensure_session(
        session_slot: &mut Option<ShellSession>,
        container_id: &str,
    ) -> Result<(), ExecError> {
        let needs_attach = match session_slot.as_mut() {
            Some(s) => !s.is_alive(),
            None => true,
        };

        if needs_attach {
            let new_session = Self::attach_session(container_id).await?;
            *session_slot = Some(new_session);
        }

        Ok(())
    }

    /// One-shot execution via `docker exec`. Fallback when session fails.
    async fn execute_oneshot(
        &self,
        container_id: &str,
        command: &str,
        timeout: Duration,
    ) -> Result<ExecOutput, ExecError> {
        let exec_future = async {
            let output = tokio::process::Command::new("docker")
                .args(["exec", container_id, "bash", "-c", command])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .await
                .map_err(|e| ExecError::SpawnFailed(format!("docker exec failed: {e}")))?;

            let (stdout, trunc_out) = truncate_output(&output.stdout, self.max_output_bytes);
            let (stderr, trunc_err) = truncate_output(&output.stderr, self.max_output_bytes);

            Ok(ExecOutput {
                stdout,
                stderr,
                exit_code: output.status.code().unwrap_or(-1),
                truncated: trunc_out || trunc_err,
            })
        };

        match tokio::time::timeout(timeout, exec_future).await {
            Ok(result) => result,
            Err(_) => {
                let _ = tokio::process::Command::new("docker")
                    .args(["stop", "-t", "0", container_id])
                    .output()
                    .await;
                Err(ExecError::Timeout(timeout))
            }
        }
    }
}

#[async_trait]
impl Sandbox for DockerSandbox {
    async fn execute(&self, command: &str, timeout: Duration) -> Result<ExecOutput, ExecError> {
        let container_id = self.get_or_create_container().await?;
        let mut session_guard = self.session.lock().await;

        // Try persistent session first
        match Self::ensure_session(&mut session_guard, container_id).await {
            Ok(()) => {
                let session = session_guard.as_mut().unwrap();
                match session.execute(command, timeout).await {
                    Ok(output) => {
                        let raw = output.stdout.as_bytes();
                        let (stdout, truncated) = truncate_output(raw, self.max_output_bytes);
                        return Ok(ExecOutput {
                            stdout,
                            stderr: String::new(),
                            exit_code: output.exit_code,
                            truncated,
                        });
                    }
                    Err(ExecError::Timeout(d)) => {
                        if let Some(s) = session_guard.as_mut() {
                            s.kill().await;
                        }
                        *session_guard = None;
                        return Err(ExecError::Timeout(d));
                    }
                    Err(_) => {
                        if let Some(s) = session_guard.as_mut() {
                            s.kill().await;
                        }
                        *session_guard = None;
                    }
                }
            }
            Err(_) => {
                *session_guard = None;
            }
        }

        // Fallback: exec-per-command
        drop(session_guard);
        self.execute_oneshot(container_id, command, timeout).await
    }

    async fn restart(&self) -> Result<(), ExecError> {
        let mut session_guard = self.session.lock().await;
        if let Some(s) = session_guard.as_mut() {
            s.kill().await;
        }
        *session_guard = None;
        Ok(())
    }
}

impl Drop for DockerSandbox {
    fn drop(&mut self) {
        if let Some(id) = self.container_id.get() {
            let _ = std::process::Command::new("docker")
                .args(["stop", "-t", "1", id])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_availability_check() {
        // Just verify it doesn't panic - result depends on environment
        let _ = DockerSandbox::is_available();
    }

    #[tokio::test]
    async fn test_docker_echo() {
        if !DockerSandbox::is_available() {
            eprintln!("Skipping: Docker not available");
            return;
        }

        let sb = DockerSandbox::new(std::env::temp_dir(), "ubuntu:24.04", 100_000);
        let out = sb
            .execute("echo hello from docker", Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "hello from docker");
        assert_eq!(out.exit_code, 0);
        assert!(!out.truncated);
    }

    #[tokio::test]
    async fn test_docker_env_isolation() {
        if !DockerSandbox::is_available() {
            eprintln!("Skipping: Docker not available");
            return;
        }

        // Host env vars should not leak into the container
        unsafe { std::env::set_var("NAVI_DOCKER_SECRET", "leaked") };
        let sb = DockerSandbox::new(std::env::temp_dir(), "ubuntu:24.04", 100_000);
        let out = sb
            .execute("echo $NAVI_DOCKER_SECRET", Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "");
        unsafe { std::env::remove_var("NAVI_DOCKER_SECRET") };
    }

    #[tokio::test]
    async fn test_docker_state_persists() {
        if !DockerSandbox::is_available() {
            eprintln!("Skipping: Docker not available");
            return;
        }

        let sb = DockerSandbox::new(std::env::temp_dir(), "ubuntu:24.04", 100_000);

        // With persistent session, env vars now persist across calls
        sb.execute("export NAVI_STATE=hello", Duration::from_secs(30))
            .await
            .unwrap();

        let out = sb
            .execute("echo $NAVI_STATE", Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_docker_file_persistence() {
        if !DockerSandbox::is_available() {
            eprintln!("Skipping: Docker not available");
            return;
        }

        let sb = DockerSandbox::new(std::env::temp_dir(), "ubuntu:24.04", 100_000);

        sb.execute("touch /tmp/navi_test_marker", Duration::from_secs(30))
            .await
            .unwrap();
        let out = sb
            .execute(
                "test -f /tmp/navi_test_marker && echo exists",
                Duration::from_secs(30),
            )
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "exists");
    }
}
