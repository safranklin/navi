//! # IO Tools
//!
//! IO-related tool implementations. (Read, Write, etc.)
//! Each tool is a unit struct implementing the `Tool` trait.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Tool, ToolError};

// ── Read File ─────────────────────────────────────────────────────────────────────

pub struct ReadFileTool;

#[derive(Deserialize, JsonSchema)]
pub struct ReadFileArgs {
    /// Absolute or relative path to the file to read.
    pub file_path: String,
}

#[derive(Debug, Serialize)]
pub struct ReadFileResult {
    pub content: String,
}

#[async_trait]
impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    const DESCRIPTION: &'static str = "\
        Reads the full content of a text file at the given path and returns it as a string. \
        Use this tool whenever you need to inspect a file's contents - for example, to answer \
        questions about code, configuration, logs, or data files. The file_path can be absolute \
        or relative to the working directory. Returns an error if the file does not exist, is not \
        readable, or is not valid UTF-8 text. This tool does not support binary files.";
    type Args = ReadFileArgs;
    type Output = ReadFileResult;

    async fn call(&self, args: ReadFileArgs) -> Result<ReadFileResult, ToolError> {
        let content = std::fs::read_to_string(&args.file_path)
            .map_err(|e| ToolError(format!("Failed to read file '{}': {}", args.file_path, e)))?;

        Ok(ReadFileResult { content })
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: create a temp file with the given content, return its path.
    fn temp_file(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("navi_test_{name}"));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[tokio::test]
    async fn read_file_returns_content() {
        let path = temp_file("read_ok", "hello world\n");
        let result = ReadFileTool
            .call(ReadFileArgs {
                file_path: path.to_string_lossy().into(),
            })
            .await
            .unwrap();
        assert_eq!(result.content, "hello world\n");
        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn read_file_missing_returns_error() {
        let result = ReadFileTool
            .call(ReadFileArgs {
                file_path: "/tmp/navi_test_does_not_exist_xyz".into(),
            })
            .await;
        let err = result.unwrap_err();
        assert!(err.0.contains("Failed to read file"), "got: {err}");
    }

    #[tokio::test]
    async fn read_file_via_registry() {
        use crate::core::tools::default_registry;
        use crate::inference::types::ToolCall;

        let path = temp_file("read_reg", "registry test");
        let registry = default_registry();
        let tc = ToolCall {
            id: "fc_io".into(),
            call_id: "call_io".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({ "file_path": path.to_string_lossy() }).to_string(),
        };
        let result = registry.execute(&tc).await;
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["content"], "registry test");
        std::fs::remove_file(path).ok();
    }

    // TODO(human): Add a test for reading a binary/non-UTF-8 file.
    // The tool claims it doesn't support binary files - verify it returns
    // a clear error with an appropriate message.
}
