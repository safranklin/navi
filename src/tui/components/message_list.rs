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
use ratatui::layout::{Position, Rect, Size};
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};

use crate::inference::{Context, Source};
use crate::tui::component::{Component, EventHandler};
use crate::tui::components::message::Message;
use crate::tui::components::tool_message::{ToolMessage, ToolMessageKind};
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
}

/// Finds the tool name for a result by scanning backwards for the matching ToolCall.
/// Free function to avoid borrowing `self` (which conflicts with mutable layout borrows).
fn find_tool_name_for_result<'a>(
    items: &'a [crate::inference::ContextItem],
    call_id: &str,
    before: usize,
) -> &'a str {
    for item in items[..before].iter().rev() {
        if let crate::inference::ContextItem::ToolCall(tc) = item
            && tc.call_id == call_id
        {
            return &tc.name;
        }
    }
    "unknown"
}

/// Scrollable conversation view component.
/// Created fresh each frame with references to state and data.
pub struct MessageList<'a> {
    // Mutable reference to persistent state
    pub state: &'a mut MessageListState,
    pub context: &'a Context,
    pub is_loading: bool,
    pub pulse_value: f32,
}

impl<'a> MessageList<'a> {
    pub fn new(
        state: &'a mut MessageListState,
        context: &'a Context,
        is_loading: bool,
        pulse_value: f32,
    ) -> Self {
        Self {
            state,
            context,
            is_loading,
            pulse_value,
        }
    }

