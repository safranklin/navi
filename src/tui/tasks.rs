//! # Background Task Spawners
//!
//! One-shot orchestration functions that kick off async work from the TUI
//! event loop. Each function grabs the state it needs from `App`, spawns
//! a `tokio` task, and pipes results back through the `Action` channel.

use log::{debug, info, warn};
use std::sync::Arc;
use std::sync::mpsc;

use crate::core::action::Action;
use crate::core::session;
use crate::core::state::App;
use crate::inference::tasks::title;
use crate::inference::{CompletionRequest, StreamChunk};

/// Spawns the LLM streaming request, returning abort handles for cancellation.
///
/// Two tasks are spawned:
/// 1. The provider streaming task (drives `stream_completion`)
/// 2. A forwarder that bridges the async channel to the sync `mpsc::Sender<Action>`
pub fn spawn_request(app: &App, tx: mpsc::Sender<Action>) -> Vec<tokio::task::AbortHandle> {
    info!("Spawning API request");

    // Clone what we need for the async task
    let provider = app.provider.clone();
    let context = app.context.clone();
    let model = app.model_name.clone();
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
            response_format: None,
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

    // Spawn a task to forward chunks to the Action channel
    let forward_handle = tokio::spawn(async move {
        let mut forwarded_count = 0usize;
        let mut total_content_len = 0usize;

        // Client-side timing
        let request_start = std::time::Instant::now();
        let mut first_content_time: Option<std::time::Instant> = None;

        while let Some(chunk) = chunk_rx.recv().await {
            forwarded_count += 1;
            match chunk {
                StreamChunk::Content { text, item_id } => {
                    if first_content_time.is_none() {
                        first_content_time = Some(std::time::Instant::now());
                    }
                    total_content_len += text.len();
                    debug!(
                        "Forwarding Action::ResponseChunk (len={}, total={})",
                        text.len(),
                        total_content_len
                    );
                    if tx.send(Action::ResponseChunk { text, item_id }).is_err() {
                        warn!("Failed to forward ResponseChunk: receiver dropped");
                        return;
                    }
                }
                StreamChunk::Thinking { text, item_id } => {
                    if first_content_time.is_none() {
                        first_content_time = Some(std::time::Instant::now());
                    }
                    debug!("Forwarding Action::ThinkingChunk (len={})", text.len());
                    if tx.send(Action::ThinkingChunk { text, item_id }).is_err() {
                        warn!("Failed to forward ThinkingChunk: receiver dropped");
                        return;
                    }
                }
                StreamChunk::ToolCall(tc) => {
                    debug!("Forwarding ToolCall: {} (call_id={})", tc.name, tc.call_id);
                    if tx.send(Action::ToolCallReceived(tc)).is_err() {
                        warn!("Failed to forward ToolCall: receiver dropped");
                        return;
                    }
                }
                StreamChunk::Completed(provider_stats) => {
                    let duration_ms = request_start.elapsed().as_millis() as u64;
                    let ttft_ms =
                        first_content_time.map(|t| (t - request_start).as_millis() as u64);

                    // Enrich provider stats with client-side timing
                    let mut stats = provider_stats.unwrap_or_default();
                    stats.ttft_ms = ttft_ms;
                    stats.generation_duration_ms = Some(duration_ms);
                    if let Some(output_tokens) = stats.output_tokens
                        && duration_ms > 0
                    {
                        stats.tokens_per_sec =
                            Some(output_tokens as f32 / (duration_ms as f32 / 1000.0));
                    }

                    debug!(
                        "Stream completed: ttft={}ms, duration={}ms, tok/s={:?}",
                        ttft_ms.unwrap_or(0),
                        duration_ms,
                        stats.tokens_per_sec
                    );

                    info!(
                        "Forwarding complete: {} actions, {} content bytes",
                        forwarded_count, total_content_len
                    );
                    if tx.send(Action::ResponseDone(Some(stats))).is_err() {
                        warn!("Failed to send ResponseDone: receiver dropped");
                    }
                    return;
                }
            }
        }

        // Fallback: channel closed without a Completed event
        info!(
            "Stream channel closed: {} actions, {} content bytes",
            forwarded_count, total_content_len
        );
        if tx.send(Action::ResponseDone(None)).is_err() {
            warn!("Failed to send ResponseDone: receiver dropped");
        }
    });

    vec![stream_handle.abort_handle(), forward_handle.abort_handle()]
}

/// Spawns a tool execution with a 30-second timeout.
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

