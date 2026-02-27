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

use std::collections::{HashMap, HashSet};

use ratatui::Frame;
use ratatui::layout::{Position, Rect, Size};
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};

use crate::inference::{Context, ContextItem, Source};
use crate::tui::component::{Component, EventHandler};
use crate::tui::components::logo::Logo;
use crate::tui::components::message::Message;
use crate::tui::components::tool_message::ToolGroup;
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
    /// Currently selected message index (hover or keyboard navigation)
    pub selected_index: Option<usize>,
    /// Tool call indices that are currently expanded (toggled by click or Space)
    pub expanded_indices: HashSet<usize>,
    /// Last known viewport height (for scroll clamping between frames)
    pub viewport_height: u16,
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
            selected_index: None,
            expanded_indices: HashSet::new(),
            viewport_height: 0,
        }
    }

    /// Clamp scroll offset so it never exceeds the content bounds.
    /// Prevents overscrolling past the last message.
    pub fn clamp_scroll(&mut self) {
        let total_content_height: u16 = self.layout.heights.iter().sum();
        let max_y = total_content_height.saturating_sub(self.viewport_height);
        let current = self.scroll_state.offset();
        if current.y > max_y {
            self.scroll_state.set_offset(Position {
                x: current.x,
                y: max_y,
            });
        }
    }

    /// Scroll the viewport so the selected message is fully visible.
    /// If the message is taller than the viewport, align its top edge.
    pub fn scroll_to_selected(&mut self) {
        let Some(idx) = self.selected_index else {
            return;
        };
        if idx >= self.layout.prefix_heights.len() {
            return;
        }

        let item_top = if idx == 0 {
            0
        } else {
            self.layout.prefix_heights[idx - 1]
        };
        let item_bottom = self.layout.prefix_heights[idx];
        let offset_y = self.scroll_state.offset().y;

        if item_top < offset_y {
            // Selected message is above viewport — scroll up to show its top
            self.scroll_state
                .set_offset(Position { x: 0, y: item_top });
            self.stick_to_bottom = false;
        } else if item_bottom > offset_y + self.viewport_height {
            // Selected message is below viewport — scroll down to show its bottom
            let new_y = item_bottom.saturating_sub(self.viewport_height);
            self.scroll_state
                .set_offset(Position { x: 0, y: new_y });
            // Re-pin if we've landed at the absolute bottom
            let total: u16 = self.layout.heights.iter().sum();
            let max_y = total.saturating_sub(self.viewport_height);
            self.stick_to_bottom = new_y >= max_y;
        }
    }

    /// Clamp scroll and re-engage auto-scroll if the user has reached the bottom.
    /// Called on scroll-down events so that scrolling past the end re-pins to bottom.
    pub fn repin_if_at_bottom(&mut self) {
        let total_content_height: u16 = self.layout.heights.iter().sum();
        let max_y = total_content_height.saturating_sub(self.viewport_height);
        let current = self.scroll_state.offset();
        if current.y >= max_y {
            self.stick_to_bottom = true;
            self.scroll_state.set_offset(Position {
                x: current.x,
                y: max_y,
            });
        }
    }
}

/// Build a lookup from call_id → (index, &ToolResult) for all ToolResult items,
/// plus the set of consumed ToolResult indices (those whose call_id matches a ToolCall).
fn build_result_map(items: &[ContextItem]) -> (HashMap<&str, &crate::inference::ToolResult>, HashSet<usize>) {
    let mut result_map: HashMap<&str, &crate::inference::ToolResult> = HashMap::new();
    let mut result_indices: HashMap<&str, usize> = HashMap::new();

    for (i, item) in items.iter().enumerate() {
        if let ContextItem::ToolResult(tr) = item {
            result_map.insert(&tr.call_id, tr);
            result_indices.insert(&tr.call_id, i);
        }
    }

    // A ToolResult is "consumed" if there's a ToolCall with a matching call_id
    let mut consumed = HashSet::new();
    for item in items {
        if let ContextItem::ToolCall(tc) = item
            && let Some(&idx) = result_indices.get(tc.call_id.as_str())
        {
            consumed.insert(idx);
        }
    }

    (result_map, consumed)
}

