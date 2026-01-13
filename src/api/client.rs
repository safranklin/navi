use super::types::{ModelSegment, ModelRequest, ModelResponse, Context, ModelStreamResponse};
use std::env;
use std::sync::mpsc::Sender;

/// Sends a chat completion request to the OpenRouter API using the provided user message.
/// # Arguments
/// * `context` - A reference to the Context containing the conversation history.
/// # Returns
/// A Result containing the ModelSegment response from the API or an error.
/// # Example
/// ```no_run
/// use api::client;
/// use api::types::{ModelSegment, Source};
/// let mut context = Context::new();
/// let user_message = ModelSegment {
///    source: Source::User,
///    content: String::from("Hello, how are you?"),
/// };
/// 
/// context.add(user_message);
/// 
/// match client::model_completion(&context).await {
///     Ok(response) => {
///         println!("Response: {:?}", response);
///     }
///     Err(e) => {
///         eprintln!("Error: {}", e);
///     }
/// }
/// ```
pub async fn model_completion(context: &Context) -> Result<ModelSegment, Box <dyn std::error::Error>> {
    let req = ModelRequest {
        model: env::var("PRIMARY_MODEL_NAME")?,
        messages: context.items.to_vec(),
        stream: Some(false),
    };

    // Retrieve the API key from environment variables
    let api_key = env::var("OPENROUTER_API_KEY")?;

    let client = reqwest::Client::new();
    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&req)
        .send()
        .await?;

    let res: ModelResponse = response.json().await?;

    let message = res.choices.first().ok_or("No valid response from API!")?.message.normalized();
    
    Ok(message)
}

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