    /// Build the ghost "preparing" segment shown while waiting for first token.
    /// Dot count breathes with pulse_value (sine wave): . → .. → ... → .. → .
    fn ghost_segment(&self) -> crate::inference::ContextSegment {
        let dots = match self.pulse_value {
            v if v > 0.66 => "...",
            v if v > 0.33 => "..",
            _ => ".",
        };
        crate::inference::ContextSegment {
            source: Source::Status,
            content: format!("Preparing{dots}"),
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
        let reusable = layout.reusable_count(
            num_items,
            content_width,
            self.is_loading,
            &self.context.items,
        );

        layout.heights.truncate(reusable.min(layout.heights.len()));

        for (i, item) in self
            .context
            .items
            .iter()
            .enumerate()
            .skip(layout.heights.len())
        {
            let height = match item {
                crate::inference::ContextItem::Message(seg) => {
                    Message::calculate_height(seg, content_width)
                }
                crate::inference::ContextItem::ToolCall(tc) => {
                    ToolMessage::new(ToolMessageKind::Call(tc)).calculate_height(content_width)
                }
                crate::inference::ContextItem::ToolResult(tr) => {
                    let name = find_tool_name_for_result(&self.context.items, &tr.call_id, i);
                    ToolMessage::new(ToolMessageKind::Result(tr, name))
                        .calculate_height(content_width)
                }
            };
            layout.heights.push(height);
        }
        layout.rebuild_prefix_heights();
        layout.update_metadata(num_items, content_width);

        let mut total_height: u16 = self.state.layout.heights.iter().sum();

        // Pre-compute ghost loader state (used for both total_height and rendering)
        let ghost = if self.is_loading && self.context.items.last().is_some_and(|last| {
            matches!(last, crate::inference::ContextItem::Message(seg) if matches!(seg.source, Source::User | Source::Directive))
        }) {
            let seg = self.ghost_segment();
            let height = Message::calculate_height(&seg, content_width);
            total_height += height;
            Some((seg, height))
        } else {
            None
        };

        // 2. Clamp scroll offset to prevent overscrolling past content
        self.state.viewport_height = area.height;
        self.state.clamp_scroll();

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
            let item = &self.context.items[i];
            let height = self.state.layout.heights[i];

            let is_last = i == num_items.saturating_sub(1);
            let is_selected =
                self.state.selected_index == Some(i) && !(is_last && self.is_loading);

            let segment_rect = Rect::new(0, y_offset, content_width, height);

            match item {
                crate::inference::ContextItem::Message(seg) => {
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
                crate::inference::ContextItem::ToolCall(tc) => {
                    let tool_msg = ToolMessage::new(ToolMessageKind::Call(tc));
                    scroll_view.render_widget(tool_msg, segment_rect);
                }
                crate::inference::ContextItem::ToolResult(tr) => {
                    let name = find_tool_name_for_result(&self.context.items, &tr.call_id, i);
                    let tool_msg = ToolMessage::new(ToolMessageKind::Result(tr, name));
                    scroll_view.render_widget(tool_msg, segment_rect);
                }
            }

            y_offset += height;
        }

        // 4. Render Ghost "Thinking..." Indicator if needed
        if let Some((ghost_seg, ghost_height)) = &ghost {
            let viewport_bottom = scroll_offset + area.height;
            if y_offset < viewport_bottom {
                let message = Message::new(ghost_seg, false, self.pulse_value);
                let segment_rect = Rect::new(0, y_offset, content_width, *ghost_height);
                scroll_view.render_widget(message, segment_rect);
            }
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
                self.clamp_scroll();
                None
            }
            TuiEvent::ScrollPageUp => {
                self.scroll_state.scroll_page_up();
                self.stick_to_bottom = false;
                None
            }
            TuiEvent::ScrollPageDown => {
                self.scroll_state.scroll_page_down();
                self.clamp_scroll();
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

    pub fn reusable_count(
        &self,
        message_count: usize,
        content_width: u16,
        is_loading: bool,
        items: &[crate::inference::ContextItem],
    ) -> usize {
        if self.content_width != content_width || self.heights.is_empty() {
            return 0;
        }

        // If we have FEWER messages than cached, we must have cleared context -> invalid
        if message_count < self.message_count {
            return 0;
        }

        // If not loading, most heights are stable — but the last Model/Thinking
        // message may have grown during a streaming batch that completed between
        // frames (loading flipped false before we recalculated its height).
        if !is_loading {
            let last_is_volatile = items.last().is_some_and(|last| match last {
                crate::inference::ContextItem::Message(seg) => {
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
            crate::inference::ContextItem::Message(seg) => {
                matches!(seg.source, Source::User | Source::Directive)
            }
            crate::inference::ContextItem::ToolCall(_)
            | crate::inference::ContextItem::ToolResult(_) => true,
        });

        if last_is_stable {
            message_count
        } else {
            message_count.saturating_sub(1)
        }
    }

    pub fn update_metadata(&mut self, message_count: usize, content_width: u16) {
        self.message_count = message_count;
        self.content_width = content_width;
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
        // Initial build
        cache.update_metadata(5, 80);
        cache.heights = vec![1; 5]; // Simulating 5 messages of height 1

        // Case 1: Same everything -> All reusable
        assert_eq!(cache.reusable_count(5, 80, false, &[]), 5);

        // Case 2: New message added -> 6 reusable (will be clamped by truncate)
        assert_eq!(cache.reusable_count(6, 80, false, &[]), 6);

        // Case 3: Width changed -> 0 reusable
        assert_eq!(cache.reusable_count(5, 40, false, &[]), 0);

        // Case 4: Loading (last message is volatile) -> n-1 reusable
        let volatile_items = vec![crate::inference::ContextItem::Message(
            crate::inference::ContextSegment {
                source: Source::Model,
                content: String::new(),
            },
        )];
        cache.update_metadata(1, 80);
        assert_eq!(cache.reusable_count(1, 80, true, &volatile_items), 0);

        // Case 5: Loading (last message is stable) -> n reusable
        let stable_items = vec![crate::inference::ContextItem::Message(
            crate::inference::ContextSegment {
                source: Source::User,
                content: String::new(),
            },
        )];
        cache.update_metadata(1, 80);
        assert_eq!(cache.reusable_count(1, 80, true, &stable_items), 1);
    }

    #[test]
    fn test_volatile_last_message_recalculated_after_loading() {
        let mut cache = LayoutCache::new();
        cache.heights = vec![3, 5];
        cache.update_metadata(2, 80);

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
        assert_eq!(cache.reusable_count(2, 80, false, &items), 1);

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
        assert_eq!(cache.reusable_count(2, 80, false, &stable_items), 2);
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
        let reusable = cache.reusable_count(2, width, true, &items_streaming);
        cache.heights.truncate(reusable); // 0 — fresh cache

        for item in &items_streaming {
            let h = match item {
                ContextItem::Message(seg) => Message::calculate_height(seg, width),
                _ => unreachable!(),
            };
            cache.heights.push(h);
        }
        cache.rebuild_prefix_heights();
        cache.update_metadata(2, width);

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
        let reusable = cache.reusable_count(2, width, false, &items_done);
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
        cache.update_metadata(2, width);

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
}
