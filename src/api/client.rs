use super::types::{Effort, ModelRequest, Context, ModelStreamResponse, ReasoningConfig, Source, StreamChunk};
use std::env;
use std::sync::mpsc::Sender;

/// Streams chat completion chunks from the OpenRouter API.
/// # Arguments
/// * `context` - A reference to the Context containing the conversation history.
/// * `effort` - The reasoning effort level to use for this request.
/// * `sender` - A channel sender to transmit StreamChunk (Thinking or Content).
pub async fn stream_completion(context: &Context, effort: Effort, sender: Sender<StreamChunk>) -> Result<(), Box<dyn std::error::Error>> {
    // DESIGN DECISION: Filter out thinking (reasoning) segments from history when sending to API.
    // Why:
    // 1. Token Efficiency: Reasoning can easily generate a ton of tokens; omitting it saves significant context window and cost.
    // 2. Context Clarity: AI models generally only need the final answers of previous turns to maintain context; 
    //    their internal "thought process" is redundant for future turns and can sometimes cause confusion.
    // 3. User Experience: Thoughts remain in the local TUI history for the user to see, but are hidden from the model.
    let filtered_messages: Vec<_> = context.items.iter()
        .filter(|seg| seg.source != Source::Thinking)
        .cloned()
        .collect();

    // Build reasoning config based on effort level
    let reasoning = if effort == Effort::None {
        None // Don't include reasoning parameter when disabled
    } else {
        Some(ReasoningConfig {
            effort: Some(effort),
            exclude: None,
        })
    };

    let req = ModelRequest {
        model: env::var("PRIMARY_MODEL_NAME")?,
        messages: filtered_messages,
        stream: Some(true),
        reasoning,
    };

    let api_key = env::var("OPENROUTER_API_KEY")?;
    let client = reqwest::Client::new();
    let mut response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&req)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await?;
        return Err(format!("API Error: {} - {}", status, err_body).into());
    }

    let mut buffer = String::new();

    while let Some(chunk) = response.chunk().await? {
        let s = String::from_utf8_lossy(&chunk);
        buffer.push_str(&s);

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].to_string();
            buffer.drain(..pos + 1);

            let line = line.trim();
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    break; // Exit the inner loop
                }
                
                // Parse JSON
                if let Ok(stream_resp) = serde_json::from_str::<ModelStreamResponse>(data) {
                    if let Some(choice) = stream_resp.choices.first() {
                        // Handle reasoning if present
                        if let Some(reasoning) = &choice.delta.reasoning {
                            if !reasoning.is_empty() {
                                if sender.send(StreamChunk::Thinking(reasoning.clone())).is_err() {
                                    return Ok(());
                                }
                            }
                        }
                        // Handle content if present
                        if let Some(content) = &choice.delta.content {
                            if !content.is_empty() {
                                if sender.send(StreamChunk::Content(content.clone())).is_err() {
                                    return Ok(()); // Receiver dropped
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}