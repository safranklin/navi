//! OpenRouter provider implementation using the Responses API.
//!
//! This module uses OpenAI Responses API terminology:
//! - "input" (array of messages, not "context")
//! - "role" (not "source")
//! - SSE events: response.output_text.delta, response.reasoning_summary_text.delta

use async_trait::async_trait;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::inference::{
    CompletionProvider, CompletionRequest, Context, Effort, ProviderError, Source, StreamChunk,
};

// ============================================================================
// OpenRouter Responses API Types
// ============================================================================

/// Role in an input message (OpenAI terminology)
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
enum Role {
    System,
    User,
    Assistant,
}

/// A single message in the input array
#[derive(Serialize, Debug, Clone)]
struct InputMessage {
    role: Role,
    content: String,
}

/// Configuration for reasoning tokens
#[derive(Serialize, Debug)]
struct Reasoning {
    effort: &'static str, // "low", "medium", or "high"
}

/// The request body for the Responses API
#[derive(Serialize, Debug)]
struct ResponsesRequest {
    model: String,
    input: Vec<InputMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<Reasoning>,
}

/// Generic SSE event wrapper to extract the type field
/// OpenRouter embeds the event type inside the JSON, not in SSE event: lines
#[derive(Deserialize, Debug)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: String,
}

// ============================================================================
// Translation Layer
// ============================================================================

/// Converts Navi's Context into Responses API input format.
///
/// Filters out Thinking segments (reasoning is model-generated, not input).
fn context_to_input(context: &Context) -> Vec<InputMessage> {
    context
        .items
        .iter()
        .filter_map(|item| {
            match item.source {
                Source::Directive => Some(Role::System),
                Source::User => Some(Role::User),
                Source::Model => Some(Role::Assistant),
                Source::Thinking => None, // Skip Thinking items
            }
            .map(|role| InputMessage {
                role,
                content: item.content.clone(),
            })
        })
        .collect()
}

/// Maps our Effort enum to Responses API effort string.
/// Returns None for Effort::None (omit reasoning entirely).
fn effort_to_string(effort: Effort) -> Option<&'static str> {
    match effort {
        Effort::High => Some("high"),
        Effort::Medium => Some("medium"),
        Effort::Low => Some("low"),
        Effort::None => None,
    }
}

// ============================================================================
// Provider Implementation
// ============================================================================

