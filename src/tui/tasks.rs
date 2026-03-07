//! Background task spawners for async operations (API requests, tool execution, model fetching).

use log::{debug, info, warn};
use std::sync::{Arc, mpsc};

use crate::core::action::Action;
use crate::core::state::App;
use crate::inference::{CompletionRequest, StreamChunk, model_discovery};
use crate::tui::stream_buffer::{BufferableChunk, ChunkKind, SmoothedChunk, StreamBuffer};

pub fn spawn_tool_execution(
    tool_call: crate::inference::ToolCall,
    registry: Arc<crate::core::tools::ToolRegistry>,
    tx: mpsc::Sender<Action>,
) {
    info!(
        "Spawning tool execution: {} (call_id={})",
        tool_call.name, tool_call.call_id
    );
    tokio::spawn(async move {
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            registry.execute(&tool_call),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    "Tool '{}' timed out after 30s (call_id={})",
                    tool_call.name, tool_call.call_id
                );
                serde_json::json!({"error": "Tool execution timed out after 30s"}).to_string()
            }
        };
        if tx
            .send(Action::ToolResultReady {
                call_id: tool_call.call_id.clone(),
                output,
            })
            .is_err()
        {
            warn!(
                "Failed to send tool result for call_id={}: receiver dropped",
                tool_call.call_id
            );
        }
    });
}

pub fn spawn_request(app: &App, tx: mpsc::Sender<Action>) -> Vec<tokio::task::AbortHandle> {
    info!("Spawning API request");

    // Clone what we need for the async task
    let provider = app.provider.clone();
    let context = app.session.context.clone();
    let model = app.model.name.clone();
    let effort = app.effort;
    let tools = app.tool_definitions();
    let max_output_tokens = Some(app.max_output_tokens);

    // Async channel for streaming chunks
    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<StreamChunk>(100);

    // Clone tx for the streaming task
    let tx_stream = tx.clone();

    // Spawn the provider streaming task
    let stream_handle = tokio::spawn(async move {
        let request = CompletionRequest {
            context: &context,
            model: &model,
            effort,
            tools: &tools,
            max_output_tokens,
        };

        if let Err(e) = provider.stream_completion(request, chunk_tx).await {
            info!("Stream error: {}", e);
            if tx_stream
                .send(Action::ResponseChunk {
                    text: format!("\n[Error: {}]", e),
                    item_id: None,
                })
                .is_err()
            {
                warn!("Failed to send stream error action: receiver dropped");
            }
        }
    });

    // Spawn a task to forward chunks to the Action channel via a smoothing buffer
    let forward_handle = tokio::spawn(async move {
        let mut buffer = StreamBuffer::new(2, 10); // content: 2 chars/tick, thinking: 20 chars/tick
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(16)); // 60fps cadence, ~125 c/s content
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut forwarded_count = 0usize;
        let mut total_content_len = 0usize;
        let mut stream_ended = false;
        let mut got_completed = false;
        let mut completed_stats: Option<crate::inference::UsageStats> = None;

        // Client-side timing
        let request_start = std::time::Instant::now();
        let mut first_content_time: Option<std::time::Instant> = None;

        loop {
            tokio::select! {
                chunk = chunk_rx.recv(), if !stream_ended => {
                    match chunk {
                        Some(chunk @ (StreamChunk::Content { .. } | StreamChunk::Thinking { .. })) => {
                            let (text, item_id, kind) = match chunk {
                                StreamChunk::Content { text, item_id } => {
                                    total_content_len += text.len();
                                    (text, item_id, ChunkKind::Content)
                                }
                                StreamChunk::Thinking { text, item_id } => {
                                    (text, item_id, ChunkKind::Thinking)
                                }
                                _ => unreachable!(),
                            };
                            if first_content_time.is_none() {
                                first_content_time = Some(std::time::Instant::now());
                            }
                            forwarded_count += 1;
                            buffer.push(BufferableChunk { kind, item_id, text });
                        }
                        Some(StreamChunk::ToolCall(tc)) => {
                            // Flush buffered text before passing through tool calls
                            if flush_and_send(&mut buffer, &tx, true) {
                                return;
                            }
                            debug!("Forwarding ToolCall: {} (call_id={})", tc.name, tc.call_id);
                            if tx.send(Action::ToolCallReceived(tc)).is_err() {
                                warn!("Failed to forward ToolCall: receiver dropped");
                                return;
                            }
                        }
                        Some(StreamChunk::Completed(provider_stats)) => {
                            got_completed = true;
                            completed_stats = provider_stats;
                            stream_ended = true;
                        }
                        None => {
                            // Channel closed without Completed
                            stream_ended = true;
                        }
                    }
                }
                _ = ticker.tick() => {
                    if flush_and_send(&mut buffer, &tx, false) {
                        return;
                    }

                    if stream_ended && buffer.is_empty() {
                        // Build and send final stats
                        if got_completed {
                            let duration_ms = request_start.elapsed().as_millis() as u64;
                            let ttft_ms = first_content_time
                                .map(|t| (t - request_start).as_millis() as u64);

                            let mut stats = completed_stats.take().unwrap_or_default();
                            stats.ttft_ms = ttft_ms;
                            stats.generation_duration_ms = Some(duration_ms);
                            if let Some(output_tokens) = stats.output_tokens
                                && duration_ms > 0
                            {
                                stats.tokens_per_sec = Some(
                                    output_tokens as f32 / (duration_ms as f32 / 1000.0),
                                );
                            }

                            debug!(
                                "Stream completed: ttft={}ms, duration={}ms, tok/s={:?}",
                                ttft_ms.unwrap_or(0), duration_ms, stats.tokens_per_sec
                            );
                            info!(
                                "Forwarding complete: {} actions, {} content bytes",
                                forwarded_count, total_content_len
                            );
                            if tx.send(Action::ResponseDone(Some(stats))).is_err() {
                                warn!("Failed to send ResponseDone: receiver dropped");
                            }
                        } else {
                            info!(
                                "Stream channel closed: {} actions, {} content bytes",
                                forwarded_count, total_content_len
                            );
                            if tx.send(Action::ResponseDone(None)).is_err() {
                                warn!("Failed to send ResponseDone: receiver dropped");
                            }
                        }
                        return;
                    }
                }
            }
        }
    });

    vec![stream_handle.abort_handle(), forward_handle.abort_handle()]
}

