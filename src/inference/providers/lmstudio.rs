//! LM Studio provider implementation using the Responses API.
//!
//! LM Studio v0.3.29+ supports the /v1/responses endpoint with:
//! - Reasoning support with effort parameter
//! - Streaming with SSE events

use std::collections::HashMap;

use async_trait::async_trait;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::inference::{
    CompletionProvider, CompletionRequest, ContextItem, Effort, ProviderError, Source, StreamChunk,
    ToolDefinition, UsageStats,
};

// ============================================================================
// LM Studio Responses API Types
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
    tool_type: &'static str,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

/// SSE event for delta content (used for both text and reasoning)
#[derive(Deserialize, Debug)]
struct DeltaEvent {
    delta: String,
    #[serde(default)]
    item_id: String,
}

/// Converts an empty string to None, non-empty to Some.
fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

/// SSE event for response.output_item.added
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

/// SSE event for response.function_call_arguments.done
/// Note: this event has `item_id` (not `call_id`). The call_id comes from
/// the earlier `response.output_item.added` event and must be captured there.
/// LM Studio omits `name` here (it's in the earlier `output_item.added` event).
#[derive(Deserialize, Debug)]
struct FunctionCallArgsDoneEvent {
    #[serde(default)]
    item_id: String,
    #[serde(default)]
    name: String,
    arguments: String,
}

/// Tracks a tool call across multiple SSE events (added → delta* → done).
/// Unlike OpenRouter, LM Studio sends argument deltas that must be accumulated.
struct PendingToolCall {
    id: String,          // API object ID (e.g. "fc_abc123")
    call_id: String,     // Correlation ID (e.g. "call_xyz789")
    name: String,        // Function name
    args_buffer: String, // Accumulates argument deltas
}

/// Payload of the `response.completed` SSE event.
/// The real structure nests data under a `response` key:
/// `{"type":"response.completed","response":{"id":"...","usage":{...},"status":"completed"}}`
#[derive(Deserialize, Debug)]
struct CompletedResponsePayload {
    #[serde(default)]
    response: Option<CompletedResponse>,
}

/// The inner `response` object from the completed event.
#[derive(Deserialize, Debug)]
struct CompletedResponse {
    #[serde(default)]
    usage: Option<CompletedUsage>,
    #[serde(default)]
    status: Option<String>,
}

/// Token usage breakdown from the completed response.
#[derive(Deserialize, Debug)]
struct CompletedUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
    #[serde(default)]
    total_tokens: Option<u32>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
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

/// Parses the `response.completed` SSE data into `UsageStats`.
/// Returns `None` if parsing fails — we never want to crash over missing metrics.
fn parse_completed_payload(data: &str) -> Option<UsageStats> {
    let payload: CompletedResponsePayload = match serde_json::from_str(data) {
        Ok(p) => p,
        Err(e) => {
            debug!("Failed to parse response.completed payload: {}", e);
            return None;
        }
    };
    let response = payload.response?;
    let usage = response.usage?;
    Some(UsageStats {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
        finish_reason: response.status,
        ..Default::default()
    })
}

// ============================================================================
// Provider Implementation
// ============================================================================

/// LM Studio API provider using Responses API (local inference server)
pub struct LmStudioProvider {
    base_url: String,
    client: reqwest::Client,
}

impl LmStudioProvider {
    pub fn new(base_url: Option<String>) -> Self {
        let env_url = std::env::var("LM_STUDIO_BASE_URL").ok();
        let final_url = base_url
            .or(env_url)
            .unwrap_or_else(|| "http://localhost:1234/v1".to_string());

        Self {
            base_url: final_url,
            client: reqwest::Client::new(),
        }
    }

