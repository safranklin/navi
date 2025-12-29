use super::types::{ModelSegment, ModelRequest, ModelResponse, Context};
use std::env;

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