/// Scrollable conversation view component.
/// Created fresh each frame with references to state and data.
pub struct MessageList<'a> {
    // Mutable reference to persistent state
    pub state: &'a mut MessageListState,
    pub context: &'a Context,
    pub is_loading: bool,
    pub pulse_value: f32,
    pub spinner_frame: usize,
}

impl<'a> MessageList<'a> {
    pub fn new(
        state: &'a mut MessageListState,
        context: &'a Context,
        is_loading: bool,
        pulse_value: f32,
        spinner_frame: usize,
    ) -> Self {
        Self {
            state,
            context,
            is_loading,
            pulse_value,
            spinner_frame,
        }
    }
}

impl<'a> Component for MessageList<'a> {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let content_width = area.width.saturating_sub(1); // -1 for scrollbar safe area
        let num_items = self.context.items.len();

        // Build call_id → &ToolResult lookup and consumed index set
        let (result_map, consumed) = build_result_map(&self.context.items);

        // 1. Update Layout Cache (Internal Mutation)
        let expanded_indices = &self.state.expanded_indices;
        let layout = &mut self.state.layout;
        let reusable = layout.reusable_count(
            num_items,
            content_width,
            self.is_loading,
            &self.context.items,
            expanded_indices,
        );

        layout.heights.truncate(reusable.min(layout.heights.len()));

        for (i, item) in self
            .context
            .items
            .iter()
            .enumerate()
            .skip(layout.heights.len())
        {
            let is_expanded = expanded_indices.contains(&i);
            let height = match item {
                ContextItem::Message(seg) => {
                    Message::calculate_height(seg, content_width)
                }
                ContextItem::ToolCall(tc) => {
                    let paired_result = result_map.get(tc.call_id.as_str()).copied();
                    ToolGroup::calculate_height(tc, paired_result, is_expanded, content_width)
                }
                ContextItem::ToolResult(_) if consumed.contains(&i) => 0,
                ContextItem::ToolResult(_) => 0, // Defensive: orphaned results hidden too
            };
            layout.heights.push(height);
        }
        layout.rebuild_prefix_heights();
        layout.update_metadata(num_items, content_width, expanded_indices);

        let total_height: u16 = self.state.layout.heights.iter().sum();

        // Show loading indicator for the entire duration of model response
        let show_spinner = self.is_loading && self.state.stick_to_bottom;

        // When loading, add bottom padding to the canvas so scroll_to_bottom
        // pushes messages up, leaving screen space for the logo overlay.
        // The ScrollView always fills the full viewport — no area reduction.
        let logo_padding = if show_spinner {
            5u16.min(area.height / 2)
        } else {
            0
        };
        let canvas_height = total_height + logo_padding;

        // 2. Clamp scroll offset to prevent overscrolling past content.
        // Skip when auto-scrolling: scroll_to_bottom targets canvas_height
        // (which includes logo padding), while clamp uses content height only.
        self.state.viewport_height = area.height;
        if !self.state.stick_to_bottom {
            self.state.clamp_scroll();
        }

        let scroll_offset = self.state.scroll_state.offset().y;
        let visible_range = self.state.layout.visible_range(scroll_offset, area.height);

        // 3. Render visible segments into a ScrollView
        // Canvas includes logo padding so scroll_to_bottom leaves room for the overlay.
        let mut scroll_view = ScrollView::new(Size::new(content_width, canvas_height))
            .vertical_scrollbar_visibility(ScrollbarVisibility::Always)
            .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

        let mut y_offset: u16 = if visible_range.start > 0 {
            self.state.layout.prefix_heights[visible_range.start - 1]
        } else {
            0
        };

