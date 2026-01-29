//! Test utilities shared across the crate.
//!
//! This module is only compiled during tests (`#[cfg(test)]`).

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::inference::{
    CompletionProvider, CompletionRequest, ProviderError, StreamChunk,
};

/// A no-op provider for tests that don't need real API calls.
pub struct NoopProvider;

#[async_trait]
impl CompletionProvider for NoopProvider {
    fn name(&self) -> &str {
        "noop"
    }

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
