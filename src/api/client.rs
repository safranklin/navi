use super::types::{ChatMessage, ChatRequest, ChatResponse};
use std::env;

/// Sends a chat completion request to the OpenRouter API using the provided user message.
/// # Arguments
/// * `user_message` - A reference to the ChatMessage containing the user's input
/// # Returns
/// A Result containing the ChatMessage response from the API or an error.
/// # Example
/// ```no_run
/// use api::client;
/// use api::types::ChatMessage;
/// let user_message = ChatMessage {
///    role: "user".to_string(),
///    content: "Hello, how are you?".to_string(),
/// };
/// match client::chat_completion(&user_message).await {
///     Ok(response_message) => {
///         println!("Response: {:?}", response_message);
///     }
///     Err(e) => {
///         eprintln!("Error: {}", e);
///     }
/// }
/// ```
pub async fn chat_completion(user_message: &ChatMessage) -> Result<ChatMessage, Box <dyn std::error::Error>> {
    let req = ChatRequest {
        model: "nvidia/nemotron-nano-12b-v2-vl:free".to_string(),
        messages: vec![
            user_message.clone(),
        ]
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

    let chat_response: ChatResponse = response.json().await?;

    let message = chat_response.choices.first().ok_or("No valid response from API!")?.message.clone();
    
    Ok(message)
}