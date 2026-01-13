use super::types::{ModelRequest, Context, ModelStreamResponse};
use std::env;
use std::sync::mpsc::Sender;

/// Streams chat completion chunks from the OpenRouter API.
/// # Arguments
/// * `context` - A reference to the Context containing the conversation history.
/// * `sender` - A channel sender to transmit content chunks.
pub async fn stream_completion(context: &Context, sender: Sender<String>) -> Result<(), Box<dyn std::error::Error>> {
    let req = ModelRequest {
        model: env::var("PRIMARY_MODEL_NAME")?,
        messages: context.items.to_vec(),
        stream: Some(true),
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
        return Err(format!("API Error: {}", response.status()).into());
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
                        if let Some(content) = &choice.delta.content {
                            if sender.send(content.clone()).is_err() {
                                return Ok(()); // Receiver dropped
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}