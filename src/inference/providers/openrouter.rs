//! OpenRouter provider implementation using the Responses API.
//!
//! This module uses OpenAI Responses API terminology:
//! - "input" (array of messages, not "context")
//! - "role" (not "source")
//! - SSE events: response.output_text.delta, response.reasoning_summary_text.delta

use std::collections::HashMap;

use async_trait::async_trait;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::inference::{
    CompletionProvider, CompletionRequest, ContextItem, Effort, ProviderError, Source, StreamChunk,
    ToolDefinition,
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

/// Polymorphic input item for the Responses API input array.
/// Messages, function calls, and function call outputs are peers at the same level.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
enum InputItem {
    #[serde(rename = "message")]
    Message { role: Role, content: String },
    #[serde(rename = "function_call")]
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        id: String,
        call_id: String,
        output: String,
    },
}

/// Configuration for reasoning tokens
#[derive(Serialize, Debug)]
struct Reasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
}

/// Tool definition for the API request
#[derive(Serialize, Debug)]
struct ApiToolDefinition {
    #[serde(rename = "type")]
    tool_type: &'static str, // always "function"
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// The request body for the Responses API
#[derive(Serialize, Debug)]
struct ResponsesRequest {
    model: String,
    input: Vec<InputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    reasoning: Reasoning,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiToolDefinition>>,
}

/// Generic SSE event wrapper to extract the type field
/// OpenRouter embeds the event type inside the JSON, not in SSE event: lines
#[derive(Deserialize, Debug)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: String,
    #[serde(default)]
    item_id: String,
}

/// Converts an empty string to None, non-empty to Some.
fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

/// SSE event for response.output_item.added (detects function_call output items)
#[derive(Deserialize, Debug)]
struct OutputItemAddedEvent {
    item: OutputItemData,
}

#[derive(Deserialize, Debug)]
struct OutputItemData {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    call_id: String,
    #[serde(default)]
    name: String,
}

/// SSE event for response.function_call_arguments.delta
#[derive(Deserialize, Debug)]
struct FunctionCallArgsDeltaEvent {
    item_id: String,
    #[allow(dead_code)]
    delta: String,
}

/// SSE event for response.function_call_arguments.done
/// The `item_id` correlates back to the `output_item.added` event's `item.id`.
#[derive(Deserialize, Debug)]
struct FunctionCallArgsDoneEvent {
    item_id: String,
    name: String,
    arguments: String,
}

/// Tracks a tool call across multiple SSE events (added → delta* → done).
struct PendingToolCall {
    id: String,      // API object ID (e.g. "fc_abc123")
    call_id: String, // Correlation ID (e.g. "call_xyz789")
}

// ============================================================================
// Translation Layer
// ============================================================================

/// Converts context items into Responses API input format.
///
/// Accepts a slice of ContextItems (full or partial) to support prompt caching.
/// Produces a polymorphic input array: messages, function calls, and function call outputs.
/// Filters out Thinking segments (reasoning is model-generated, not input).
fn context_to_input(items: &[ContextItem]) -> Vec<InputItem> {
    let mut fco_counter = 0usize;
    items
        .iter()
        .filter_map(|item| match item {
            ContextItem::Message(seg) => match seg.source {
                Source::Directive => Some(Role::System),
                Source::User => Some(Role::User),
                Source::Model => Some(Role::Assistant),
                Source::Thinking | Source::Status => None,
            }
            .map(|role| InputItem::Message {
                role,
                content: seg.content.clone(),
            }),
            ContextItem::ToolCall(tc) => Some(InputItem::FunctionCall {
                id: tc.id.clone(),
                call_id: tc.call_id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
            }),
            ContextItem::ToolResult(tr) => {
                fco_counter += 1;
                Some(InputItem::FunctionCallOutput {
                    id: format!("fco_{fco_counter}"),
                    call_id: tr.call_id.clone(),
                    output: tr.output.clone(),
                })
            }
        })
        .collect()
}

/// Converts tool definitions to API format. Returns None if empty (omitted from JSON).
fn tools_to_api(tools: &[ToolDefinition]) -> Option<Vec<ApiToolDefinition>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| ApiToolDefinition {
                tool_type: "function",
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect(),
    )
}