    /// Sends a request to the Responses endpoint and returns the response.
    async fn send_request(
        &self,
        request: &ResponsesRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let response = self
            .client
            .post(format!("{}/responses", self.base_url))
            .json(request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        debug!("LM Studio response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let err_body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            warn!("LM Studio API error: {} - {}", status, err_body);
            return Err(ProviderError::Api {
                status,
                message: err_body,
            });
        }

        Ok(response)
    }
}

#[async_trait]
impl CompletionProvider for LmStudioProvider {
    async fn stream_completion(
        &self,
        request: CompletionRequest<'_>,
        sender: Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        let reasoning = effort_to_reasoning(request.effort);

        let input = context_to_input(&request.context.items);

        let responses_request = ResponsesRequest {
            model: request.model.to_string(),
            input,
            stream: Some(true),
            reasoning,
            tools: tools_to_api(request.tools),
            max_output_tokens: Some(16384),
        };

        info!(
            "LM Studio Responses API request: model={}, input_count={}, effort={:?}",
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
                    debug!(
                        "SSE data for event {:?}: {} bytes",
                        current_event_type,
                        data.len()
                    );
                    match current_event_type.as_deref() {
                        Some("response.output_text.delta") => {
                            if let Ok(event) = serde_json::from_str::<DeltaEvent>(data)
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
                        Some("response.reasoning_text.delta") => {
                            if let Ok(event) = serde_json::from_str::<DeltaEvent>(data)
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
                                        name: event.item.name,
                                        args_buffer: String::new(),
                                    },
                                );
                            }
                        }
                        Some("response.function_call_arguments.delta") => {
                            if let Ok(event) = serde_json::from_str::<DeltaEvent>(data) {
                                // Append to the matching pending tool's args buffer.
                                // item_id on DeltaEvent may be empty for some LM Studio
                                // versions, so fall back to single-entry heuristic.
                                let entry = if !event.item_id.is_empty() {
                                    pending_tools.get_mut(&event.item_id)
                                } else {
                                    pending_tools.values_mut().next()
                                };
                                if let Some(pending) = entry {
                                    pending.args_buffer.push_str(&event.delta);
                                } else {
                                    warn!("Argument delta for unknown tool call");
                                }
                            }
                        }
                        Some("response.function_call_arguments.done") => {
                            if let Ok(event) =
                                serde_json::from_str::<FunctionCallArgsDoneEvent>(data)
                            {
                                // Look up by item_id; fall back to single-entry heuristic
                                // if LM Studio omits item_id.
                                let pending = if !event.item_id.is_empty() {
                                    pending_tools.remove(&event.item_id)
                                } else {
                                    let key = pending_tools.keys().next().cloned();
                                    key.and_then(|k| pending_tools.remove(&k))
                                };
                                let (id, call_id, name) = match pending {
                                    Some(p) => {
                                        // LM Studio omits `name` from arguments.done —
                                        // use the name from output_item.added.
                                        let name = if event.name.is_empty() {
                                            p.name
                                        } else {
                                            event.name
                                        };
                                        (p.id, p.call_id, name)
                                    }
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
                                    name: name.clone(),
                                    arguments: event.arguments,
                                };
                                debug!(
                                    "Tool call complete: {} (call_id={})",
                                    name, tool_call.call_id
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
                            let stats = parse_completed_payload(data);
                            if sender.send(StreamChunk::Completed(stats)).await.is_err() {
                                warn!("Completed send failed: receiver dropped");
                                return Err(ProviderError::ChannelClosed);
                            }
                            return Ok(());
                        }
                        Some(other) => {
                            debug!("Unrecognized event type '{}' with data: {}", other, data);
                        }
                        None => {
                            // Data without event type - log it
                            debug!("Data without event type: {}", data);
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
    fn test_delta_event_deserializes_correctly() {
        let json = r#"{"delta":"test content"}"#;
        let event: DeltaEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.delta, "test content");
    }

    #[test]
    fn test_delta_event_with_empty_delta() {
        let json = r#"{"delta":""}"#;
        let event: DeltaEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.delta, "");
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
            max_output_tokens: None,
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
            reasoning: effort_to_reasoning(Effort::Medium),
            tools: None,
            max_output_tokens: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""stream":true"#));
        assert!(json.contains(r#""effort":"medium"#));
    }

    #[test]
    fn test_lmstudio_provider_new_with_env_var() {
        unsafe {
            std::env::set_var("LM_STUDIO_BASE_URL", "http://test:1234");
        }
        let provider = LmStudioProvider::new(None);
        assert_eq!(provider.base_url, "http://test:1234");
        unsafe {
            std::env::remove_var("LM_STUDIO_BASE_URL");
        }
    }

    #[test]
    fn test_lmstudio_provider_new_with_explicit_url() {
        unsafe {
            std::env::set_var("LM_STUDIO_BASE_URL", "http://env:1234");
        }
        let provider = LmStudioProvider::new(Some("http://explicit:5678".to_string()));
        assert_eq!(provider.base_url, "http://explicit:5678");
        unsafe {
            std::env::remove_var("LM_STUDIO_BASE_URL");
        }
    }

    #[test]
    fn test_lmstudio_provider_new_with_defaults() {
        unsafe {
            std::env::remove_var("LM_STUDIO_BASE_URL");
        }
        let provider = LmStudioProvider::new(None);
        assert_eq!(provider.base_url, "http://localhost:1234/v1");
    }

    #[test]
    fn test_context_to_input_with_partial_slice() {
        let mut context = Context::new();
        // System directive is items[0]
        context.add(ContextSegment {
            source: Source::User,
            content: "Turn 1".to_string(),
        });
        context.add(ContextSegment {
            source: Source::Model,
            content: "Response 1".to_string(),
        });
        // Watermark = 3 (system + user + model)
        context.add(ContextSegment {
            source: Source::User,
            content: "Turn 2".to_string(),
        });

        // Partial slice: only the new user message
        let partial = context_to_input(&context.items[3..]);
        assert_eq!(partial.len(), 1);
        assert!(
            matches!(&partial[0], InputItem::Message { role: Role::User, content } if content == "Turn 2")
        );

        // Full slice: all items
        let full = context_to_input(&context.items[..]);
        assert_eq!(full.len(), 4); // system + user + model + user
    }

    #[test]
    fn test_parse_completed_payload_with_usage() {
        let data = r#"{"type":"response.completed","response":{"id":"resp_1","usage":{"input_tokens":100,"output_tokens":30,"total_tokens":130},"status":"completed"}}"#;
        let stats = parse_completed_payload(data).unwrap();
        assert_eq!(stats.input_tokens, Some(100));
        assert_eq!(stats.output_tokens, Some(30));
        assert_eq!(stats.total_tokens, Some(130));
        assert_eq!(stats.finish_reason.as_deref(), Some("completed"));
    }

    #[test]
    fn test_parse_completed_payload_without_usage() {
        let data =
            r#"{"type":"response.completed","response":{"id":"resp_1","status":"completed"}}"#;
        let stats = parse_completed_payload(data);
        assert!(stats.is_none());
    }

    #[test]
    fn test_parse_completed_payload_no_response_object() {
        let data = r#"{"type":"response.completed"}"#;
        let stats = parse_completed_payload(data);
        assert!(stats.is_none());
    }

    #[test]
    fn test_parse_completed_payload_invalid_json() {
        let stats = parse_completed_payload("not json");
        assert!(stats.is_none());
    }
}
