//! Title generation pipeline.
//!
//! Two modes:
//! - **First response**: extract the first user+model exchange, run a single title prompt.
//! - **Close time / session switch**: build a full transcript, summarize → title via `Chain`.
//!
//! Input builders take `&[ContextItem]` so they're decoupled from `App` / TUI state.

use std::sync::Arc;

use log::warn;

use crate::inference::provider::CompletionProvider;
use crate::inference::task::{Chain, Prompt, Task};
use crate::inference::types::{ContextItem, Source};

// ============================================================================
// Input builders
// ============================================================================

/// Extracts the first user + model exchange from context items, truncated.
///
/// Returns a formatted string like `"<user text>\n---\n<model text>"`, suitable
/// for feeding directly into `title_prompt().run()`. Returns `None` if the
/// context doesn't yet contain both a user and model message.
pub fn first_exchange(items: &[ContextItem]) -> Option<String> {
    let mut user_msg = None;
    let mut model_msg = None;

    for item in items {
        if let ContextItem::Message(seg) = item {
            match seg.source {
                Source::User if user_msg.is_none() => {
                    user_msg = Some(&seg.content);
                }
                Source::Model if model_msg.is_none() => {
                    model_msg = Some(&seg.content);
                }
                _ => {}
            }
        }
        if user_msg.is_some() && model_msg.is_some() {
            break;
        }
    }

    let (user, model) = (user_msg?, model_msg?);
    let user_trunc: String = user.chars().take(250).collect();
    let model_trunc: String = model.chars().take(250).collect();
    Some(format!("{user_trunc}\n---\n{model_trunc}"))
}

/// Builds a compact transcript of the full conversation for summarization.
///
/// Walks all context items — user messages, model responses, tool calls, and
/// tool results — and formats them into a labeled transcript. Each item is
/// truncated to keep the total reasonable for a summarizer prompt.
///
/// Returns `None` if there are no user messages (nothing to summarize).
pub fn conversation_transcript(items: &[ContextItem]) -> Option<String> {
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

    if has_exchange {
        Some(transcript)
    } else {
        None
    }
}

// ============================================================================
// Task factories
// ============================================================================

/// A single-step title prompt: `String -> String` (24 tokens max).
///
/// Suitable for lightweight title generation from a short input (e.g. the
/// first exchange). For longer conversations, use `summarize_then_title()`
/// which compresses the conversation first.
pub fn title_prompt(provider: Arc<dyn CompletionProvider>, model: &str) -> Prompt {
    Prompt::new(provider, model)
        .system(
            "Generate a concise title (3-8 words) for this conversation. \
             Return ONLY the title text, no quotes, no explanation.",
        )
        .max_tokens(24)
}

/// Two-step pipeline: summarize (128 tokens) → title (24 tokens).
///
/// Uses `.then()` composition — first real consumer of the `Chain` combinator.
/// The summarizer compresses a full conversation transcript into 2-3 sentences,
/// then the titler generates a short title from that summary. This captures
/// topic drift across long conversations without sending the full transcript
/// to the title prompt.
pub fn summarize_then_title(
    provider: Arc<dyn CompletionProvider>,
    model: &str,
) -> Chain<Prompt, Prompt> {
    let summarizer = Prompt::new(provider.clone(), model)
        .system(
            "Summarize this conversation in 2-3 sentences. Focus on the main topics discussed \
             and any key decisions or outcomes. Be concise.",
        )
        .max_tokens(128);

    let titler = title_prompt(provider, model);

    summarizer.then(titler)
}

// ============================================================================
// Runner
// ============================================================================

/// Runs a title-producing task, handling errors and empty results.
///
/// Trims the output, returns `None` if the model returned empty text or if
/// the task failed (logged as a warning). This centralizes the error/empty
/// handling that every caller needs.
pub async fn generate_title(
    task: &impl Task<Input = String, Output = String>,
    input: String,
) -> Option<String> {
    match task.run(input).await {
        Ok(t) => {
            let t = t.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        }
        Err(e) => {
            warn!("Title generation failed: {}", e);
            None
        }
    }
}