/// Maps our Effort enum to a Reasoning config for the Responses API.
fn effort_to_reasoning(effort: Effort) -> Reasoning {
    match effort {
        Effort::Auto => Reasoning {
            effort: None,
            enabled: Some(true),
        },
        other => {
            let effort = match other {
                Effort::High => "high",
                Effort::Medium => "medium",
                Effort::Low => "low",
                Effort::None => "none",
                Effort::Auto => unreachable!(),
            };
            Reasoning {
                effort: Some(effort),
                enabled: None,
            }
        }
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

    /// Sends a request to the Responses endpoint and returns the response.
    async fn send_request(
        &self,
        request: &ResponsesRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let json_body = serde_json::to_string(request)
            .map_err(|e| ProviderError::Network(format!("Request serialization failed: {e}")))?;
        info!("Raw OpenRouter Request: {}", json_body);

        let response = self
            .client
            .post(format!("{}/responses", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .body(json_body)
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

        Ok(response)
    }
}

#[async_trait]
impl CompletionProvider for OpenRouterProvider {
    async fn stream_completion(
        &self,
        request: CompletionRequest<'_>,
        sender: Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        let reasoning = effort_to_reasoning(request.effort);

        // Always send full context. OpenRouter's Responses API is stateless —
        // it does not persist conversation state between requests. Prompt
        // caching happens transparently via KV cache prefix reuse when the
        // prompt prefix stays stable across turns.
        let input = context_to_input(&request.context.items);

        let responses_request = ResponsesRequest {
            model: request.model.to_string(),
            input,
            stream: Some(true),
            reasoning,
            tools: tools_to_api(request.tools),
        };

        info!(
            "OpenRouter Responses API request: model={}, input_count={}, effort={:?}",
            request.model,
            responses_request.input.len(),
            request.effort,
        );

        let response = self.send_request(&responses_request).await?;

        // Process the SSE stream with typed events
        let mut buffer = String::new();
        let mut current_event_type: Option<String> = None;
        let mut total_content_len = 0usize;
        let mut chunk_count = 0usize;
        let mut response = response;

        // Tool call state: tracks concurrent tool calls by item_id
        let mut pending_tools: HashMap<String, PendingToolCall> = HashMap::new();

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
                                    .send(StreamChunk::Content {
                                        text: event.delta,
                                        item_id: non_empty(event.item_id),
                                    })
                                    .await
                                    .is_err()
                                {
                                    warn!("Content chunk send failed: receiver dropped");
                                    return Err(ProviderError::ChannelClosed);
                                }
                            }
                        }
                        Some("response.reasoning_summary_text.delta")
                        | Some("response.reasoning_text.delta") => {
                            if let Ok(event) = serde_json::from_str::<SseEvent>(data)
                                && !event.delta.is_empty()
                            {
                                chunk_count += 1;
                                debug!("Sending Thinking chunk (len={})", event.delta.len());
                                if sender
                                    .send(StreamChunk::Thinking {
                                        text: event.delta,
                                        item_id: non_empty(event.item_id),
                                    })
                                    .await
                                    .is_err()
                                {
                                    warn!("Thinking chunk send failed: receiver dropped");
                                    return Err(ProviderError::ChannelClosed);
                                }
                            }
                        }
                        Some("response.output_item.added") => {
                            if let Ok(event) = serde_json::from_str::<OutputItemAddedEvent>(data)
                                && event.item.item_type == "function_call"
                            {
                                debug!(
                                    "Tool call started: {} (item_id={}, call_id={})",
                                    event.item.name, event.item.id, event.item.call_id
                                );
                                pending_tools.insert(
                                    event.item.id.clone(),
                                    PendingToolCall {
                                        id: event.item.id,
                                        call_id: event.item.call_id,
                                    },
                                );
                            }
                        }
                        Some("response.function_call_arguments.delta") => {
                            // Delta events are ignored — the done event contains the full arguments.
                            // We parse the event only to validate the item_id correlation.
                            if let Ok(event) =
                                serde_json::from_str::<FunctionCallArgsDeltaEvent>(data)
                                && !pending_tools.contains_key(&event.item_id)
                            {
                                warn!("Argument delta for unknown item_id: {}", event.item_id);
                            }
                        }
                        Some("response.function_call_arguments.done") => {
                            if let Ok(event) =
                                serde_json::from_str::<FunctionCallArgsDoneEvent>(data)
                            {
                                let pending = pending_tools.remove(&event.item_id);
                                let (id, call_id) = match pending {
                                    Some(p) => (p.id, p.call_id),
                                    None => {
                                        warn!(
                                            "arguments.done for unknown item_id: {}, skipping",
                                            event.item_id
                                        );
                                        continue;
                                    }
                                };
                                let tool_call = crate::inference::ToolCall {
                                    id,
                                    call_id,
                                    name: event.name.clone(),
                                    arguments: event.arguments,
                                };
                                debug!(
                                    "Tool call complete: {} (item_id={}, call_id={})",
                                    event.name, event.item_id, tool_call.call_id
                                );
                                chunk_count += 1;
                                if sender.send(StreamChunk::ToolCall(tool_call)).await.is_err() {
                                    warn!("ToolCall send failed: receiver dropped");
                                    return Err(ProviderError::ChannelClosed);
                                }
                            }
                        }
                        Some("response.completed") => {
                            info!(
                                "Stream complete: {} chunks, {} content bytes",
                                chunk_count, total_content_len
                            );
                            debug!("response.completed data: {}", data);
                            if sender.send(StreamChunk::Completed).await.is_err() {
                                warn!("Completed send failed: receiver dropped");
                                return Err(ProviderError::ChannelClosed);
                            }
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

        if !pending_tools.is_empty() {
            warn!(
                "Stream ended with {} unresolved tool call(s): {:?}",
                pending_tools.len(),
                pending_tools.keys().collect::<Vec<_>>()
            );
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

        let input = context_to_input(&context.items);

        // Should have 3 items: Directive (from Context::new), User, and Model
        // Thinking should be filtered out
        assert_eq!(input.len(), 3);
        assert!(matches!(
            &input[0],
            InputItem::Message {
                role: Role::System,
                ..
            }
        ));
        assert!(
            matches!(&input[1], InputItem::Message { role: Role::User, content } if content == "Hello")
        );
        assert!(
            matches!(&input[2], InputItem::Message { role: Role::Assistant, content } if content == "Response")
        );
    }

    #[test]
    fn test_context_to_input_translates_roles_correctly() {
        let mut context = Context::new();
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

        let input = context_to_input(&context.items);

        assert_eq!(input.len(), 3);
        assert!(
            matches!(&input[0], InputItem::Message { role: Role::System, content } if content == "System message")
        );
        assert!(
            matches!(&input[1], InputItem::Message { role: Role::User, content } if content == "User message")
        );
        assert!(
            matches!(&input[2], InputItem::Message { role: Role::Assistant, content } if content == "Model message")
        );
    }

    #[test]
    fn test_effort_to_reasoning_returns_correct_values() {
        assert_eq!(effort_to_reasoning(Effort::High).effort, Some("high"));
        assert_eq!(effort_to_reasoning(Effort::Medium).effort, Some("medium"));
        assert_eq!(effort_to_reasoning(Effort::Low).effort, Some("low"));
        assert_eq!(effort_to_reasoning(Effort::None).effort, Some("none"));
        assert_eq!(effort_to_reasoning(Effort::Auto).effort, None);

        // Auto uses enabled flag, explicit efforts don't
        assert_eq!(effort_to_reasoning(Effort::Auto).enabled, Some(true));
        assert_eq!(effort_to_reasoning(Effort::High).enabled, None);
    }

    #[test]
    fn test_input_item_message_serializes_correctly() {
        let item = InputItem::Message {
            role: Role::User,
            content: "test".to_string(),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains(r#""role":"user"#));
        assert!(json.contains(r#""content":"test"#));
        assert!(json.contains(r#""type":"message"#));
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
    fn test_responses_request_auto_effort() {
        let request = ResponsesRequest {
            model: "test".to_string(),
            input: vec![],
            stream: None,
            reasoning: effort_to_reasoning(Effort::Auto),
            tools: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("stream"));
        assert!(!json.contains("tools"));
        assert!(json.contains(r#""enabled":true"#));
        assert!(!json.contains(r#""effort""#));
    }

    #[test]
    fn test_responses_request_explicit_effort() {
        let request = ResponsesRequest {
            model: "test".to_string(),
            input: vec![],
            stream: Some(true),
            reasoning: effort_to_reasoning(Effort::High),
            tools: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""stream":true"#));
        assert!(json.contains(r#""effort":"high"#));
    }

    #[test]
    fn test_responses_request_reasoning_off() {
        let request = ResponsesRequest {
            model: "test".to_string(),
            input: vec![],
            stream: Some(true),
            reasoning: effort_to_reasoning(Effort::None),
            tools: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""effort":"none"#));
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
