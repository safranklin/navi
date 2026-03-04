//! # Modal Event Handlers
//!
//! Handler functions for modal overlays (session manager, model picker).
//! Each function takes an event + mutable state and returns whether the app should quit.
//!
//! Extracted from `run()` so the event loop stays a thin dispatcher.

use log::{info, warn};

use crate::core::action::{Action, Effect, update};
use crate::core::config::ResolvedConfig;
use crate::core::session;
use crate::core::state::App;
use crate::tui::TuiState;
use crate::tui::components::MessageListState;
use crate::tui::components::model_picker::ModelPickerEvent;
use crate::tui::components::session_manager::SessionEvent;
use crate::tui::tasks;

/// Handle a session manager event. Returns true if the app should quit.
pub fn handle_session_event(event: SessionEvent, app: &mut App, tui: &mut TuiState) -> bool {
    let mut should_quit = false;

    match event {
        SessionEvent::Load(id) => {
            // Save outgoing session and regenerate its title in the background
            session::save_current_session(app);
            tasks::spawn_title_regeneration_for_outgoing(app);

            match session::load_session(&id) {
                Ok(data) => {
                    let effect = update(app, Action::LoadSession(data));
                    if effect == Effect::Quit {
                        should_quit = true;
                    }
                    tui.message_list = MessageListState::new();
                    tui.title_generation_pending = false;
                }
                Err(e) => {
                    warn!("Failed to load session {}: {}", id, e);
                    app.status_message = format!("Load failed: {}", e);
                }
            }
            tui.session_manager = None;
        }
        SessionEvent::CreateNew => {
            // Save outgoing session and regenerate its title in the background
            session::save_current_session(app);
            tasks::spawn_title_regeneration_for_outgoing(app);

            let effect = update(app, Action::NewSession);
            if effect == Effect::Quit {
                should_quit = true;
            }
            // Assign "Session #N" title immediately
            app.session_title = format!("Session #{}", session::next_session_number());
            tui.message_list = MessageListState::new();
            tui.title_generation_pending = false;
            tui.session_manager = None;
        }
        SessionEvent::Rename { id, new_title } => {
            if let Err(e) = session::rename_session(&id, &new_title) {
                warn!("Failed to rename session {}: {}", id, e);
            }
            // Update active session title if renaming the current one
            if app.current_session_id.as_deref() == Some(&id) {
                app.session_title = new_title;
            }
        }
        SessionEvent::Delete(id) => {
            if let Err(e) = session::delete_session(&id) {
                warn!("Failed to delete session {}: {}", id, e);
            }
            if let Some(ref mut sm) = tui.session_manager {
                sm.remove_session(&id);
            }
            // If we deleted the active session, clear the ID
            if app.current_session_id.as_deref() == Some(&id) {
                app.current_session_id = None;
            }
        }
        SessionEvent::Dismiss => {
            tui.session_manager = None;
        }
    }

    should_quit
}

/// Handle a model picker event.
pub fn handle_model_picker_event(
    event: ModelPickerEvent,
    app: &mut App,
    tui: &mut TuiState,
    config: &ResolvedConfig,
) {
    match event {
        ModelPickerEvent::Select(entry) => {
            // Build a new resolved config with the selected model/provider
            let mut new_config = config.clone();
            new_config.provider = entry.provider.clone();
            new_config.model_name = entry.name.clone();

            // Rebuild the provider for the new model
            app.provider = crate::inference::build_provider(&new_config);
            app.model_name = entry.name.clone();
            app.provider_name = entry.provider.clone();
            app.status_message = format!("Switched to {} ({})", entry.name, entry.provider);
            info!("Model switched: {} ({})", entry.name, entry.provider);
            tui.model_picker = None;
        }
        ModelPickerEvent::Dismiss => {
            tui.model_picker = None;
        }
    }
}