        for i in visible_range.clone() {
            let item = &self.context.items[i];
            let height = self.state.layout.heights[i];

            // Skip consumed ToolResults (height=0, no visual space)
            if height == 0 {
                continue;
            }

            let is_last = i == num_items.saturating_sub(1);
            let is_selected = self.state.selected_index == Some(i) && !(is_last && self.is_loading);

            let segment_rect = Rect::new(0, y_offset, content_width, height);

            match item {
                ContextItem::Message(seg) => {
                    let is_volatile = matches!(
                        seg.source,
                        Source::Model | Source::Thinking | Source::Status
                    );
                    let pulse_intensity = if is_last && self.is_loading && is_volatile {
                        self.pulse_value
                    } else {
                        0.0
                    };
                    let message = Message::new(seg, is_selected, pulse_intensity);
                    scroll_view.render_widget(message, segment_rect);
                }
                ContextItem::ToolCall(tc) => {
                    let paired_result = result_map.get(tc.call_id.as_str()).copied();
                    let group = ToolGroup {
                        call: tc,
                        result: paired_result,
                        is_selected,
                        is_expanded: self.state.expanded_indices.contains(&i),
                        spinner_frame: self.spinner_frame,
                    };
                    scroll_view.render_widget(group, segment_rect);
                }
                ContextItem::ToolResult(_) => {
                    // Should not reach here (height=0 items skipped above)
                }
            }

            y_offset += height;
        }

        // Auto-scroll logic (Mutation)
        if self.state.stick_to_bottom {
            self.state.scroll_state.scroll_to_bottom();
        }

        // Render the ScrollView into the full viewport area
        frame.render_stateful_widget(scroll_view, area, &mut self.state.scroll_state);

