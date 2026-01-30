//! LM Studio provider implementation using the Responses API.
//!
//! LM Studio v0.3.29+ supports the /v1/responses endpoint with:
//! - Stateful interactions via previous_response_id (we don't use this)
//! - Reasoning support with effort parameter
//! - Streaming with SSE events

use async_trait::async_trait;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::inference::{
    CompletionProvider, CompletionRequest, Context, Effort, ProviderError, Source, StreamChunk,
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

/// A single message in the input array
#[derive(Serialize, Debug, Clone)]
struct InputMessage {
    role: Role,
    content: String,
}

/// Configuration for reasoning tokens
#[derive(Serialize, Debug)]
struct Reasoning {
    effort: String, // "low", "medium", or "high"
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

/// SSE event for text content delta
#[derive(Deserialize, Debug)]
struct TextDeltaEvent {
    delta: String,
}

/// SSE event for reasoning delta
#[derive(Deserialize, Debug)]
struct ReasoningDeltaEvent {
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
fn effort_to_string(effort: Effort) -> Option<String> {
    match effort {
        Effort::High => Some("high".to_string()),
        Effort::Medium => Some("medium".to_string()),
        Effort::Low => Some("low".to_string()),
        Effort::None => None,
    }
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
}

#[async_trait]
impl CompletionProvider for LmStudioProvider {
    fn name(&self) -> &str {
        "lmstudio"
    }

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
            "LM Studio Responses API request: model={}, input_count={}, effort={:?}",
            request.model,
            responses_request.input.len(),
            request.effort
        );

        // Make the API request to the Responses endpoint (no auth for local LM Studio)
        let response = self
            .client
            .post(format!("{}/responses", self.base_url))
            .json(&responses_request)
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
                    debug!("SSE data for event {:?}: {} bytes", current_event_type, data.len());
                    match current_event_type.as_deref() {
                        Some("response.output_text.delta") => {
                            if let Ok(event) = serde_json::from_str::<TextDeltaEvent>(data)
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
                        Some("response.reasoning_text.delta") => {
                            if let Ok(event) = serde_json::from_str::<ReasoningDeltaEvent>(data)
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
                            // Log the full completed event data to see if reasoning is in here
                            debug!("response.completed data: {}", data);
                            return Ok(());
                        }
                        Some(other) => {
                            // Log unrecognized event types so we can discover new ones
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

        info!(
            "Stream ended: {} chunks processed, {} total content bytes",
            chunk_count, total_content_len
        );
        Ok(())
    }
}
