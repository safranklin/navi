pub mod model_discovery;
pub mod provider;
pub mod providers;
#[allow(dead_code)] // Infrastructure — consumers land in upcoming commits.
pub mod task;
pub mod types;

pub use provider::{CompletionProvider, CompletionRequest, ProviderError};
pub use providers::{LmStudioProvider, OpenRouterProvider};
#[allow(unused_imports)]
pub use task::{Prompt, Task, TaskError};
pub use types::{
    Context, ContextItem, ContextSegment, Effort, ResponseFormat, Source, StreamChunk, ToolCall,
    ToolDefinition, ToolResult, UsageStats,
};
