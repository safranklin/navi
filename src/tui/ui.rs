//! # UI Layout & Rendering
//!
//! Top-level `draw_ui` function that composes all components into a frame.
//!
//! **Layout:** title bar (1 line) + main area (flex) + input box (3-7 lines).
//!
//! **Rendering order:** Main area renders first so `MessageList::render` can
//! update the layout cache before `hit_test_message` needs it. Then title bar,
//! input box, and finally overlays (session manager, model picker) on top.

use crate::core::state::App;
use crate::tui::TuiState;
use crate::tui::component::Component;
use crate::tui::components::{MessageList, ModelPicker, SessionManager, TitleBar};

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph};

pub fn draw_ui(frame: &mut Frame, app: &App, tui: &mut TuiState, spinner_frame: usize) {
    use Constraint::{Length, Min};

    // Calculate input height dynamically based on content
    let input_height = tui.input_box.calculate_height(frame.area().width);

    // Dynamic layout: title(1) + messages(flex) + input(3-7)
    let layout = Layout::vertical([Length(1), Min(0), Length(input_height)]);
    let [title_area, main_area, input_area] = layout.areas(frame.area());

    // 1. Render Main Area (MessageList or Error)
    // Rendered first so MessageList::render updates layout cache in TuiState.
    if let Some(error_msg) = &app.error {
        draw_error_view(frame, main_area, error_msg);
    } else if !app.context.has_visible_messages() {
        // Render Landing Page
        let mut landing = crate::tui::components::LandingPage::new(spinner_frame);
        landing.render(frame, main_area);
    } else {
        // Create MessageList wrapper around mutable persistent state
        let mut message_list = MessageList::new(
            &mut tui.message_list, // &mut MessageListState
            &app.context,
            app.is_loading,
            tui.pulse_value,
            spinner_frame,
            &app.message_stats,
        );
        // Mutable render call updates layout cache and renders to scroll view
        message_list.render(frame, main_area);
    }

    // 2. Render TitleBar
    let mut title_bar = TitleBar::new(
        &app.model_name,
        &app.provider_name,
        app.is_loading,
        spinner_frame,
        &app.session_title,
        app.session_total_tokens,
    );
    title_bar.render(frame, title_area);

    // 3. Render InputBox
    // InputBox state is persistent in TuiState
    tui.input_box.render(frame, input_area);

    // 4. Session manager overlay (on top of everything)
    if let Some(ref mut sm) = tui.session_manager {
        SessionManager::new(sm).render(frame, frame.area());
    }

    // 5. Model picker overlay (on top of everything, including session manager)
    if let Some(ref mut mp) = tui.model_picker {
        ModelPicker::new(mp, &app.model_name).render(frame, frame.area());
    }
}

fn draw_error_view(frame: &mut Frame, area: Rect, error_msg: &str) {
    let error_paragraph = Paragraph::new(error_msg)
        .block(Block::bordered().title("ERROR"))
        .alignment(Alignment::Center);

    frame.render_widget(error_paragraph, area);
}

/// Hit test: given a screen Y coordinate, find which message index (if any) is at that position.
/// Uses binary search on prefix_heights for O(log n) performance.
pub fn hit_test_message(
    screen_y: u16,
    frame_area: Rect,
    scroll_offset_y: u16,
    prefix_heights: &[u16],
    input_height: u16,
) -> Option<usize> {
    use Constraint::{Length, Min};

    // Calculate layout to find main_area
    // NOTE: This MUST match the layout in draw_ui
    let layout = Layout::vertical([Length(1), Min(0), Length(input_height)]);
    let [_title_area, main_area, _input_area] = layout.areas(frame_area);

    // Check if mouse is within the main content area
    if screen_y < main_area.y || screen_y >= main_area.y + main_area.height {
        return None;
    }

    // Convert screen Y to content Y (accounting for scroll)
    let content_y = (screen_y - main_area.y) + scroll_offset_y;

    // Binary search: find first segment whose end position is > content_y
    let idx = prefix_heights.partition_point(|&end_y| end_y <= content_y);

    if idx < prefix_heights.len() {
        Some(idx)
    } else {
        None // Below all content
    }
}
