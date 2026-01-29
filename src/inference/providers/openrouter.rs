//! OpenRouter provider implementation.
//!
//! This module uses OpenRouter/OpenAI API terminology internally:
//! - "messages" (not "context")
//! - "role" (not "source")
//! - "reasoning" (not "thinking")

use async_trait::async_trait;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::inference::{
    CompletionProvider, CompletionRequest, Context, Effort, ProviderError, Source, StreamChunk,
};

// ============================================================================
// OpenRouter API Types
// ============================================================================

/// Role in a chat message (OpenAI/OpenRouter terminology)
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
enum Role {
    System,
    User,
    Assistant,
}

/// A single message in the chat (OpenRouter format)
#[derive(Serialize, Debug, Clone)]
struct Message {
    role: Role,
    content: String,
}

/// Configuration for reasoning/thinking tokens
#[derive(Serialize, Debug)]
struct Reasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<Effort>,
}

/// The request body for chat completions
#[derive(Serialize, Debug)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<Reasoning>,
}

/// Streaming response from OpenRouter
#[derive(Deserialize, Debug)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize, Debug)]
struct StreamChoice {
    delta: Delta,
}

#[derive(Deserialize, Debug)]
struct Delta {
    content: Option<String>,
    reasoning: Option<String>,
}

// ============================================================================
// Translation Layer
// ============================================================================

/// Converts Navi's Context into OpenRouter's message format.
///
/// Filters out Thinking segments.
fn context_to_messages(context: &Context) -> Vec<Message> {
    context.items.iter().filter_map(|item| {
        match item.source {
            Source::Directive => Some(Role::System),
            Source::User => Some(Role::User),
            Source::Model => Some(Role::Assistant),
            Source::Thinking => None, // Skip Thinking items
        }.map(|role| Message {
            role,
            content: item.content.clone(),
        })
    }).collect()
}

// ============================================================================
// Provider Implementation
// ============================================================================

/// OpenRouter API provider
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
    fn name(&self) -> &str {
        "openrouter"
    }

    async fn stream_completion(
        &self,
        request: CompletionRequest<'_>,
        sender: Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        // Build the reasoning config
        let reasoning = if request.effort == Effort::None {
            None
        } else {
            Some(Reasoning {
                effort: Some(request.effort),
            })
        };

        // Translate domain types to OpenRouter format
        let messages = context_to_messages(request.context);

        let chat_request = ChatCompletionRequest {
            model: request.model.to_string(),
            messages,
            stream: Some(true),
            reasoning,
        };

        info!(
            "OpenRouter request: model={}, messages={}, effort={:?}",
            request.model,
            chat_request.messages.len(),
            request.effort
        );

        // Make the API request
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&chat_request)
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

        // Process the SSE stream
        let mut buffer = String::new();
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

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].to_string();
                buffer.drain(..pos + 1);

                let line = line.trim();
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        info!(
                            "Stream complete: {} chunks, {} total content bytes",
                            chunk_count, total_content_len
                        );
                        return Ok(());
                    }

                    // Parse JSON
                    if let Ok(stream_resp) = serde_json::from_str::<StreamResponse>(data) {
                        if let Some(choice) = stream_resp.choices.first() {
                            // Handle reasoning (thinking) if present
                            if let Some(reasoning) = &choice.delta.reasoning {
                                if !reasoning.is_empty() {
                                    chunk_count += 1;
                                    debug!("Sending Thinking chunk (len={})", reasoning.len());
                                    if sender.send(StreamChunk::Thinking(reasoning.clone())).await.is_err() {
                                        warn!("Thinking chunk send failed: receiver dropped");
                                        return Err(ProviderError::ChannelClosed);
                                    }
                                }
                            }
                            // Handle content if present
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    chunk_count += 1;
                                    total_content_len += content.len();
                                    debug!(
                                        "Sending Content chunk (len={}, total={})",
                                        content.len(),
                                        total_content_len
                                    );
                                    if sender.send(StreamChunk::Content(content.clone())).await.is_err() {
                                        warn!("Content chunk send failed: receiver dropped");
                                        return Err(ProviderError::ChannelClosed);
                                    }
                                }
                            }
                        }
                    }
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
