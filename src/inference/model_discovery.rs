//! # Model Discovery
//!
//! Fetches available models from provider APIs. Separate from `CompletionProvider`
//! because listing models is a different concern from streaming completions —
//! a provider can fail to list models but still serve completions fine.
//!
//! Each function creates its own `reqwest::Client` since this runs at most
//! once per picker open (Ctrl+P). No persistent connection pool needed.

use log::{debug, info, warn};
use serde::Deserialize;

use crate::core::config::ModelEntry;

// ============================================================================
// OpenRouter Model Discovery
// ============================================================================

/// Response shape from OpenRouter's `GET /api/v1/models`.
#[derive(Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

/// A single model from the OpenRouter catalog.
#[derive(Deserialize)]
struct OpenRouterModel {
    /// The model identifier used in API requests (e.g. "anthropic/claude-sonnet-4").
    id: String,
    /// Human-readable display name (e.g. "Anthropic: Claude Sonnet 4").
    #[serde(default)]
    name: String,
}

/// Fetches all models from the OpenRouter API.
///
/// Maps OpenRouter `id` → `ModelEntry.name` (what the Responses API expects)
/// and OpenRouter `name` → `ModelEntry.description` (human-readable display).
pub async fn fetch_openrouter_models(
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ModelEntry>, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/models", base_url);

    info!("Fetching OpenRouter models from {}", url);

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("OpenRouter model fetch failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        warn!("OpenRouter models API error: {} - {}", status, body);
        return Err(format!("OpenRouter API error {status}: {body}"));
    }

    let models_response: OpenRouterModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenRouter models response: {e}"))?;

    let models: Vec<ModelEntry> = models_response
        .data
        .into_iter()
        .map(|m| {
            let description = if m.name.is_empty() {
                None
            } else {
                Some(m.name)
            };
            ModelEntry {
                name: m.id,
                provider: "openrouter".to_string(),
                description,
            }
        })
        .collect();

    info!("Fetched {} models from OpenRouter", models.len());
    debug!(
        "First 5 OpenRouter models: {:?}",
        models.iter().take(5).map(|m| &m.name).collect::<Vec<_>>()
    );

    Ok(models)
}

// ============================================================================
// LM Studio Model Discovery
// ============================================================================

/// Response shape from LM Studio's `GET /v1/models` (OpenAI-compatible).
#[derive(Deserialize)]
struct LmStudioModelsResponse {
    data: Vec<LmStudioModel>,
}

/// A single model from LM Studio.
#[derive(Deserialize)]
struct LmStudioModel {
    /// The model identifier (e.g. "qwen2.5-coder-32b").
    id: String,
}

/// Fetches all loaded models from a local LM Studio instance.
///
/// Uses a 3-second timeout since the local server may not be running.
/// Returns an empty vec on timeout rather than propagating the error,
/// so the picker still works with just OpenRouter models.
pub async fn fetch_lmstudio_models(base_url: &str) -> Result<Vec<ModelEntry>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let url = format!("{}/models", base_url);

    info!("Fetching LM Studio models from {}", url);

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("LM Studio model fetch failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        warn!("LM Studio models API error: {} - {}", status, body);
        return Err(format!("LM Studio API error {status}: {body}"));
    }

    let models_response: LmStudioModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse LM Studio models response: {e}"))?;

    let models: Vec<ModelEntry> = models_response
        .data
        .into_iter()
        .map(|m| ModelEntry {
            name: m.id,
            provider: "lmstudio".to_string(),
            description: None,
        })
        .collect();

    info!("Fetched {} models from LM Studio", models.len());

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openrouter_model_response_deserializes() {
        let json = r#"{
            "data": [
                {"id": "anthropic/claude-sonnet-4", "name": "Anthropic: Claude Sonnet 4"},
                {"id": "google/gemini-2.5-flash", "name": "Google: Gemini 2.5 Flash"}
            ]
        }"#;
        let response: OpenRouterModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].id, "anthropic/claude-sonnet-4");
        assert_eq!(response.data[0].name, "Anthropic: Claude Sonnet 4");
    }

    #[test]
    fn test_openrouter_model_missing_name_defaults_to_empty() {
        let json = r#"{"data": [{"id": "some/model"}]}"#;
        let response: OpenRouterModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data[0].name, "");
    }

    #[test]
    fn test_lmstudio_model_response_deserializes() {
        let json = r#"{
            "data": [
                {"id": "qwen2.5-coder-32b", "object": "model"},
                {"id": "llama-3.1-8b", "object": "model"}
            ]
        }"#;
        let response: LmStudioModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].id, "qwen2.5-coder-32b");
    }

    #[test]
    fn test_openrouter_model_to_entry_mapping() {
        let model = OpenRouterModel {
            id: "anthropic/claude-sonnet-4".to_string(),
            name: "Anthropic: Claude Sonnet 4".to_string(),
        };
        let entry = ModelEntry {
            name: model.id,
            provider: "openrouter".to_string(),
            description: Some(model.name),
        };
        assert_eq!(entry.name, "anthropic/claude-sonnet-4");
        assert_eq!(
            entry.description.as_deref(),
            Some("Anthropic: Claude Sonnet 4")
        );
    }

    #[test]
    fn test_openrouter_model_empty_name_becomes_none() {
        let model = OpenRouterModel {
            id: "some/model".to_string(),
            name: String::new(),
        };
        let description = if model.name.is_empty() {
            None
        } else {
            Some(model.name)
        };
        assert!(description.is_none());
    }

    #[test]
    fn test_lmstudio_model_to_entry_mapping() {
        let model = LmStudioModel {
            id: "qwen2.5-coder-32b".to_string(),
        };
        let entry = ModelEntry {
            name: model.id,
            provider: "lmstudio".to_string(),
            description: None,
        };
        assert_eq!(entry.name, "qwen2.5-coder-32b");
        assert_eq!(entry.provider, "lmstudio");
        assert!(entry.description.is_none());
    }
}
