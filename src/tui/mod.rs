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
use tokio::runtime::{Handle};
use tokio::task::block_in_place;

use crate::api::client::model_completion;
use crate::core::action::{Action, update};
use crate::core::state::App;
use crate::tui::event::poll_event;

pub fn run() -> std::io::Result<()> {
    let mut app = App::new(env::var("PRIMARY_MODEL_NAME").expect("PRIMARY_MODEL_NAME must be set"));
    let mut terminal = ratatui::init();
    
    loop {
        terminal.draw(|f| ui::draw_ui(f, &mut app))?;

        if let Some(action) = poll_event() {
            update(&mut app, action);
        }

        if app.should_quit {
            break;
        }

        if app.is_loading {
            let result = block_in_place(|| {
                Handle::current().block_on(model_completion(&app.context))
            });
            match result {
                Ok(response) => {
                    update(&mut app, Action::ResponseReceived(response));
                }
                Err(e) => {
                    app.error = Some(format!("API ERROR: {}\n\nPress Esc to quit", e));
                    app.is_loading = false;
                }
            }
        }
    }
    ratatui::restore();
    Ok(())
}