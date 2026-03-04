pub mod model_discovery;
pub mod provider;
pub mod providers;
pub mod task;
pub mod tasks;
pub mod types;

pub use provider::{CompletionProvider, CompletionRequest, ProviderError};
pub use providers::{LmStudioProvider, OpenRouterProvider};
pub use types::{
    Context, ContextItem, ContextSegment, Effort, ResponseFormat, Source, StreamChunk, ToolCall,
    ToolDefinition, ToolResult, UsageStats,
};

use std::sync::Arc;

use crate::core::config::ResolvedConfig;

/// Build a provider from a resolved config's provider name and credentials.
pub fn build_provider(config: &ResolvedConfig) -> Arc<dyn CompletionProvider> {
    match config.provider.as_str() {
        "lmstudio" => Arc::new(LmStudioProvider::new(config.lmstudio_base_url.clone())),
        _ => {
            // Default to openrouter
            let api_key = config
                .openrouter_api_key
                .clone()
                .expect("OpenRouter API key must be set (config file, OPENROUTER_API_KEY env var, or --provider lmstudio)");
            Arc::new(OpenRouterProvider::new(
                api_key,
                Some(config.openrouter_base_url.clone()),
            ))
        }
    }
}
