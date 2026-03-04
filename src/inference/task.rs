//! Composable LLM task abstraction.
//!
//! The `Task` trait models an async function `Input -> Result<Output>`.
//! `Prompt` is the LLM boundary: `String -> String`. Typed I/O comes from
//! combinators (`.map()`, `.then()`), not from the prompt itself.
//!
//! # Examples
//!
//! ```ignore
//! // Simple text generation
//! let title = Prompt::new(provider.clone(), "model-name")
//!     .system("Generate a 3-6 word title. Return ONLY the title.")
//!     .max_tokens(32)
//!     .run("Summarize this conversation".into()).await?;
//!
//! // JSON mode with parsed output
//! let info = Prompt::new(provider, "model-name")
//!     .system(r#"Return {"title": "...", "tags": [...]}"#)
//!     .json()
//!     .map(|text| serde_json::from_str(&text).map_err(|e| TaskError::Parse(e.to_string())))
//!     .run(summary).await?;
//! ```

use std::sync::Arc;

use async_trait::async_trait;

use super::provider::{CompletionProvider, CompletionRequest, ProviderError};
use super::types::{Context, Effort, ResponseFormat, StreamChunk};

// ============================================================================
// Error
// ============================================================================

/// Errors from task execution.
///
/// A task can fail in two ways: the LLM provider itself fails (`Provider`),
/// or the provider succeeded but post-processing the output fails (`Parse`).
/// This distinction lets callers decide whether to retry (provider errors are
/// often transient) or fix the prompt (parse errors mean the model returned
/// something unexpected).
#[derive(Debug)]
pub enum TaskError {
    /// The underlying provider failed (network, API, channel closed).
    Provider(ProviderError),
    /// Output parsing/transformation failed (e.g. invalid JSON from `.map()`).
    #[allow(dead_code)] // Used by .map() combinator — real callers land later.
    Parse(String),
}

impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskError::Provider(e) => write!(f, "provider error: {e}"),
            TaskError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for TaskError {}

impl From<ProviderError> for TaskError {
    fn from(e: ProviderError) -> Self {
        TaskError::Provider(e)
    }
}

// ============================================================================
// Task trait
// ============================================================================

/// An async, composable unit of work: `Input -> Result<Output>`.
///
/// This is the core abstraction. Every step in an LLM pipeline implements `Task`:
/// - `Prompt` is `Task<String, String>` — the LLM boundary.
/// - `Chain<A, B>` is `Task<A::Input, B::Output>` — sequential composition.
/// - `Map<T, F>` is `Task<T::Input, F::Output>` — output transformation.
///
/// Implementations take `&self` so they're reusable templates. The same `Prompt`
/// can be called with different inputs without rebuilding the pipeline.
#[async_trait]
pub trait Task: Send + Sync {
    type Input: Send + Sync;
    type Output: Send + Sync;

    /// Execute the task with the given input.
    async fn run(&self, input: Self::Input) -> Result<Self::Output, TaskError>;

    /// Sequential composition: run `self`, then feed its output as input to `next`.
    ///
    /// Type-safe: `next.Input` must match `self.Output`. The compiler enforces
    /// that pipelines are well-typed at construction time, not at runtime.
    fn then<T>(self, next: T) -> Chain<Self, T>
    where
        Self: Sized,
        T: Task<Input = Self::Output>,
    {
        Chain {
            first: self,
            second: next,
        }
    }

    /// Transform the output with a fallible function.
    ///
    /// This is how you go from `String` (what LLMs produce) to typed data
    /// (what your application needs). The function can fail with `TaskError::Parse`
    /// if the model's output doesn't match the expected format.
    #[allow(dead_code)] // Combinator infrastructure — consumed by tests, real callers land later.
    fn map<F, O>(self, f: F) -> Map<Self, F>
    where
        Self: Sized,
        F: Fn(Self::Output) -> Result<O, TaskError> + Send + Sync,
        O: Send + Sync,
    {
        Map { inner: self, f }
    }
}

// ============================================================================
// Prompt — the LLM call primitive
// ============================================================================

