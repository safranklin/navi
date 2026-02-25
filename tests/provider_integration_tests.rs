use navi::inference::{
    CompletionProvider, CompletionRequest, Context, ContextSegment, Effort, LmStudioProvider,
    OpenRouterProvider, ProviderError, Source, StreamChunk,
};
use tokio::sync::mpsc;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates a simple test context with a user message
fn create_test_context() -> Context {
    let mut context = Context::new();
    context.add(ContextSegment {
        source: Source::User,
        content: "Hello".to_string(),
    });
    context
}

struct CollectedStream {
    content: Vec<String>,
    thinking: Vec<String>,
}

/// Collects all chunks from a stream
async fn collect_chunks(mut receiver: mpsc::Receiver<StreamChunk>) -> CollectedStream {
    let mut result = CollectedStream {
        content: Vec::new(),
        thinking: Vec::new(),
    };

    while let Some(chunk) = receiver.recv().await {
        match chunk {
            StreamChunk::Content { text, .. } => result.content.push(text),
            StreamChunk::Thinking { text, .. } => result.thinking.push(text),
            StreamChunk::Completed | StreamChunk::ToolCall(_) => {}
        }
    }

    result
}

// ============================================================================
// OpenRouter Provider Tests
// ============================================================================

#[tokio::test]
async fn test_openrouter_successful_streaming_content_only() {
    let mock_server = MockServer::start().await;

    // Mock SSE response with only content chunks
    let sse_response = "\
event: response.created
data: {\"type\":\"response.created\"}

event: response.output_text.delta
data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}

event: response.output_text.delta
data: {\"type\":\"response.output_text.delta\",\"delta\":\" world\"}

event: response.completed
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_or_001\"}}
";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
        .mount(&mock_server)
        .await;

    let provider = OpenRouterProvider::new("test-key".to_string(), Some(mock_server.uri()));

    let context = create_test_context();
    let request = CompletionRequest {
        model: "test-model",
        context: &context,
        effort: Effort::None,
        tools: &[],
    };

    let (tx, rx) = mpsc::channel(100);
    let result = provider.stream_completion(request, tx).await;

    assert!(result.is_ok());

    let collected = collect_chunks(rx).await;
    assert_eq!(collected.content, vec!["Hello", " world"]);
    assert!(collected.thinking.is_empty());
}

#[tokio::test]
async fn test_openrouter_streaming_with_thinking() {
    let mock_server = MockServer::start().await;

    // Mock SSE response with thinking + content
    let sse_response = "\
event: response.created
data: {\"type\":\"response.created\"}

event: response.reasoning_summary_text.delta
data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"Thinking...\"}

event: response.output_text.delta
data: {\"type\":\"response.output_text.delta\",\"delta\":\"Answer\"}

event: response.completed
data: {\"type\":\"response.completed\"}
";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
        .mount(&mock_server)
        .await;

    let provider = OpenRouterProvider::new("test-key".to_string(), Some(mock_server.uri()));

    let context = create_test_context();
    let request = CompletionRequest {
        model: "test-model",
        context: &context,
        effort: Effort::High,
        tools: &[],
    };

    let (tx, rx) = mpsc::channel(100);
    let result = provider.stream_completion(request, tx).await;

    assert!(result.is_ok());

    let collected = collect_chunks(rx).await;
    assert_eq!(collected.content, vec!["Answer"]);
    assert_eq!(collected.thinking, vec!["Thinking..."]);
}

#[tokio::test]
async fn test_openrouter_api_error_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&mock_server)
        .await;

    let provider = OpenRouterProvider::new("invalid-key".to_string(), Some(mock_server.uri()));

    let context = create_test_context();
    let request = CompletionRequest {
        model: "test-model",
        context: &context,
        effort: Effort::None,
        tools: &[],
    };

    let (tx, _rx) = mpsc::channel(100);
    let result = provider.stream_completion(request, tx).await;

    assert!(matches!(
        result,
        Err(ProviderError::Api { status: 401, .. })
    ));
}

#[tokio::test]
async fn test_openrouter_channel_closed_error() {
    let mock_server = MockServer::start().await;

    let sse_response = "\
event: response.output_text.delta
data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}

event: response.output_text.delta
data: {\"type\":\"response.output_text.delta\",\"delta\":\" world\"}
";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
        .mount(&mock_server)
        .await;

    let provider = OpenRouterProvider::new("test-key".to_string(), Some(mock_server.uri()));

    let context = create_test_context();
    let request = CompletionRequest {
        model: "test-model",
        context: &context,
        effort: Effort::None,
        tools: &[],
    };

    let (tx, rx) = mpsc::channel(1);
    // Drop receiver immediately to simulate channel closed
    drop(rx);

    let result = provider.stream_completion(request, tx).await;

    assert!(matches!(result, Err(ProviderError::ChannelClosed)));
}

// ============================================================================
// LM Studio Provider Tests
// ============================================================================

#[tokio::test]
async fn test_lmstudio_successful_streaming_content_only() {
    let mock_server = MockServer::start().await;

    // Mock SSE response for LM Studio format
    let sse_response = "\
event: response.created
data: {\"id\":\"test\"}

event: response.output_text.delta
data: {\"delta\":\"Hello\"}

event: response.output_text.delta
data: {\"delta\":\" LM Studio\"}

event: response.completed
data: {\"response\":{\"id\":\"resp_lms_001\"}}
";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
        .mount(&mock_server)
        .await;

    let provider = LmStudioProvider::new(Some(mock_server.uri()));

    let context = create_test_context();
    let request = CompletionRequest {
        model: "test-model",
        context: &context,
        effort: Effort::None,
        tools: &[],
    };

    let (tx, rx) = mpsc::channel(100);
    let result = provider.stream_completion(request, tx).await;

    assert!(result.is_ok());

    let collected = collect_chunks(rx).await;
    assert_eq!(collected.content, vec!["Hello", " LM Studio"]);
    assert!(collected.thinking.is_empty());
}

#[tokio::test]
async fn test_lmstudio_streaming_with_reasoning() {
    let mock_server = MockServer::start().await;

    // Mock SSE response with reasoning (note different event name)
    let sse_response = "\
event: response.reasoning_text.delta
data: {\"delta\":\"Let me think...\"}

event: response.output_text.delta
data: {\"delta\":\"Response\"}

event: response.completed
data: {\"id\":\"test\"}
";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
        .mount(&mock_server)
        .await;

    let provider = LmStudioProvider::new(Some(mock_server.uri()));

    let context = create_test_context();
    let request = CompletionRequest {
        model: "test-model",
        context: &context,
        effort: Effort::Medium,
        tools: &[],
    };

    let (tx, rx) = mpsc::channel(100);
    let result = provider.stream_completion(request, tx).await;

    assert!(result.is_ok());

    let collected = collect_chunks(rx).await;
    assert_eq!(collected.content, vec!["Response"]);
    assert_eq!(collected.thinking, vec!["Let me think..."]);
}

#[tokio::test]
async fn test_lmstudio_handles_unknown_event_types() {
    let mock_server = MockServer::start().await;

    // Include unknown event types
    let sse_response = "\
event: response.created
data: {\"id\":\"test\"}

event: response.in_progress
data: {\"status\":\"working\"}

event: response.output_text.delta
data: {\"delta\":\"Text\"}

event: response.metadata
data: {\"usage\":100}

event: response.completed
data: {\"id\":\"test\"}
";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
        .mount(&mock_server)
        .await;

    let provider = LmStudioProvider::new(Some(mock_server.uri()));

    let context = create_test_context();
    let request = CompletionRequest {
        model: "test-model",
        context: &context,
        effort: Effort::None,
        tools: &[],
    };

    let (tx, rx) = mpsc::channel(100);
    let result = provider.stream_completion(request, tx).await;

    assert!(result.is_ok());

    let collected = collect_chunks(rx).await;
    // Should only receive the text chunk, ignoring unknown events
    assert_eq!(collected.content, vec!["Text"]);
    assert!(collected.thinking.is_empty());
}

// ============================================================================
// Effort Level Tests
// ============================================================================

#[tokio::test]
async fn test_effort_levels_affect_request() {
    let mock_server = MockServer::start().await;

    let sse_response = "event: response.completed\ndata: {\"id\":\"test\"}\n";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response))
        .expect(5) // Will be called 5 times (one for each effort level)
        .mount(&mock_server)
        .await;

    let provider = LmStudioProvider::new(Some(mock_server.uri()));
    let context = create_test_context();

    // Test all effort levels
    for effort in [
        Effort::None,
        Effort::Auto,
        Effort::Low,
        Effort::Medium,
        Effort::High,
    ] {
        let request = CompletionRequest {
            model: "test-model",
            context: &context,
            effort,
            tools: &[],
        };

        let (tx, _rx) = mpsc::channel(100);
        let result = provider.stream_completion(request, tx).await;
        assert!(result.is_ok(), "Failed for effort level: {:?}", effort);
    }
}

