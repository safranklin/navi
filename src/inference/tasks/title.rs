//! Title generation for sessions.
//!
//! One public function: `generate_title()`. Internally runs two LLM calls —
//! summarize the conversation, then generate a title from the summary.

use std::sync::Arc;

use log::warn;

use crate::inference::provider::{CompletionProvider, CompletionRequest, ProviderError};
use crate::inference::types::{Context, ContextItem, Effort, Source, StreamChunk};

/// Generate a session title from conversation context.
///
/// Builds a transcript from context items, summarizes it (128 tokens), then
/// generates a concise title from the summary (24 tokens). Returns `None` if
/// there's nothing to summarize or either LLM call fails.
pub async fn generate_title(
    provider: Arc<dyn CompletionProvider>,
    model: &str,
    items: &[ContextItem],
) -> Option<String> {
    let transcript = conversation_transcript(items)?;

    let summary = run_prompt(
        &*provider,
        model,
        "Summarize this conversation in 2-3 sentences. Focus on the main topics discussed \
         and any key decisions or outcomes. Be concise.",
        &transcript,
        128,
    )
    .await
    .map_err(|e| warn!("Title summarization failed: {e}"))
    .ok()?;

    let title = run_prompt(
        &*provider,
        model,
        "Generate a concise title (3-8 words) for this conversation. \
         Return ONLY the title text, no quotes, no explanation.",
        &summary,
        24,
    )
    .await
    .map_err(|e| warn!("Title generation failed: {e}"))
    .ok()?;

    let title = title.trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

/// Run a single LLM prompt: system + user input → collected text output.
async fn run_prompt(
    provider: &dyn CompletionProvider,
    model: &str,
    system: &str,
    input: &str,
    max_tokens: u32,
) -> Result<String, ProviderError> {
    let mut context = Context::with_system_prompt(system.to_string());
    context.add_user_message(input.to_string());

    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);

    let collector = tokio::spawn(async move {
        let mut output = String::new();
        while let Some(chunk) = chunk_rx.recv().await {
            if let StreamChunk::Content { text, .. } = chunk {
                output.push_str(&text);
            }
        }
        output
    });

    let request = CompletionRequest {
        context: &context,
        model,
        effort: Effort::None,
        tools: &[],
        max_output_tokens: Some(max_tokens),
        response_format: None,
    };

    provider.stream_completion(request, chunk_tx).await?;

    let output = collector.await.expect("collector task panicked");
    Ok(output)
}

/// Build a compact transcript of the full conversation for summarization.
///
/// Walks all context items and formats them into a labeled transcript.
/// Each item is truncated to keep the total reasonable for a summarizer prompt.
/// Returns `None` if there are no user messages.
fn conversation_transcript(items: &[ContextItem]) -> Option<String> {
    let mut transcript = String::new();
    let mut has_exchange = false;

    for item in items {
        match item {
            ContextItem::Message(seg) => {
                let (label, include) = match seg.source {
                    Source::User => {
                        has_exchange = true;
                        ("User", true)
                    }
                    Source::Model => ("Assistant", true),
                    Source::Thinking => ("Thinking", false),
                    Source::Directive | Source::Status => continue,
                };
                if include {
                    let truncated: String = seg.content.chars().take(300).collect();
                    transcript.push_str(&format!("[{label}]: {truncated}\n"));
                }
            }
            ContextItem::ToolCall(tc) => {
                let args_trunc: String = tc.arguments.chars().take(100).collect();
                transcript.push_str(&format!("[Tool Call]: {}({})\n", tc.name, args_trunc));
            }
            ContextItem::ToolResult(tr) => {
                let output_trunc: String = tr.output.chars().take(150).collect();
                transcript.push_str(&format!("[Tool Result]: {output_trunc}\n"));
            }
        }
    }

    if has_exchange { Some(transcript) } else { None }
}
