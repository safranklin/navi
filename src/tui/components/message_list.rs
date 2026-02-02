//! # MessageList Component
//!
//! Scrollable view of conversation history.
//!
//! ## Responsibilities
//!
//! - Display list of messages
//! - Manage scrolling specific logic
//! - Hit testing for mouse interactions
//! - Perform efficient layout caching (Message heights)
//!
//! ## Architecture
//!
//! `MessageList` is a transient component (created each frame) that wraps
//! `&'a mut MessageListState` (persistent state) and `Context` (props).
//!
//! Since `Component::render` takes `&mut self`, we can safely mutate the state
//! (including layout cache and scroll state) during the render pass, aligning
//! with Ratatui's `StatefulWidget` pattern.

use ratatui::Frame;
use ratatui::layout::{Size, Rect};
use tui_scrollview::{ScrollView, ScrollbarVisibility, ScrollViewState};

use crate::inference::Context;
use crate::tui::component::{Component, EventHandler};
use crate::tui::components::message::Message;
use crate::tui::event::TuiEvent;

/// Layout and scroll state for the message list.
/// Must be persisted in the parent TuiState.
pub struct MessageListState {
    /// Scroll offset and view state
    pub scroll_state: ScrollViewState,
    /// Cached layout measurements
    pub layout: LayoutCache,
    /// When true, auto-scroll to bottom on new content
    pub stick_to_bottom: bool,
    /// Furthest scroll position reached (for "new content" indicator)
    pub max_scroll_reached: u16,
    /// Currently hovered message index (for visual feedback)
    pub hovered_index: Option<usize>,
}

impl Default for MessageListState {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageListState {
    pub fn new() -> Self {
        Self {
            scroll_state: ScrollViewState::default(),
            layout: LayoutCache::new(),
            stick_to_bottom: true, // Start attached to bottom
            max_scroll_reached: 0,
            hovered_index: None,
        }
    }
}

/// Scrollable conversation view component.
/// Created fresh each frame with references to state and data.
pub struct MessageList<'a> {
    // Mutable reference to persistent state
    pub state: &'a mut MessageListState,
    pub context: &'a Context,
    pub is_loading: bool,
}

impl<'a> MessageList<'a> {
    pub fn new(state: &'a mut MessageListState, context: &'a Context, is_loading: bool) -> Self {
        Self {
            state,
            context,
            is_loading,
        }
    }
}

impl<'a> Component for MessageList<'a> {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let content_width = area.width.saturating_sub(1); // -1 for scrollbar safe area
        let num_items = self.context.items.len();
        
        // 1. Update Layout Cache (Internal Mutation)
        // We can access self.state directly because we have &mut self 
        // and self.state is &mut MessageListState
        let layout = &mut self.state.layout;
        let reusable = layout.reusable_count(num_items, content_width, self.is_loading);
        
        layout.heights.truncate(reusable.min(layout.heights.len()));
        
        for seg in self.context.items.iter().skip(layout.heights.len()) {
            layout.heights.push(Message::calculate_height(seg, content_width));
        }
        layout.rebuild_prefix_heights();
        layout.update_metadata(num_items, content_width);

        let total_height: u16 = self.state.layout.heights.iter().sum();

         // 2. Calculate visible range
        let scroll_offset = self.state.scroll_state.offset().y;
        let visible_range = self.state.layout.visible_range(scroll_offset, area.height);

        // 3. Render visible segments
        // We render into a ScrollView widget.
        let mut scroll_view = ScrollView::new(Size::new(content_width, total_height))
            .vertical_scrollbar_visibility(ScrollbarVisibility::Always)
            .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

        let mut y_offset: u16 = if visible_range.start > 0 {
            self.state.layout.prefix_heights[visible_range.start - 1]
        } else {
            0
        };

        for i in visible_range.clone() {
            let seg = &self.context.items[i];
            let height = self.state.layout.heights[i];
            
            let is_last = i == num_items.saturating_sub(1);
            // Check hover status safely using state
            let is_hovered = self.state.hovered_index == Some(i) && !(is_last && self.is_loading);
            
            // Create the transient Message component logic
            let message = Message::new(seg, is_hovered);
            
            let segment_rect = Rect::new(0, y_offset, content_width, height);
            
            // Since Message implements Widget, we can render it directly into ScrollView
            scroll_view.render_widget(message, segment_rect);
            
            y_offset += height;
        }