/// OpenRouter API provider using Responses API
pub struct OpenRouterProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenRouterProvider {
    /// Creates a new OpenRouter provider.
    ///
    /// # Arguments
    /// * `api_key` - OpenRouter API key
    /// * `base_url` - Optional custom base URL (defaults to OpenRouter's API)
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string()),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl CompletionProvider for OpenRouterProvider {
    async fn stream_completion(
        &self,
        request: CompletionRequest<'_>,
        sender: Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        // Build the reasoning config (None if effort is None)
        let reasoning = effort_to_string(request.effort).map(|effort| Reasoning { effort });

        // Translate domain types to Responses API format
        let input = context_to_input(request.context);

        let responses_request = ResponsesRequest {
            model: request.model.to_string(),
            input,
            stream: Some(true),
            reasoning,
        };

        info!(
            "OpenRouter Responses API request: model={}, input_count={}, effort={:?}",
            request.model,
            responses_request.input.len(),
            request.effort
        );

        // Make the API request to the Responses endpoint
        let response = self
            .client
            .post(format!("{}/responses", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&responses_request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        debug!("OpenRouter response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let err_body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            warn!("OpenRouter API error: {} - {}", status, err_body);
            return Err(ProviderError::Api {
                status,
                message: err_body,
            });
        }

        // Process the SSE stream with typed events
        let mut buffer = String::new();
        let mut current_event_type: Option<String> = None;
        let mut total_content_len = 0usize;
        let mut chunk_count = 0usize;
        let mut response = response;

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?
        {
            let s = String::from_utf8_lossy(&chunk);
            debug!("Raw chunk received: {} bytes", chunk.len());
            buffer.push_str(&s);

            // Process complete lines from buffer
            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].to_string();
                buffer.drain(..pos + 1);

                let line = line.trim();

                // Log all non-empty lines for debugging
                if !line.is_empty() {
                    debug!("SSE line: {}", line);
                }

                // Parse SSE event type
                if let Some(event_type) = line.strip_prefix("event: ") {
                    debug!("SSE event type: {}", event_type);
                    current_event_type = Some(event_type.to_string());
                    continue;
                }

                // Parse SSE data
                if let Some(data) = line.strip_prefix("data: ") {
                    // Skip [DONE] marker
                    if data == "[DONE]" {
                        debug!("Received [DONE] marker");
                        continue;
                    }

                    // OpenRouter embeds type in JSON, not in event: lines
                    // Parse the JSON to extract the type field
                    let event_type = current_event_type.clone().or_else(|| {
                        serde_json::from_str::<SseEvent>(data)
                            .ok()
                            .map(|e| e.event_type)
                    });

                    debug!("SSE data for event {:?}: {} bytes", event_type, data.len());

                    match event_type.as_deref() {
                        Some("response.output_text.delta") => {
                            if let Ok(event) = serde_json::from_str::<SseEvent>(data)
                                && !event.delta.is_empty()
                            {
                                chunk_count += 1;
                                total_content_len += event.delta.len();
                                debug!(
                                    "Sending Content chunk (len={}, total={})",
                                    event.delta.len(),
                                    total_content_len
                                );
                                if sender
                                    .send(StreamChunk::Content(event.delta))
                                    .await
                                    .is_err()
                                {
                                    warn!("Content chunk send failed: receiver dropped");
                                    return Err(ProviderError::ChannelClosed);
                                }
                            }
                        }
                        Some("response.reasoning_summary_text.delta") => {
                            if let Ok(event) = serde_json::from_str::<SseEvent>(data)
                                && !event.delta.is_empty()
                            {
                                chunk_count += 1;
                                debug!("Sending Thinking chunk (len={})", event.delta.len());
                                if sender
                                    .send(StreamChunk::Thinking(event.delta))
                                    .await
                                    .is_err()
                                {
                                    warn!("Thinking chunk send failed: receiver dropped");
                                    return Err(ProviderError::ChannelClosed);
                                }
                            }
                        }
                        Some("response.completed") => {
                            info!(
                                "Stream complete: {} chunks, {} total content bytes",
                                chunk_count, total_content_len
                            );
                            debug!("response.completed data: {}", data);
                            return Ok(());
                        }
                        Some(other) => {
                            // Ignore other event types (response.created, response.in_progress, etc.)
                            debug!("Ignoring event type '{}': {} bytes", other, data.len());
                        }
                        None => {
                            debug!("Could not parse event type from data: {}", data);
                        }
                    }

                    // Reset event type after processing data
                    current_event_type = None;
                }
            }
        }

        info!(
            "Stream ended: {} chunks processed, {} total content bytes",
            chunk_count, total_content_len
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{Context, ContextSegment, Effort, Source};

    #[test]
    fn test_context_to_input_filters_thinking() {
        let mut context = Context::new();
        context.add(ContextSegment {
            source: Source::User,
            content: "Hello".to_string(),
        });
        context.add(ContextSegment {
            source: Source::Thinking,
            content: "Internal thought".to_string(),
        });
        context.add(ContextSegment {
            source: Source::Model,
            content: "Response".to_string(),
        });

        let input = context_to_input(&context);

        // Should have 3 items: Directive (from Context::new), User, and Model
        // Thinking should be filtered out
        assert_eq!(input.len(), 3);
        assert!(matches!(input[0].role, Role::System));
        assert!(matches!(input[1].role, Role::User));
        assert_eq!(input[1].content, "Hello");
        assert!(matches!(input[2].role, Role::Assistant));
        assert_eq!(input[2].content, "Response");
    }

    #[test]
    fn test_context_to_input_translates_roles_correctly() {
        let mut context = Context::new();
        // Clear default directive for this test
        context.items.clear();

        context.add(ContextSegment {
            source: Source::Directive,
            content: "System message".to_string(),
        });
        context.add(ContextSegment {
            source: Source::User,
            content: "User message".to_string(),
        });
        context.add(ContextSegment {
            source: Source::Model,
            content: "Model message".to_string(),
        });

        let input = context_to_input(&context);

        assert_eq!(input.len(), 3);
        assert!(matches!(input[0].role, Role::System));
        assert_eq!(input[0].content, "System message");
        assert!(matches!(input[1].role, Role::User));
        assert_eq!(input[1].content, "User message");
        assert!(matches!(input[2].role, Role::Assistant));
        assert_eq!(input[2].content, "Model message");
    }

    #[test]
    fn test_effort_to_string_returns_correct_values() {
        assert_eq!(effort_to_string(Effort::High), Some("high"));
        assert_eq!(effort_to_string(Effort::Medium), Some("medium"));
        assert_eq!(effort_to_string(Effort::Low), Some("low"));
        assert_eq!(effort_to_string(Effort::None), None);
    }

    #[test]
    fn test_input_message_serializes_correctly() {
        let msg = InputMessage {
            role: Role::User,
            content: "test".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user"#));
        assert!(json.contains(r#""content":"test"#));
    }

    #[test]
    fn test_role_serialization() {
        let system = serde_json::to_string(&Role::System).unwrap();
        assert_eq!(system, "\"system\"");

        let user = serde_json::to_string(&Role::User).unwrap();
        assert_eq!(user, "\"user\"");

        let assistant = serde_json::to_string(&Role::Assistant).unwrap();
        assert_eq!(assistant, "\"assistant\"");
    }

    #[test]
    fn test_responses_request_omits_none_fields() {
        let request = ResponsesRequest {
            model: "test".to_string(),
            input: vec![],
            stream: None,
            reasoning: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        // None fields should be omitted
        assert!(!json.contains("stream"));
        assert!(!json.contains("reasoning"));
    }

    #[test]
    fn test_responses_request_includes_some_fields() {
        let request = ResponsesRequest {
            model: "test".to_string(),
            input: vec![],
            stream: Some(true),
            reasoning: Some(Reasoning {
                effort: "high",
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""stream":true"#));
        assert!(json.contains(r#""effort":"high"#));
    }

    #[test]
    fn test_sse_event_deserialization_with_embedded_type() {
        let json = r#"{"type":"response.output_text.delta","delta":"Hello"}"#;
        let event: SseEvent = serde_json::from_str(json).unwrap();

        assert_eq!(event.event_type, "response.output_text.delta");
        assert_eq!(event.delta, "Hello");
    }

    #[test]
    fn test_sse_event_deserialization_missing_delta() {
        let json = r#"{"type":"response.created"}"#;
        let event: SseEvent = serde_json::from_str(json).unwrap();

        assert_eq!(event.event_type, "response.created");
        assert_eq!(event.delta, ""); // Default is empty string
    }
}