/// Spawns a background task to fetch models from all configured providers.
///
/// Runs OpenRouter and LM Studio fetches concurrently via `tokio::join!`.
/// LM Studio has a 3s timeout so it won't block if the server isn't running.
/// Results are deduped against pinned models in `ModelPickerState::set_fetched_models()`.
pub fn spawn_model_fetch(app: &App, tx: mpsc::Sender<Action>) {
    let openrouter_base_url = app.config.openrouter_base_url.clone();
    let openrouter_api_key = app.config.openrouter_api_key.clone();
    let lmstudio_base_url = app.config.lmstudio_base_url.clone();

    tokio::spawn(async move {
        let (or_result, lms_result) = tokio::join!(
            async {
                match openrouter_api_key {
                    Some(ref key) => {
                        model_discovery::fetch_openrouter_models(&openrouter_base_url, key).await
                    }
                    None => {
                        warn!("No OpenRouter API key — skipping model fetch");
                        Ok(Vec::new())
                    }
                }
            },
            async { model_discovery::fetch_lmstudio_models(&lmstudio_base_url).await }
        );

        let mut all_models = Vec::new();

        match or_result {
            Ok(models) => all_models.extend(models),
            Err(e) => warn!("OpenRouter model fetch failed: {}", e),
        }

        match lms_result {
            Ok(models) => all_models.extend(models),
            Err(e) => debug!(
                "LM Studio model fetch failed (server may not be running): {}",
                e
            ),
        }

        info!("Model fetch complete: {} total models", all_models.len());

        if tx.send(Action::ModelsFetched(all_models)).is_err() {
            warn!("Failed to send ModelsFetched: receiver dropped");
        }
    });
}

/// Flush the stream buffer and send resulting Actions. If `all` is true, uses flush_all.
/// Returns true if the receiver has been dropped (caller should return).
fn flush_and_send(buffer: &mut StreamBuffer, tx: &mpsc::Sender<Action>, all: bool) -> bool {
    let chunks: Vec<SmoothedChunk> = if all {
        buffer.flush_all()
    } else {
        buffer.flush()
    };

    for chunk in chunks {
        let action = match chunk.kind {
            ChunkKind::Content => Action::ResponseChunk {
                text: chunk.text,
                item_id: chunk.item_id,
            },
            ChunkKind::Thinking => Action::ThinkingChunk {
                text: chunk.text,
                item_id: chunk.item_id,
            },
        };
        if tx.send(action).is_err() {
            warn!("Failed to send buffered chunk: receiver dropped");
            return true;
        }
    }
    false
}
