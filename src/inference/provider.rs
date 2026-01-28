use std::fmt;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use super::types::{Context, Effort, StreamChunk};

/// Errors that can occur during provider operations.
/// Variants carry enough info to determine retryability (future use).
#[derive(Debug)]
pub enum ProviderError {
    /// Provider misconfigured (missing API key, bad URL). Not retryable.
    Config(String),
    /// Network-level failure (timeout, DNS, connection refused). Retryable.
    Network(String),
    /// API returned an error response. Retryable if status >= 500 or 429.
    Api { status: u16, message: String },
    /// Failed to parse the provider's response. Not retryable.
    Parse(String),
    /// The mpsc channel was closed (TUI dropped the receiver). Not retryable.
    ChannelClosed,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Config(msg) => write!(f, "config error: {msg}"),
            ProviderError::Network(msg) => write!(f, "network error: {msg}"),
            ProviderError::Api { status, message } => {
                write!(f, "API error (HTTP {status}): {message}")
            }
            ProviderError::Parse(msg) => write!(f, "parse error: {msg}"),
            ProviderError::ChannelClosed => write!(f, "channel closed"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Everything a provider needs to fulfill a completion request.
pub struct CompletionRequest<'a> {
    pub context: &'a Context,
    pub model: &'a str,
    pub effort: Effort,
}

#[async_trait]
pub trait CompletionProvider: Send + Sync {
    /// Returns the name of the provider.
    fn name(&self) -> &str;

    /// Streams a completion based on the given request, sending chunks to the provided channel.
    async fn stream_completion(
        &self,
        request: CompletionRequest<'_>,
        sender: Sender<StreamChunk>,
    ) -> Result<(), ProviderError>;
}
