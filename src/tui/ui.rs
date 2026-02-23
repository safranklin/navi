use crate::core::state::App;
use crate::tui::TuiState;
use crate::tui::component::Component;
use crate::tui::components::{MessageList, TitleBar};

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph};

pub fn draw_ui(frame: &mut Frame, app: &App, tui: &mut TuiState, spinner_frame: usize) {
    use Constraint::{Length, Min};

    // Calculate input height dynamically based on content
    let input_height = tui.input_box.calculate_height(frame.area().width);

    // Dynamic layout (input height grows from 3 to 7 lines)
    let layout = Layout::vertical([Length(1), Min(0), Length(input_height)]);
    let [title_area, main_area, input_area] = layout.areas(frame.area());

    // 1. Render Main Area (MessageList or Error)
    // Rendered first so MessageList::render updates layout cache in TuiState.

    // Check if there are any user-visible messages (User or Model)
    let has_visible_messages = app.context.items.iter().any(|item|
        matches!(item, crate::inference::ContextItem::Message(seg) if matches!(seg.source, crate::inference::Source::User | crate::inference::Source::Model))
    );

    if let Some(error_msg) = &app.error {
        draw_error_view(frame, main_area, error_msg);
    } else if !has_visible_messages {
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
        );
        // Mutable render call updates layout cache and renders to scroll view
        message_list.render(frame, main_area);
    }

    // 2. Compute logic for TitleBar
    // Since MessageList::render has run, tui.message_list.layout is up-to-date.
    let has_unseen_content = {
        let state = &tui.message_list;
        let total_height: u16 = state.layout.heights.iter().sum();
        let visible_height = main_area.height;

        if total_height <= visible_height {
            false
        } else {
            let max_possible_scroll = total_height.saturating_sub(visible_height);
            state.max_scroll_reached < max_possible_scroll
        }
    };

    // 3. Render TitleBar
    let mut title_bar = TitleBar::new(
        app.model_name.clone(),
        app.status_message.clone(),
        has_unseen_content,
    );
    title_bar.render(frame, title_area);

    // 4. Render InputBox
    // InputBox state is persistent in TuiState
    tui.input_box.render(frame, input_area);
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
