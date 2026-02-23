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

/// Whether a character is a "word" character (alphanumeric or underscore).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Find the byte offset of the previous word boundary before `pos` in `text`.
///
/// Moves backwards: first skips any non-word characters (spaces, punctuation),
/// then skips word characters until reaching a non-word character or the start.
/// This matches Emacs/readline `backward-word` behavior.
pub(super) fn prev_word_boundary(text: &str, pos: usize) -> usize {
    let before = &text[..pos];
    let mut chars = before.char_indices().rev().peekable();

    // Phase 1: skip non-word characters
    while chars.peek().is_some_and(|&(_, c)| !is_word_char(c)) {
        chars.next();
    }

    // Phase 2: skip word characters
    let mut boundary = 0;
    while let Some(&(i, c)) = chars.peek() {
        if !is_word_char(c) {
            boundary = i + c.len_utf8();
            break;
        }
        boundary = i;
        chars.next();
    }

    boundary
}

/// Find the byte offset of the next word boundary after `pos` in `text`.
///
/// Moves forward: first skips any non-word characters, then skips word
/// characters until reaching a non-word character or the end.
/// This matches Emacs/readline `forward-word` behavior.
pub(super) fn next_word_boundary(text: &str, pos: usize) -> usize {
    let after = &text[pos..];
    let mut chars = after.char_indices().peekable();

    // Phase 1: skip non-word characters
    while chars.peek().is_some_and(|&(_, c)| !is_word_char(c)) {
        chars.next();
    }

    // Phase 2: skip word characters
    while let Some(&(_, c)) = chars.peek() {
        if !is_word_char(c) {
            break;
        }
        chars.next();
    }

    // Return byte offset relative to the full string
    match chars.peek() {
        Some(&(i, _)) => pos + i,
        None => text.len(),
    }
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

    // -- prev_word_boundary -------------------------------------------------

    #[test]
    fn prev_word_simple() {
        // "hello world" — from end (11), skip back over "world" → 6
        assert_eq!(prev_word_boundary("hello world", 11), 6);
    }

    #[test]
    fn prev_word_from_middle_of_word() {
        // "hello world" — from byte 8 (mid-"world"), skip back over "wor" → 6
        assert_eq!(prev_word_boundary("hello world", 8), 6);
    }

    #[test]
    fn prev_word_multiple_spaces() {
        // "hello   world" — from byte 12 (end of "world"), skip spaces then "hello" → 0
        assert_eq!(prev_word_boundary("hello   world", 8), 0);
    }

    #[test]
    fn prev_word_at_start() {
        assert_eq!(prev_word_boundary("hello", 0), 0);
    }

    #[test]
    fn prev_word_punctuation() {
        // "foo.bar" — from end (7), skip "bar", stop at '.' → 4
        assert_eq!(prev_word_boundary("foo.bar", 7), 4);
    }

    #[test]
    fn prev_word_underscore_is_word_char() {
        // "hello_world test" — from byte 16, skip "test" → 12
        // from byte 12, skip space then "hello_world" as one word → 0
        assert_eq!(prev_word_boundary("hello_world test", 16), 12);
        assert_eq!(prev_word_boundary("hello_world test", 12), 0);
    }

    #[test]
    fn prev_word_unicode() {
        // "café latte" — from end, skip "latte" → 5 (byte offset after space)
        assert_eq!(prev_word_boundary("café latte", "café latte".len()), 6);
    }

    #[test]
    fn prev_word_at_word_boundary() {
        // "hello world" — from byte 6 ('w'), no non-word to skip, skip back over "hello" → 0
        assert_eq!(prev_word_boundary("hello world", 6), 0);
    }

    // -- next_word_boundary -------------------------------------------------

    #[test]
    fn next_word_simple() {
        // "hello world" — from 0, skip "hello" → 5
        assert_eq!(next_word_boundary("hello world", 0), 5);
    }

    #[test]
    fn next_word_from_space() {
        // "hello world" — from 5 (space), skip space then "world" → 11
        assert_eq!(next_word_boundary("hello world", 5), 11);
    }

    #[test]
    fn next_word_multiple_spaces() {
        // "hello   world" — from 5 (first space), skip spaces then "world" → 13
        assert_eq!(next_word_boundary("hello   world", 5), 13);
    }

    #[test]
    fn next_word_at_end() {
        assert_eq!(next_word_boundary("hello", 5), 5);
    }

    #[test]
    fn next_word_punctuation() {
        // "foo.bar" — from 0, skip "foo" → 3
        assert_eq!(next_word_boundary("foo.bar", 0), 3);
    }

    #[test]
    fn next_word_underscore_is_word_char() {
        // "hello_world test" — from 0, treat "hello_world" as one word → 11
        assert_eq!(next_word_boundary("hello_world test", 0), 11);
    }

    #[test]
    fn next_word_unicode() {
        // "café latte" — from 0, skip "café" → 5 (byte offset of 'é' end)
        assert_eq!(next_word_boundary("café latte", 0), 5);
    }

    #[test]
    fn next_word_from_middle() {
        // "hello world" — from 2 (mid-"hello"), skip remaining "llo" → 5
        assert_eq!(next_word_boundary("hello world", 2), 5);
    }
}
