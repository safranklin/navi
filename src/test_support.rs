//! Test utilities shared across the crate.
//!
//! This module is only compiled during tests (`#[cfg(test)]`).

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use crate::inference::{CompletionProvider, CompletionRequest, ProviderError, StreamChunk};

/// A no-op provider for tests that don't need real API calls.
pub struct NoopProvider;

#[async_trait]
impl CompletionProvider for NoopProvider {
    async fn stream_completion(
        &self,
        _request: CompletionRequest<'_>,
        _sender: Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        Ok(())
    }
}

/// Creates a test App with a NoopProvider.
pub fn test_app() -> crate::core::state::App {
    crate::core::state::App::new(Arc::new(NoopProvider), "test-model".to_string())
}

/// Creates a test App that includes a Prompt-permission tool for testing approval flows.
pub fn test_app_with_prompt_tool() -> crate::core::state::App {
    let mut app = test_app();
    let mut registry = crate::core::tools::ToolRegistry::new();
    registry.register(crate::core::tools::math::MathOperation);
    registry.register(crate::core::tools::io::ReadFileTool);
    registry.register(StubPromptTool);
    app.registry = Arc::new(registry);
    app
}

/// A minimal tool that requires Prompt permission. Used in tests only.
pub struct StubPromptTool;

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct StubPromptArgs {
    pub command: String,
}

#[async_trait]
impl crate::core::tools::Tool for StubPromptTool {
    const NAME: &'static str = "stub_prompt";
    const DESCRIPTION: &'static str = "Stub tool for testing prompt permission";
    const PERMISSION: crate::core::tools::ToolPermission =
        crate::core::tools::ToolPermission::Prompt;
    type Args = StubPromptArgs;
    type Output = String;

    async fn call(&self, args: StubPromptArgs) -> Result<String, crate::core::tools::ToolError> {
        Ok(format!("executed: {}", args.command))
    }
}