/// A reusable LLM prompt template. Implements `Task<String, String>`.
///
/// This is the fundamental LLM primitive — it takes a user message (String),
/// sends it to a model with the configured system prompt, streams the response,
/// and collects the content chunks into a single output String.
///
/// **Why String→String?** LLMs speak text. Typed I/O belongs in the layers
/// around the prompt (`.map()` for parsing, future `adapt()` for input formatting),
/// not in the prompt itself. This keeps the LLM boundary clean and composable.
///
/// Configured via builder methods. Defaults are tuned for lightweight background
/// tasks: `Effort::None` (no reasoning tokens), 256 max output tokens, text mode.
///
/// # Streaming internals
///
/// `run()` creates an mpsc channel, spawns a collector task to receive chunks,
/// then drives `stream_completion` on the current task. This avoids requiring
/// `'static` borrows on `CompletionRequest` (which holds `&Context` and `&str`
/// references). The collector only sees owned `StreamChunk` values through the
/// channel, so lifetimes don't leak across the spawn boundary.
pub struct Prompt {
    provider: Arc<dyn CompletionProvider>,
    model: String,
    system_prompt: String,
    effort: Effort,
    max_output_tokens: Option<u32>,
    response_format: Option<ResponseFormat>,
}

impl Prompt {
    /// Create a new prompt bound to a provider and model.
    ///
    /// Defaults: no system prompt, `Effort::None`, 256 max tokens, text mode.
    pub fn new(provider: Arc<dyn CompletionProvider>, model: &str) -> Self {
        Self {
            provider,
            model: model.to_string(),
            system_prompt: String::new(),
            effort: Effort::None,
            max_output_tokens: Some(256),
            response_format: None,
        }
    }

    /// Set the system prompt that frames the model's behavior.
    pub fn system(mut self, prompt: &str) -> Self {
        self.system_prompt = prompt.to_string();
        self
    }

    /// Set the reasoning effort level. Higher effort = more reasoning tokens = better
    /// quality but higher cost. Default is `None` (no reasoning).
    #[allow(dead_code)] // Builder method — real callers land later.
    pub fn effort(mut self, effort: Effort) -> Self {
        self.effort = effort;
        self
    }

    /// Set the maximum number of output tokens. Default is 256.
    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_output_tokens = Some(n);
        self
    }

    /// Enable JSON mode — tells the provider to use constrained generation
    /// so the model output is guaranteed to be valid JSON.
    ///
    /// This sets `response_format` at the API level. You'll typically pair this
    /// with `.map()` to parse the JSON string into a typed struct:
    ///
    /// ```ignore
    /// prompt.json().map(|text| serde_json::from_str(&text).map_err(|e| TaskError::Parse(e.to_string())))
    /// ```
    #[allow(dead_code)] // Builder method — real callers land later.
    pub fn json(mut self) -> Self {
        self.response_format = Some(ResponseFormat::Json);
        self
    }
}

#[async_trait]
impl Task for Prompt {
    type Input = String;
    type Output = String;

    async fn run(&self, input: Self::Input) -> Result<Self::Output, TaskError> {
        let mut context = Context::with_system_prompt(self.system_prompt.clone());
        context.add_user_message(input);

        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

        // Spawn the collector first — it needs to be receiving before we drive
        // the provider, since the provider sends chunks via the channel.
        let collector = tokio::spawn(async move {
            let mut output = String::new();
            while let Some(chunk) = chunk_rx.recv().await {
                if let StreamChunk::Content { text, .. } = chunk {
                    output.push_str(&text);
                }
                // Thinking, ToolCall, Completed — ignored for simple prompts
            }
            output
        });

        // Drive the provider on the current task (avoids needing 'static borrows).
        let request = CompletionRequest {
            context: &context,
            model: &self.model,
            effort: self.effort,
            tools: &[],
            max_output_tokens: self.max_output_tokens,
            response_format: self.response_format,
        };

        self.provider.stream_completion(request, chunk_tx).await?;

        // The provider is done sending — collector will drain and finish.
        let output = collector.await.expect("collector task panicked");
        Ok(output)
    }
}

// ============================================================================
// Combinators
// ============================================================================

/// Sequential composition: run A, then feed A's output as input to B.
///
/// Created by `task_a.then(task_b)`. The type system enforces that
/// `A::Output == B::Input` — mismatched pipelines are compile errors.
///
/// If A fails, B never runs and the error propagates immediately.
pub struct Chain<A, B> {
    first: A,
    second: B,
}

#[async_trait]
impl<A, B> Task for Chain<A, B>
where
    A: Task,
    B: Task<Input = A::Output>,
{
    type Input = A::Input;
    type Output = B::Output;

    async fn run(&self, input: Self::Input) -> Result<Self::Output, TaskError> {
        let intermediate = self.first.run(input).await?;
        self.second.run(intermediate).await
    }
}

/// Output transformation via a fallible function.
///
/// Created by `task.map(f)`. This is the bridge between the LLM's text world
/// and your application's typed world. The function receives the task's output
/// and can transform or parse it, returning `Err(TaskError::Parse(...))` if
/// the output doesn't match expectations.
///
/// The function must be `Fn` (not `FnOnce`) because tasks are reusable via `&self`.
#[allow(dead_code)] // Combinator infrastructure — consumed by tests, real callers land later.
pub struct Map<T, F> {
    inner: T,
    f: F,
}

