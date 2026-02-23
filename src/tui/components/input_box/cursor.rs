//! Cursor position tracking and navigation for the InputBox.
//!
//! `CursorState` owns the cursor byte offset, scroll offset, and cached width.
//! All navigation methods accept `buffer: &str` explicitly — the text data is
//! owned by `InputBox`, keeping the dependency visible.

use super::text_wrap::{
    BORDER_OFFSET, MAX_VISIBLE_LINES, inner_width, wrap_line_count, wrap_options,
};
use ratatui::layout::Rect;

/// Cursor and scroll state, separated from the text buffer.
pub(super) struct CursorState {
    /// Cursor position as byte offset in buffer (0..=buffer.len())
    pub pos: usize,
    /// Line offset for internal scrolling (0 when content fits in viewport)
    pub scroll_offset: u16,
    /// Cached content width from last render (used for cursor movement)
    pub last_content_width: u16,
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

        let lines = textwrap::wrap(buffer, wrap_options(width));
        if lines.is_empty() {
            return false;
        }

        // Calculate byte length of a wrapped line including its trailing newline (if present)
        let line_byte_span = |line: &str, offset: usize| -> usize {
            let has_newline = offset + line.len() < buffer.len()
                && buffer.as_bytes()[offset + line.len()] == b'\n';
            line.len() + usize::from(has_newline)
        };

        // Find which wrapped line the cursor is on and its column offset
        let mut byte_offset = 0;
        let mut current_line_idx = 0;
        let mut column_in_line = 0;

        for (idx, line) in lines.iter().enumerate() {
            if byte_offset + line.len() >= self.pos {
                current_line_idx = idx;
                column_in_line = self.pos - byte_offset;
                break;
            }
            byte_offset += line_byte_span(line, byte_offset);
        }

        // Calculate target line index, returning false if at boundary
        let target_line_idx = if direction < 0 {
            if current_line_idx == 0 {
                return false;
            }
            current_line_idx - 1
        } else {
            if current_line_idx >= lines.len() - 1 {
                return false;
            }
            current_line_idx + 1
        };

        // Walk forward to find byte offset of the target line
        let mut target_line_start = 0;
        for line in lines.iter().take(target_line_idx) {
            target_line_start += line_byte_span(line, target_line_start);
        }

        // Place cursor at the same column, clamped to the target line's length
        let target_column = column_in_line.min(lines[target_line_idx].len());
        self.pos = target_line_start + target_column;

        true
    }

    /// Calculate which wrapped line (0-based) the cursor is on.
    pub fn calculate_line(&self, buffer: &str, content_width: u16) -> u16 {
        let width = inner_width(content_width);
        if width == 0 {
            return 0;
        }

        let text_before_cursor = &buffer[..self.pos];
        let lines = textwrap::wrap(text_before_cursor, wrap_options(width));
        let mut cursor_line = lines.len().saturating_sub(1) as u16;

        // If cursor is right after a newline that textwrap didn't represent, add one
        if self.pos > 0
            && buffer.as_bytes()[self.pos - 1] == b'\n'
            && !lines.last().is_some_and(|l| l.is_empty())
        {
            cursor_line += 1;
        }

        cursor_line
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

        let options = wrap_options(width);
        let text_before_cursor = &buffer[..self.pos];
        let lines = textwrap::wrap(text_before_cursor, &options);

        let cursor_line = lines.len().saturating_sub(1) as u16;

        // Calculate cursor column by counting chars from last newline (preserves spaces!).
        // textwrap trims trailing whitespace, so we can't use wrapped line length.
        let last_newline = text_before_cursor
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);
        let logical_line_to_cursor = &text_before_cursor[last_newline..];

        // Wrap just the current logical line to find which wrapped segment we're on
        let logical_line_wrapped = textwrap::wrap(logical_line_to_cursor, options);

        let cursor_col = if logical_line_wrapped.is_empty() {
            0
        } else {
            let chars_in_prev_segments: usize = logical_line_wrapped
                .iter()
                .take(logical_line_wrapped.len() - 1)
                .map(|seg| seg.chars().count())
                .sum();

            let total_chars = logical_line_to_cursor.chars().count();
            (total_chars - chars_in_prev_segments) as u16
        };

        let visible_line = cursor_line.saturating_sub(self.scroll_offset);

        let screen_col = area.x + BORDER_OFFSET + cursor_col;
        let screen_row = area.y + BORDER_OFFSET + visible_line;

        (screen_col, screen_row)
    }
}