        // Auto-scroll logic (Mutation)
        if self.state.stick_to_bottom {
            self.state.scroll_state.scroll_to_bottom();
        }

        // Render the ScrollView itself
        // This requires &mut scroll_state, which we can validly borrow from &mut self.state
        frame.render_stateful_widget(scroll_view, area, &mut self.state.scroll_state);
        
        // Update auxiliary state
        let current_offset = self.state.scroll_state.offset().y;
        self.state.max_scroll_reached = self.state.max_scroll_reached.max(current_offset);
    }
}

/// EventHandler is implemented on `MessageListState` rather than `MessageList` because:
/// 1. Event handling requires persistent state (scroll position, stick_to_bottom flag)
/// 2. `MessageList` is recreated each frame with fresh props, so it can't hold state
/// 3. The state object lives in `App` and persists across the event loop
impl EventHandler for MessageListState {
    type Event = (); // MessageList currently emits no events (scroll handled internally)

    fn handle_event(&mut self, event: &TuiEvent) -> Option<Self::Event> {
        match event {
            TuiEvent::ScrollUp => {
                self.scroll_state.scroll_up();
                self.stick_to_bottom = false;
                None
            }
            TuiEvent::ScrollDown => {
                self.scroll_state.scroll_down();
                None
            }
            TuiEvent::ScrollPageUp => {
                self.scroll_state.scroll_page_up();
                self.stick_to_bottom = false;
                None
            }
            TuiEvent::ScrollPageDown => {
                self.scroll_state.scroll_page_down();
                None
            }
            TuiEvent::ScrollToBottom => {
                self.scroll_state.scroll_to_bottom();
                self.stick_to_bottom = true;
                None
            }
            // Mouse moves handled by parent for now due to hit testing complexity
            _ => None,
        }
    }
}

/// Cached layout measurements
pub struct LayoutCache {
    pub heights: Vec<u16>,
    pub prefix_heights: Vec<u16>,
    message_count: usize,
    content_width: u16,
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            heights: Vec::new(),
            prefix_heights: Vec::new(),
            message_count: 0,
            content_width: 0,
        }
    }

    pub fn reusable_count(&self, message_count: usize, content_width: u16, is_loading: bool) -> usize {
        if self.content_width != content_width || self.heights.is_empty() || message_count < self.message_count {
            0
        } else if is_loading {
            if message_count == 0 { 0 } else { message_count - 1 }
        } else {
            message_count
        }
    }

    pub fn update_metadata(&mut self, message_count: usize, content_width: u16) {
        self.message_count = message_count;
        self.content_width = content_width;
    }

    pub fn rebuild_prefix_heights(&mut self) {
        self.prefix_heights = self.heights.iter().scan(0u16, |acc, &h| {
            *acc += h;
            Some(*acc)
        }).collect();
    }

    pub fn visible_range(&self, scroll_offset: u16, viewport_height: u16) -> std::ops::Range<usize> {
        let buffer = viewport_height / 2;
        let buffered_start = scroll_offset.saturating_sub(buffer);
        let buffered_end = scroll_offset.saturating_add(viewport_height).saturating_add(buffer);

        let start = self.prefix_heights.partition_point(|&end| end <= buffered_start);
        let end = self.prefix_heights.partition_point(|&end| end < buffered_end)
            .saturating_add(1)
            .min(self.prefix_heights.len());

        start..end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_cache_reusable() {
        let mut cache = LayoutCache::new();
        // Initial build
        cache.update_metadata(5, 80);
        cache.heights = vec![1; 5]; // Simulating 5 messages of height 1

        // Case 1: Same everything -> All reusable
        assert_eq!(cache.reusable_count(5, 80, false), 5);
        
        // Case 2: New message added -> 6 reusable (will be clamped by truncate)
        assert_eq!(cache.reusable_count(6, 80, false), 6);

        // Case 3: Width changed -> 0 reusable
        assert_eq!(cache.reusable_count(5, 40, false), 0);
        
        // Case 4: Loading (last message might change) -> n-1 reusable
        cache.update_metadata(5, 80);
        assert_eq!(cache.reusable_count(5, 80, true), 4);
    }
}