#[async_trait]
impl<T, F, O> Task for Map<T, F>
where
    T: Task,
    F: Fn(T::Output) -> Result<O, TaskError> + Send + Sync,
    O: Send + Sync,
{
    type Input = T::Input;
    type Output = O;

    async fn run(&self, input: Self::Input) -> Result<Self::Output, TaskError> {
        let output = self.inner.run(input).await?;
        (self.f)(output)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::provider::ProviderError;
    use crate::inference::types::StreamChunk;
    use tokio::sync::mpsc::Sender;

    // ====================================================================
    // Test Providers
    //
    // These are fake CompletionProvider implementations used to test the
    // task abstraction in isolation — no network, no HTTP, no SSE parsing.
    //
    // The integration tests in tests/provider_integration_tests.rs cover
    // the real HTTP/SSE path via wiremock. Here we only care about:
    // "given these StreamChunks, does Prompt/Chain/Map behave correctly?"
    //
    // Three providers cover the three interesting scenarios:
    // - FixedProvider:     happy path (send predetermined chunks)
    // - FailingProvider:   error path (provider returns Err immediately)
    // - CapturingProvider: inspection (record what request fields were sent)
    // ====================================================================

    /// A test provider that replays a fixed sequence of StreamChunks.
    ///
    /// Named "Fixed" because the output is predetermined at construction time —
    /// every call to `stream_completion` sends the exact same chunks regardless
    /// of what the request contains. Same naming convention as `FixedClock` or
    /// `FixedRandom` in other test harnesses: you fix the answer to isolate
    /// the logic under test.
    ///
    /// Chunks are cloned on each call, so the provider is reusable (important
    /// for `test_prompt_reusable`).
    struct FixedProvider {
        chunks: Vec<StreamChunk>,
    }

    impl FixedProvider {
        /// Create a provider that sends content-only chunks followed by Completed.
        ///
        /// Convenience for the common case: you just want the prompt to produce
        /// a specific text output. Automatically appends `StreamChunk::Completed(None)`.
        fn with_content(texts: Vec<&str>) -> Self {
            let chunks = texts
                .into_iter()
                .map(|t| StreamChunk::Content {
                    text: t.to_string(),
                    item_id: None,
                })
                .chain(std::iter::once(StreamChunk::Completed(None)))
                .collect();
            Self { chunks }
        }

        /// Create a provider that sends an exact sequence of chunks.
        ///
        /// Use this when you need fine-grained control — e.g. interleaving
        /// Thinking and Content chunks to test that Prompt only collects Content.
        fn with_chunks(chunks: Vec<StreamChunk>) -> Self {
            Self { chunks }
        }
    }

    #[async_trait]
    impl CompletionProvider for FixedProvider {
        async fn stream_completion(
            &self,
            _request: CompletionRequest<'_>,
            sender: Sender<StreamChunk>,
        ) -> Result<(), ProviderError> {
            // Clone each chunk because StreamChunk isn't Copy and we need
            // the provider to be reusable across multiple run() calls.
            for chunk in &self.chunks {
                let chunk = match chunk {
                    StreamChunk::Content { text, item_id } => StreamChunk::Content {
                        text: text.clone(),
                        item_id: item_id.clone(),
                    },
                    StreamChunk::Thinking { text, item_id } => StreamChunk::Thinking {
                        text: text.clone(),
                        item_id: item_id.clone(),
                    },
                    StreamChunk::Completed(stats) => StreamChunk::Completed(stats.clone()),
                    StreamChunk::ToolCall(tc) => StreamChunk::ToolCall(tc.clone()),
                };
                sender.send(chunk).await.map_err(|_| ProviderError::ChannelClosed)?;
            }
            Ok(())
        }
    }

    /// A test provider that always returns an error without sending any chunks.
    ///
    /// Used to verify that provider-level failures propagate through the task
    /// pipeline as `TaskError::Provider(...)`.
    struct FailingProvider;

    #[async_trait]
    impl CompletionProvider for FailingProvider {
        async fn stream_completion(
            &self,
            _request: CompletionRequest<'_>,
            _sender: Sender<StreamChunk>,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Api {
                status: 500,
                message: "test error".into(),
            })
        }
    }

    /// A test provider that records request parameters for later inspection.
    ///
    /// Sends a minimal valid response ("ok" + Completed) so the prompt succeeds,
    /// but the real purpose is capturing what `CompletionRequest` fields were
    /// passed in. Currently captures `response_format` to verify that `.json()`
    /// correctly threads through to the provider layer.
    struct CapturingProvider {
        /// Each call to stream_completion pushes the request's response_format here.
        captured: std::sync::Mutex<Vec<Option<ResponseFormat>>>,
    }

    impl CapturingProvider {
        fn new() -> Self {
            Self {
                captured: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl CompletionProvider for CapturingProvider {
        async fn stream_completion(
            &self,
            request: CompletionRequest<'_>,
            sender: Sender<StreamChunk>,
        ) -> Result<(), ProviderError> {
            self.captured
                .lock()
                .unwrap()
                .push(request.response_format);
            sender
                .send(StreamChunk::Content {
                    text: "ok".into(),
                    item_id: None,
                })
                .await
                .map_err(|_| ProviderError::ChannelClosed)?;
            sender
                .send(StreamChunk::Completed(None))
                .await
                .map_err(|_| ProviderError::ChannelClosed)?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_prompt_collects_content() {
        let provider = Arc::new(FixedProvider::with_content(vec!["Hello", " world"]));
        let result = Prompt::new(provider, "test-model")
            .system("Be helpful")
            .run("hi".into())
            .await
            .unwrap();
        assert_eq!(result, "Hello world");
    }

    #[tokio::test]
    async fn test_prompt_ignores_thinking() {
        let provider = Arc::new(FixedProvider::with_chunks(vec![
            StreamChunk::Thinking {
                text: "hmm...".into(),
                item_id: None,
            },
            StreamChunk::Content {
                text: "answer".into(),
                item_id: None,
            },
            StreamChunk::Completed(None),
        ]));
        let result = Prompt::new(provider, "test-model")
            .run("hi".into())
            .await
            .unwrap();
        assert_eq!(result, "answer");
    }

    #[tokio::test]
    async fn test_prompt_reusable() {
        let provider = Arc::new(FixedProvider::with_content(vec!["response"]));
        let prompt = Prompt::new(provider, "test-model").system("system");

        let r1 = prompt.run("input1".into()).await.unwrap();
        let r2 = prompt.run("input2".into()).await.unwrap();
        assert_eq!(r1, "response");
        assert_eq!(r2, "response");
    }

    #[tokio::test]
    async fn test_prompt_json_mode() {
        let provider = Arc::new(CapturingProvider::new());
        let prompt = Prompt::new(provider.clone(), "test-model")
            .system("return json")
            .json();

        prompt.run("input".into()).await.unwrap();

        let captured = provider.captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0], Some(ResponseFormat::Json));
    }

    #[tokio::test]
    async fn test_prompt_text_mode_default() {
        let provider = Arc::new(CapturingProvider::new());
        let prompt = Prompt::new(provider.clone(), "test-model");

        prompt.run("input".into()).await.unwrap();

        let captured = provider.captured.lock().unwrap();
        assert_eq!(captured[0], None);
    }

    #[tokio::test]
    async fn test_chain_sequential() {
        let p1 = Arc::new(FixedProvider::with_content(vec!["intermediate"]));
        let p2 = Arc::new(FixedProvider::with_content(vec!["final"]));

        let a = Prompt::new(p1, "m1").system("step 1");
        let b = Prompt::new(p2, "m2").system("step 2");
        let chained = a.then(b);

        let result = chained.run("start".into()).await.unwrap();
        assert_eq!(result, "final");
    }

    #[tokio::test]
    async fn test_map_transforms() {
        let provider = Arc::new(FixedProvider::with_content(vec!["42"]));
        let task = Prompt::new(provider, "test-model").map(|text| {
            text.trim()
                .parse::<i32>()
                .map_err(|e| TaskError::Parse(e.to_string()))
        });

        let result = task.run("what is 6*7?".into()).await.unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_map_parse_error() {
        let provider = Arc::new(FixedProvider::with_content(vec!["not a number"]));
        let task = Prompt::new(provider, "test-model").map(|text| {
            text.trim()
                .parse::<i32>()
                .map_err(|e| TaskError::Parse(e.to_string()))
        });

        let result = task.run("input".into()).await;
        assert!(matches!(result, Err(TaskError::Parse(_))));
    }

    #[tokio::test]
    async fn test_error_propagates() {
        let provider: Arc<dyn CompletionProvider> = Arc::new(FailingProvider);
        let result = Prompt::new(provider, "test-model")
            .run("hi".into())
            .await;
        assert!(matches!(result, Err(TaskError::Provider(_))));
    }
}
