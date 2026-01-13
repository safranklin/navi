//! # TUI Adapter
//! 
//! The ratatui-specific layer. Handles terminal I/O, renders the UI,
//! and translates keyboard events into core::Action values.
//!
//! This is the only module that knows about ratatui and crossterm.
//! The intention is to swap this out for a different adapter (React, web, etc.) in the future
//! if needed.

mod event;
mod ui;

use std::env;
// use tokio::runtime::Handle; // Removed
// use tokio::task::block_in_place; // Removed

// use crate::api::client::model_completion; // Removed
use crate::core::action::{Action, update, Effect};
use crate::core::state::App;
use crate::tui::event::poll_event;

use std::sync::mpsc;
use crate::api::client::stream_completion;
use crate::api::types::StreamChunk;

pub fn run() -> std::io::Result<()> {
    let mut app = App::new(env::var("PRIMARY_MODEL_NAME").expect("PRIMARY_MODEL_NAME must be set"));
    let mut terminal = ratatui::init();
    
    // Channel for actions from background tasks
    let (tx, rx) = mpsc::channel();

    loop {
        terminal.draw(|f| ui::draw_ui(f, &mut app))?;

        // Handle user input
        if let Some(action) = poll_event() {
            let effect = update(&mut app, action);
            match effect {
                Effect::Quit => break,
                Effect::SpawnRequest => {
                    spawn_request(&app, tx.clone());
                }
                _ => {}
            }
        }

        // Handle background task actions (streaming responses)
        while let Ok(action) = rx.try_recv() {
            let effect = update(&mut app, action);
             match effect {
                Effect::Quit => break, // Should not happen from background task usually
                Effect::SpawnRequest => {
                    spawn_request(&app, tx.clone()); // Chain request?
                }
                _ => {}
            }
        }
    }
    ratatui::restore();
    Ok(())
}

fn spawn_request(app: &App, tx: mpsc::Sender<Action>) {
    let context = app.context.clone();
    
    // Channel for the stream chunks (StreamChunk)
    let (str_tx, str_rx) = mpsc::channel();
    
    // Spawn async task to drive the stream
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = stream_completion(&context, str_tx).await {
            let _ = tx_clone.send(Action::ResponseChunk(format!("\n[Error: {}]", e)));
            let _ = tx_clone.send(Action::ResponseDone);
        }
    });
    
    // Spawn blocking task to forward chunks as Actions
    let tx_forward = tx.clone();
    tokio::task::spawn_blocking(move || {
        while let Ok(chunk) = str_rx.recv() {
            match chunk {
                StreamChunk::Content(c) => {
                    let _ = tx_forward.send(Action::ResponseChunk(c));
                }
                StreamChunk::Thinking(t) => {
                    let _ = tx_forward.send(Action::ThinkingChunk(t));
                }
            }
        }
        let _ = tx_forward.send(Action::ResponseDone);
    }); 
}