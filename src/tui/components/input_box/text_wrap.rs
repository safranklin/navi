//! Pure text wrapping utilities and dimensional constants for the InputBox.
//!
//! These are stateless helpers with no dependency on InputBox or CursorState.

/// Border (2) + padding (2) consumed horizontally by the bordered block
pub(super) const HORIZONTAL_OVERHEAD: u16 = 4;
/// Top + bottom borders consumed vertically
pub(super) const VERTICAL_OVERHEAD: u16 = 2;
/// Maximum visible content lines before internal scrolling kicks in
pub(super) const MAX_VISIBLE_LINES: u16 = 5;
/// Offset from area edge to content (border width)
pub(super) const BORDER_OFFSET: u16 = 1;

/// Build textwrap options configured for the input box inner width.
pub(super) fn wrap_options(inner_width: u16) -> textwrap::Options<'static> {
    textwrap::Options::new(inner_width as usize)
        .break_words(true)
        .word_separator(textwrap::WordSeparator::AsciiSpace)
}

/// Calculate the inner content width after subtracting border/padding overhead.
/// Returns 0 if the area is too narrow.
pub(super) fn inner_width(content_width: u16) -> u16 {
    content_width.saturating_sub(HORIZONTAL_OVERHEAD)
}

/// Count wrapped lines for the given text, accounting for trailing newlines
/// that textwrap may not represent as empty lines.
pub(super) fn wrap_line_count(text: &str, width: u16) -> u16 {
    if width == 0 || text.is_empty() {
        return 1;
    }

    let lines = textwrap::wrap(text, wrap_options(width));
    let mut count = (lines.len() as u16).max(1);

    // textwrap doesn't always produce an empty trailing line for a trailing newline
    if text.ends_with('\n') && !lines.last().is_some_and(|l| l.is_empty()) {
        count += 1;
    }

    count
}

/// Find the byte offset of the previous character boundary before `pos` in `text`.
pub(super) fn prev_char_boundary(text: &str, pos: usize) -> usize {
    text[..pos]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Find the byte offset of the next character boundary after `pos` in `text`.
pub(super) fn next_char_boundary(text: &str, pos: usize) -> usize {
    text[pos..]
        .char_indices()
        .nth(1)
        .map(|(i, _)| pos + i)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- wrap_line_count -------------------------------------------------

    #[test]
    fn wrap_line_count_empty_string() {
        assert_eq!(wrap_line_count("", 80), 1);
    }

    #[test]
    fn wrap_line_count_zero_width() {
        assert_eq!(wrap_line_count("hello", 0), 1);
    }

    #[test]
    fn wrap_line_count_single_line_fits() {
        assert_eq!(wrap_line_count("hello", 80), 1);
    }

    #[test]
    fn wrap_line_count_wraps_long_text() {
        // 10 chars into a 5-wide column -> 2 lines
        assert_eq!(wrap_line_count("aaaaaaaaaa", 5), 2);
    }

    #[test]
    fn wrap_line_count_trailing_newline_adds_line() {
        assert_eq!(wrap_line_count("hello\n", 80), 2);
    }

    #[test]
    fn wrap_line_count_explicit_newlines() {
        assert_eq!(wrap_line_count("a\nb\nc", 80), 3);
    }

    #[test]
    fn wrap_line_count_trailing_newline_after_wrap() {
        // "aaaaaaaaaa\n" at width 5 -> "aaaaa", "aaaaa", "" = 3 lines
        assert_eq!(wrap_line_count("aaaaaaaaaa\n", 5), 3);
    }

    // -- prev_char_boundary ----------------------------------------------

    #[test]
    fn prev_char_boundary_ascii() {
        assert_eq!(prev_char_boundary("abc", 2), 1);
    }

    #[test]
    fn prev_char_boundary_at_start() {
        // From pos 1, previous boundary is 0; from pos 0 would be degenerate
        assert_eq!(prev_char_boundary("abc", 1), 0);
    }

    #[test]
    fn prev_char_boundary_multibyte() {
        // "café" = [99, 97, 102, 195, 169] — 'é' starts at byte 3, len 2
        let s = "café";
        assert_eq!(s.len(), 5);
        // From end (byte 5), previous char boundary is byte 3 ('é')
        assert_eq!(prev_char_boundary(s, 5), 3);
        // From byte 3 ('é'), previous char boundary is byte 2 ('f')
        assert_eq!(prev_char_boundary(s, 3), 2);
    }

    #[test]
    fn prev_char_boundary_emoji() {
        // "a🔥b" = [97, 240,159,148,165, 98] — emoji is 4 bytes at offset 1
        let s = "a🔥b";
        assert_eq!(s.len(), 6);
        // From byte 6 (end), previous is byte 5 ('b')
        assert_eq!(prev_char_boundary(s, 6), 5);
        // From byte 5 ('b'), previous is byte 1 (emoji)
        assert_eq!(prev_char_boundary(s, 5), 1);
        // From byte 1 (emoji), previous is byte 0 ('a')
        assert_eq!(prev_char_boundary(s, 1), 0);
    }

    // -- next_char_boundary ----------------------------------------------

    #[test]
    fn next_char_boundary_ascii() {
        assert_eq!(next_char_boundary("abc", 0), 1);
        assert_eq!(next_char_boundary("abc", 1), 2);
    }

    #[test]
    fn next_char_boundary_at_end() {
        assert_eq!(next_char_boundary("abc", 2), 3);
    }

    #[test]
    fn next_char_boundary_multibyte() {
        let s = "café";
        // From byte 3 ('é'), next boundary is byte 5 (end)
        assert_eq!(next_char_boundary(s, 3), 5);
        // From byte 2 ('f'), next boundary is byte 3 ('é')
        assert_eq!(next_char_boundary(s, 2), 3);
    }

    #[test]
    fn next_char_boundary_emoji() {
        let s = "a🔥b";
        // From byte 0 ('a'), next is byte 1 (emoji)
        assert_eq!(next_char_boundary(s, 0), 1);
        // From byte 1 (emoji start), next is byte 5 ('b')
        assert_eq!(next_char_boundary(s, 1), 5);
    }
}