/// Spawns concurrent model fetches from OpenRouter and LM Studio.
///
/// LM Studio has a 3s timeout so it won't block if the server isn't running.
/// Results are deduped against pinned models in `ModelPickerState::set_fetched_models()`.
pub fn spawn_model_fetch(app: &App, tx: mpsc::Sender<Action>) {
    let openrouter_base_url = app.openrouter_base_url.clone();
    let openrouter_api_key = app.openrouter_api_key.clone();
    let lmstudio_base_url = app.lmstudio_base_url.clone();

    tokio::spawn(async move {
        let (or_result, lms_result) = tokio::join!(
            async {
                match openrouter_api_key {
                    Some(ref key) => {
                        crate::inference::model_discovery::fetch_openrouter_models(
                            &openrouter_base_url,
                            key,
                        )
                        .await
                    }
                    None => {
                        warn!("No OpenRouter API key — skipping model fetch");
                        Ok(Vec::new())
                    }
                }
            },
            async {
                crate::inference::model_discovery::fetch_lmstudio_models(&lmstudio_base_url).await
            }
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

/// Spawns a background task to generate a session title after the first model response.
pub fn spawn_title_generation(app: &App, tx: mpsc::Sender<Action>) {
    let provider = app.provider.clone();
    let model_name = app.model_name.clone();
    let items = app.context.items.clone();
    info!("Spawning title generation task");

    tokio::spawn(async move {
        if let Some(t) = title::generate_title(provider, &model_name, &items).await {
            info!("Title generated: {}", t);
            if tx.send(Action::SessionTitleGenerated(t)).is_err() {
                warn!("Failed to send SessionTitleGenerated: receiver dropped");
            }
        }
    });
}

/// Spawns a background title regeneration for a session being switched away from.
///
/// Runs summarize→title async, then writes the result to disk via `rename_session`.
/// Non-blocking — the UI continues immediately.
pub fn spawn_title_regeneration_for_outgoing(app: &App) {
    let Some(session_id) = app.current_session_id.clone() else {
        return; // Unsaved session — nothing to rename
    };

    let provider = app.provider.clone();
    let model_name = app.model_name.clone();
    let items = app.context.items.clone();
    info!(
        "Spawning background title regeneration for outgoing session {}",
        session_id
    );

    tokio::spawn(async move {
        if let Some(t) = title::generate_title(provider, &model_name, &items).await {
            info!("Outgoing session {} title: {}", session_id, t);
            if let Err(e) = session::rename_session(&session_id, &t) {
                warn!("Failed to rename outgoing session {}: {}", session_id, e);
            }
        }
    });
}

/// Regenerates the session title while showing an animated exit screen.
///
/// Spawns summarize→title as a background task, then runs a draw loop showing
/// the logo animation until the task completes or times out.
pub fn regenerate_title_with_exit_animation(
    app: &mut App,
    terminal: &mut ratatui::DefaultTerminal,
) {
    let provider = app.provider.clone();
    let model_name = app.model_name.clone();
    let items = app.context.items.clone();
    info!("Regenerating title on quit (animated, summarize→title)");

    // Spawn summarize→title and get a oneshot receiver for the result
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = result_tx.send(title::generate_title(provider, &model_name, &items).await);
    });

    // Animate until the result arrives (or timeout after 15s for the two-step pipeline)
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(15);
    let mut frame_counter: usize = 0;
    let mut result_rx = Some(result_rx);

    loop {
        // Check if result is ready (non-blocking)
        if let Some(ref mut rx) = result_rx {
            match rx.try_recv() {
                Ok(title) => {
                    if let Some(title) = title {
                        info!("Exit-time title: {}", title);
                        app.session_title = title;
                    }
                    break;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => break,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {} // Still waiting
            }
        }

        // Timeout guard
        if start.elapsed() > timeout {
            warn!("Title generation timed out on exit");
            break;
        }

        // Draw frame
        draw_exit_screen(terminal, frame_counter);
        frame_counter += 1;

        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

/// Draws the animated exit screen: logo + message.
fn draw_exit_screen(terminal: &mut ratatui::DefaultTerminal, frame_index: usize) {
    use crate::tui::components::logo::Logo;
    use ratatui::layout::{Alignment, Constraint, Flex, Layout};
    use ratatui::style::{Color, Style};
    use ratatui::widgets::Paragraph;

    let _ = terminal.draw(|frame| {
        let area = frame.area();
        let logo_height = Logo::required_height().min(area.height.saturating_sub(4));

        let [_, logo_area, text_area, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(logo_height),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .split(area)[..] else {
            return;
        };

        Logo::render(frame, logo_area, frame_index);

        let [text_centered] = Layout::horizontal([Constraint::Length(40)])
            .flex(Flex::Center)
            .areas(text_area);

        let text = Paragraph::new("Tidying up your session...")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(text, text_centered);
    });
}
