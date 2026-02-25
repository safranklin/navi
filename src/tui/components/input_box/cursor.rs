//! Cursor position tracking and navigation for the InputBox.
//!
//! `CursorState` owns the cursor byte offset, scroll offset, and cached width.
//! All navigation methods accept `buffer: &str` explicitly — the text data is
//! owned by `InputBox`, keeping the dependency visible.

use super::text_wrap::{
    BORDER_OFFSET, MAX_VISIBLE_LINES, inner_width, wrap_line_count, wrapped_line_byte_starts,
};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

/// Cursor and scroll state, separated from the text buffer.
pub(super) struct CursorState {
    /// Cursor position as byte offset in buffer (0..=buffer.len())
    pub pos: usize,
    /// Line offset for internal scrolling (0 when content fits in viewport)
    pub scroll_offset: u16,
    /// Cached content width from last render (used for cursor movement)
    pub last_content_width: u16,
}

/// Find which wrapped line a byte position falls on, given the line start offsets.
fn line_index_for_pos(starts: &[usize], pos: usize) -> usize {
    starts.iter().rposition(|&s| s <= pos).unwrap_or(0)
}

/// Content byte length of a wrapped line, derived from the starts vector.
///
/// The slice from `starts[i]` to `starts[i+1]` (or buffer end) includes any
/// separator chars (spaces/newlines) that textwrap consumed between lines.
/// Trimming those gives back the actual content length.
fn line_content_len(starts: &[usize], line_idx: usize, buffer: &str) -> usize {
    let start = starts[line_idx];
    let end = starts
        .get(line_idx + 1)
        .copied()
        .unwrap_or(buffer.len());
    buffer[start..end].trim_end_matches([' ', '\n']).len()
}

impl CursorState {
    const DEFAULT_WIDTH: u16 = 80;

    pub fn new() -> Self {
        Self {
            pos: 0,
            scroll_offset: 0,
            last_content_width: Self::DEFAULT_WIDTH,
        }
    }

    /// Reset cursor to start (used after Submit clears the buffer).
    pub fn reset(&mut self) {
        self.pos = 0;
        self.scroll_offset = 0;
    }

    /// Move cursor vertically (up or down) while trying to maintain column position.
    ///
    /// Returns `true` if cursor moved, `false` if already at boundary.
    pub fn move_vertically(&mut self, buffer: &str, direction: i16, content_width: u16) -> bool {
        let width = inner_width(content_width);
        if width == 0 || buffer.is_empty() {
            return false;
        }

        let starts = wrapped_line_byte_starts(buffer, width);

        let current = line_index_for_pos(&starts, self.pos);
        let column = self.pos - starts[current];

        let target = if direction < 0 {
            if current == 0 {
                return false;
            }
            current - 1
        } else {
            if current >= starts.len() - 1 {
                return false;
            }
            current + 1
        };

        let target_len = line_content_len(&starts, target, buffer);
        self.pos = starts[target] + column.min(target_len);

        true
    }

    /// Calculate which wrapped line (0-based) the cursor is on.
    pub fn calculate_line(&self, buffer: &str, content_width: u16) -> u16 {
        let width = inner_width(content_width);
        if width == 0 {
            return 0;
        }

        let starts = wrapped_line_byte_starts(buffer, width);
        line_index_for_pos(&starts, self.pos) as u16
    }

    /// Update scroll offset to keep cursor visible within the viewport.
    pub fn update_scroll_offset(&mut self, buffer: &str, content_width: u16) {
        let width = inner_width(content_width);
        let total_lines = wrap_line_count(buffer, width);

        if total_lines <= MAX_VISIBLE_LINES {
            self.scroll_offset = 0;
            return;
        }

        let cursor_line = self.calculate_line(buffer, content_width);

        if cursor_line < self.scroll_offset {
            self.scroll_offset = cursor_line;
        } else if cursor_line >= self.scroll_offset + MAX_VISIBLE_LINES {
            self.scroll_offset = cursor_line.saturating_sub(MAX_VISIBLE_LINES - 1);
        }
    }

    /// Calculate screen position for cursor based on wrapped text layout.
    /// Returns (column, row) in screen coordinates.
    pub fn screen_pos(&self, buffer: &str, area: Rect) -> (u16, u16) {
        let width = inner_width(area.width);
        if width == 0 {
            return (area.x + BORDER_OFFSET, area.y + BORDER_OFFSET);
        }

        let starts = wrapped_line_byte_starts(buffer, width);
        let line_idx = line_index_for_pos(&starts, self.pos);
        let line_start = starts[line_idx];

        let cursor_col = UnicodeWidthStr::width(&buffer[line_start..self.pos]) as u16;
        let visible_line = (line_idx as u16).saturating_sub(self.scroll_offset);

        (
            area.x + BORDER_OFFSET + cursor_col,
            area.y + BORDER_OFFSET + visible_line,
        )
    }
}
