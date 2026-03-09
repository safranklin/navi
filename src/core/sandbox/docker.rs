//! # Docker Sandbox
//!
//! Executes commands inside a persistent Docker container. The container is
//! created lazily on first use and stopped on drop. CWD is volume-mounted
//! as /workspace.
//!
//! Shells out to the `docker` CLI rather than using bollard - simpler, no
//! extra dependencies, works everywhere Docker is installed.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::OnceCell;

use super::{ExecError, ExecOutput, Sandbox};

pub struct DockerSandbox {
    container_id: OnceCell<String>,
    image: String,
    working_dir: PathBuf,
    max_output_bytes: usize,
}

impl DockerSandbox {
    pub fn new(working_dir: PathBuf, image: &str, max_output_bytes: usize) -> Self {
        Self {
            container_id: OnceCell::new(),
            image: image.to_string(),
            working_dir,
            max_output_bytes,
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
}

#[async_trait]
impl Sandbox for DockerSandbox {
    async fn execute(&self, command: &str, timeout: Duration) -> Result<ExecOutput, ExecError> {
        let container_id = self.get_or_create_container().await?;

        let exec_future = async {
            let output = tokio::process::Command::new("docker")
                .args(["exec", container_id, "sh", "-c", command])
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
                // Kill the container on timeout
                let _ = tokio::process::Command::new("docker")
                    .args(["stop", "-t", "0", container_id])
                    .output()
                    .await;
                Err(ExecError::Timeout(timeout))
            }
        }
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

/// Truncate raw bytes to max_bytes, returning the UTF-8 string and whether truncation occurred.
fn truncate_output(raw: &[u8], max_bytes: usize) -> (String, bool) {
    if raw.len() <= max_bytes {
        (String::from_utf8_lossy(raw).to_string(), false)
    } else {
        let mut s = String::from_utf8_lossy(&raw[..max_bytes]).to_string();
        s.push_str("\n[output truncated]");
        (s, true)
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
    async fn test_docker_persistent_container() {
        if !DockerSandbox::is_available() {
            eprintln!("Skipping: Docker not available");
            return;
        }

        let sb = DockerSandbox::new(std::env::temp_dir(), "ubuntu:24.04", 100_000);

        // First command creates the container
        sb.execute("echo first", Duration::from_secs(30))
            .await
            .unwrap();

        // State should persist across commands (same container)
        sb.execute("export NAVI_STATE=hello", Duration::from_secs(30))
            .await
            .unwrap();

        // Note: env vars don't persist across docker exec calls (each is a new shell),
        // but files do
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
