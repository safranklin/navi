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