        // 4. Render animated logo centered in empty space below messages
        if show_spinner {
            // Calculate where messages end on screen (accounting for scroll)
            let content_screen_end = total_height.saturating_sub(scroll_offset);
            let logo_start = area.y + content_screen_end.min(area.height);
            let bottom_pad: u16 = 2;
            let logo_h = (area.y + area.height)
                .saturating_sub(logo_start)
                .saturating_sub(bottom_pad);
            if logo_h >= 3 {
                let logo_area = Rect::new(area.x, logo_start, area.width, logo_h);
                Logo::render(frame, logo_area, self.spinner_frame);
            }
        }

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
                self.repin_if_at_bottom();
                None
            }
            TuiEvent::ScrollPageUp => {
                self.scroll_state.scroll_page_up();
                self.stick_to_bottom = false;
                None
            }
            TuiEvent::ScrollPageDown => {
                self.scroll_state.scroll_page_down();
                self.repin_if_at_bottom();
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
    /// Tracks which tool calls are expanded so heights are invalidated on toggle.
    cached_expanded_indices: HashSet<usize>,
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
            cached_expanded_indices: HashSet::new(),
        }
    }

    pub fn reusable_count(
        &self,
        message_count: usize,
        content_width: u16,
        is_loading: bool,
        items: &[ContextItem],
        expanded_indices: &HashSet<usize>,
    ) -> usize {
        if self.content_width != content_width || self.heights.is_empty() {
            return 0;
        }

        // If we have FEWER messages than cached, we must have cleared context -> invalid
        if message_count < self.message_count {
            return 0;
        }

        // Expansion state changed → height changes for affected ToolCalls.
        // Invalidate from the earliest toggled index onward.
        if expanded_indices != &self.cached_expanded_indices
            && let Some(earliest) = expanded_indices
                .symmetric_difference(&self.cached_expanded_indices)
                .copied()
                .min()
            && earliest < items.len()
        {
            return earliest;
        }

        // If not loading, most heights are stable — but the last Model/Thinking
        // message may have grown during a streaming batch that completed between
        // frames (loading flipped false before we recalculated its height).
        if !is_loading {
            let last_is_volatile = items.last().is_some_and(|last| match last {
                ContextItem::Message(seg) => {
                    matches!(seg.source, Source::Model | Source::Thinking)
                }
                _ => false,
            });
            return if last_is_volatile {
                message_count.saturating_sub(1)
            } else {
                message_count
            };
        }

        // If loading, check if the last item is volatile.
        // Tool calls/results are stable once written. Messages from Model/Thinking are volatile.
        let last_is_stable = items.last().is_some_and(|last| match last {
            ContextItem::Message(seg) => {
                matches!(seg.source, Source::User | Source::Directive)
            }
            ContextItem::ToolCall(_)
            | ContextItem::ToolResult(_) => true,
        });

        if last_is_stable {
            message_count
        } else {
            message_count.saturating_sub(1)
        }
    }

    pub fn update_metadata(&mut self, message_count: usize, content_width: u16, expanded_indices: &HashSet<usize>) {
        self.message_count = message_count;
        self.content_width = content_width;
        self.cached_expanded_indices = expanded_indices.clone();
    }

    pub fn rebuild_prefix_heights(&mut self) {
        self.prefix_heights = self
            .heights
            .iter()
            .scan(0u16, |acc, &h| {
                *acc += h;
                Some(*acc)
            })
            .collect();
    }

    pub fn visible_range(
        &self,
        scroll_offset: u16,
        viewport_height: u16,
    ) -> std::ops::Range<usize> {
        let buffer = viewport_height / 2;
        let buffered_start = scroll_offset.saturating_sub(buffer);
        let buffered_end = scroll_offset
            .saturating_add(viewport_height)
            .saturating_add(buffer);

        let start = self
            .prefix_heights
            .partition_point(|&end| end <= buffered_start);
        let end = self
            .prefix_heights
            .partition_point(|&end| end < buffered_end)
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
        let no_expanded = HashSet::new();
        // Initial build
        cache.update_metadata(5, 80, &no_expanded);
        cache.heights = vec![1; 5]; // Simulating 5 messages of height 1

        // Case 1: Same everything -> All reusable
        assert_eq!(cache.reusable_count(5, 80, false, &[], &no_expanded), 5);

        // Case 2: New message added -> 6 reusable (will be clamped by truncate)
        assert_eq!(cache.reusable_count(6, 80, false, &[], &no_expanded), 6);

        // Case 3: Width changed -> 0 reusable
        assert_eq!(cache.reusable_count(5, 40, false, &[], &no_expanded), 0);

        // Case 4: Loading (last message is volatile) -> n-1 reusable
        let volatile_items = vec![crate::inference::ContextItem::Message(
            crate::inference::ContextSegment {
                source: Source::Model,
                content: String::new(),
            },
        )];
        cache.update_metadata(1, 80, &no_expanded);
        assert_eq!(
            cache.reusable_count(1, 80, true, &volatile_items, &no_expanded),
            0
        );

        // Case 5: Loading (last message is stable) -> n reusable
        let stable_items = vec![crate::inference::ContextItem::Message(
            crate::inference::ContextSegment {
                source: Source::User,
                content: String::new(),
            },
        )];
        cache.update_metadata(1, 80, &no_expanded);
        assert_eq!(
            cache.reusable_count(1, 80, true, &stable_items, &no_expanded),
            1
        );
    }

    #[test]
    fn test_volatile_last_message_recalculated_after_loading() {
        let mut cache = LayoutCache::new();
        let no_expanded = HashSet::new();
        cache.heights = vec![3, 5];
        cache.update_metadata(2, 80, &no_expanded);

        // Streaming just finished: is_loading=false, but last message is Model.
        // Its cached height may be stale from a partial streaming frame.
        let items = vec![
            crate::inference::ContextItem::Message(crate::inference::ContextSegment {
                source: Source::User,
                content: "hello".into(),
            }),
            crate::inference::ContextItem::Message(crate::inference::ContextSegment {
                source: Source::Model,
                content: "full response".into(),
            }),
        ];

        // Should exclude the volatile last item so its height gets recalculated
        assert_eq!(
            cache.reusable_count(2, 80, false, &items, &no_expanded),
            1
        );

        // Non-volatile last item (User) should trust the cache fully
        let stable_items = vec![
            crate::inference::ContextItem::Message(crate::inference::ContextSegment {
                source: Source::Model,
                content: "response".into(),
            }),
            crate::inference::ContextItem::Message(crate::inference::ContextSegment {
                source: Source::User,
                content: "follow-up".into(),
            }),
        ];
        assert_eq!(
            cache.reusable_count(2, 80, false, &stable_items, &no_expanded),
            2
        );
    }

    /// Replays the exact sequence that triggers the stale-height bug:
    /// 1. Cache built mid-stream with a short Model message (height 3)
    /// 2. All remaining chunks arrive in one batch — loading flips to false
    /// 3. The render loop must recalculate the Model message's height
    ///    and get the taller (correct) value
    #[test]
    fn test_loading_transition_replaces_stale_height() {
        use crate::inference::{ContextItem, ContextSegment};
        use crate::tui::components::message::Message;

        let width: u16 = 30;
        let no_expanded = HashSet::new();

        let user_seg = ContextSegment {
            source: Source::User,
            content: "hi".into(),
        };
        let partial_model = ContextSegment {
            source: Source::Model,
            content: "short".into(),
        };

        // --- Frame 1: mid-stream, cache the partial model message ---
        let items_streaming: Vec<ContextItem> = vec![
            ContextItem::Message(user_seg.clone()),
            ContextItem::Message(partial_model.clone()),
        ];

        let mut cache = LayoutCache::new();
        let reusable = cache.reusable_count(2, width, true, &items_streaming, &no_expanded);
        cache.heights.truncate(reusable); // 0 — fresh cache

        for item in &items_streaming {
            let h = match item {
                ContextItem::Message(seg) => Message::calculate_height(seg, width),
                _ => unreachable!(),
            };
            cache.heights.push(h);
        }
        cache.rebuild_prefix_heights();
        cache.update_metadata(2, width, &no_expanded);

        let stale_model_height = cache.heights[1];

        // --- Frame 2: streaming done, full response landed in same batch ---
        let full_model = ContextSegment {
            source: Source::Model,
            content: "this response is long enough to wrap across multiple lines at width 30"
                .into(),
        };
        let items_done: Vec<ContextItem> = vec![
            ContextItem::Message(user_seg.clone()),
            ContextItem::Message(full_model.clone()),
        ];

        // is_loading = false now. The fix should exclude the last volatile item.
        let reusable = cache.reusable_count(2, width, false, &items_done, &no_expanded);
        assert_eq!(
            reusable, 1,
            "should force recalculation of the volatile last message"
        );
        cache.heights.truncate(reusable);

        // Recalculate from reusable onward (index 1)
        for item in items_done.iter().skip(cache.heights.len()) {
            let h = match item {
                ContextItem::Message(seg) => Message::calculate_height(seg, width),
                _ => unreachable!(),
            };
            cache.heights.push(h);
        }
        cache.rebuild_prefix_heights();
        cache.update_metadata(2, width, &no_expanded);

        let fresh_model_height = cache.heights[1];

        // The recalculated height must be taller than the stale one
        assert!(
            fresh_model_height > stale_model_height,
            "fresh height ({fresh_model_height}) should exceed stale height ({stale_model_height})"
        );

        // User message height must be unchanged (was reusable)
        let expected_user_height = Message::calculate_height(&user_seg, width);
        assert_eq!(cache.heights[0], expected_user_height);
    }

    #[test]
    fn test_expansion_toggle_invalidates_cache() {
        let mut cache = LayoutCache::new();
        let no_expanded = HashSet::new();
        cache.heights = vec![3, 3, 3];
        cache.update_metadata(3, 80, &no_expanded);

        let items = vec![
            crate::inference::ContextItem::Message(crate::inference::ContextSegment {
                source: Source::User,
                content: "hello".into(),
            }),
            crate::inference::ContextItem::ToolCall(crate::inference::ToolCall {
                id: "fc_1".into(),
                call_id: "call_1".into(),
                name: "add".into(),
                arguments: "{}".into(),
            }),
            crate::inference::ContextItem::Message(crate::inference::ContextSegment {
                source: Source::Model,
                content: "done".into(),
            }),
        ];

        // Expanding index 1 should invalidate from that index onward
        let mut expanded = HashSet::new();
        expanded.insert(1);
        assert_eq!(
            cache.reusable_count(3, 80, false, &items, &expanded),
            1 // Only index 0 reusable; 1 and 2 need recalc
        );
    }
}
