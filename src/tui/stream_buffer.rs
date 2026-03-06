//! Stream smoothing buffer for normalizing SSE chunk delivery into even UI updates.
//!
//! Sits between the provider's StreamChunk channel and the Action channel. Accumulates
//! incoming text and releases it in controlled drips on a timer, splitting at word
//! boundaries so text doesn't appear mid-word.

use std::collections::VecDeque;

/// The kind of bufferable text content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    Content,
    Thinking,
}

/// Input to the buffer: a piece of text with its kind and optional item_id.
#[derive(Debug)]
pub struct BufferableChunk {
    pub kind: ChunkKind,
    pub item_id: Option<String>,
    pub text: String,
}

/// Output from the buffer: a smoothed piece of text ready to become an Action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmoothedChunk {
    pub kind: ChunkKind,
    pub item_id: Option<String>,
    pub text: String,
}

/// A segment of pending text, grouped by kind and item_id.
#[derive(Debug)]
struct PendingSegment {
    kind: ChunkKind,
    item_id: Option<String>,
    text: String,
}

/// Smoothing buffer that aggregates tiny chunks and splits large ones,
/// releasing text at a controlled rate on word boundaries.
pub struct StreamBuffer {
    pending: VecDeque<PendingSegment>,
    max_chars_per_tick: usize,
    thinking_multiplier: usize,
}

/// Max overshoot when scanning forward for a word boundary, as a fraction of budget.
/// Overshoot = max(budget * WORD_BOUNDARY_RATIO, 2) so small budgets don't emit entire words.
const WORD_BOUNDARY_RATIO: usize = 2;

impl StreamBuffer {
    pub fn new(max_chars_per_tick: usize, thinking_multiplier: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            max_chars_per_tick,
            thinking_multiplier,
        }
    }

    /// Push a chunk into the buffer. Appends to the tail segment if kind and item_id match,
    /// otherwise creates a new segment (preserving ordering).
    pub fn push(&mut self, chunk: BufferableChunk) {
        if let Some(tail) = self.pending.back_mut()
            && tail.kind == chunk.kind
            && tail.item_id == chunk.item_id
        {
            tail.text.push_str(&chunk.text);
            return;
        }
        self.pending.push_back(PendingSegment {
            kind: chunk.kind,
            item_id: chunk.item_id,
            text: chunk.text,
        });
    }

    /// Drain up to budget characters, splitting at word boundaries.
    /// Thinking segments get `thinking_multiplier` x the base budget.
    /// Budget spans across segments - if segment 1 has 4 chars left and budget is 12,
    /// the remaining 8 chars come from segment 2.
    pub fn flush(&mut self) -> Vec<SmoothedChunk> {
        let mut result = Vec::new();
        // Scale budget based on the leading segment's kind
        let multiplier = self
            .pending
            .front()
            .filter(|seg| seg.kind == ChunkKind::Thinking)
            .map_or(1, |_| self.thinking_multiplier);
        let mut budget = self.max_chars_per_tick * multiplier;

        while budget > 0 {
            let Some(seg) = self.pending.front_mut() else {
                break;
            };

            if seg.text.is_empty() {
                self.pending.pop_front();
                continue;
            }

            let split_pos = find_split_point(&seg.text, budget);

            if split_pos >= seg.text.len() {
                // Emit the entire segment
                let seg = self.pending.pop_front().unwrap();
                budget = budget.saturating_sub(seg.text.len());
                result.push(SmoothedChunk {
                    kind: seg.kind,
                    item_id: seg.item_id,
                    text: seg.text,
                });
            } else {
                // Split the segment
                let emitted: String = seg.text.drain(..split_pos).collect();
                result.push(SmoothedChunk {
                    kind: seg.kind,
                    item_id: seg.item_id.clone(),
                    text: emitted,
                });
                // After a split within a segment, stop - don't cross into the next segment
                // mid-tick to keep output coherent.
                break;
            }
        }

        result
    }

    /// Drain everything (used at stream end).
    pub fn flush_all(&mut self) -> Vec<SmoothedChunk> {
        self.pending
            .drain(..)
            .filter(|seg| !seg.text.is_empty())
            .map(|seg| SmoothedChunk {
                kind: seg.kind,
                item_id: seg.item_id,
                text: seg.text,
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.iter().all(|seg| seg.text.is_empty())
    }
}

/// Find the byte position to split text at, respecting word boundaries.
///
/// Scans forward from `budget` up to `budget + WORD_BOUNDARY_OVERSHOOT` looking for
/// whitespace. If no whitespace is found, cuts at the exact budget (won't stall on
/// long tokens like URLs).
fn find_split_point(text: &str, budget: usize) -> usize {
    if budget >= text.len() {
        return text.len();
    }

    // Snap budget to a char boundary (don't split mid-char)
    let budget = snap_to_char_boundary(text, budget);

    // Scan forward from budget for whitespace (overshoot scales with budget)
    let overshoot = (budget * WORD_BOUNDARY_RATIO).max(2);
    let scan_limit = (budget + overshoot).min(text.len());
    let scan_limit = snap_to_char_boundary(text, scan_limit);

    for (i, c) in text[budget..scan_limit].char_indices() {
        if c.is_whitespace() {
            // Include the whitespace in this chunk
            return budget + i + c.len_utf8();
        }
    }

    // No word boundary found within overshoot - cut at budget
    budget
}

/// Snap a byte index forward to the nearest char boundary.
fn snap_to_char_boundary(text: &str, pos: usize) -> usize {
    if pos >= text.len() {
        return text.len();
    }
    let mut p = pos;
    while p < text.len() && !text.is_char_boundary(p) {
        p += 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_chunk(text: &str) -> BufferableChunk {
        BufferableChunk {
            kind: ChunkKind::Content,
            item_id: None,
            text: text.to_string(),
        }
    }

    fn content_chunk_with_id(text: &str, id: &str) -> BufferableChunk {
        BufferableChunk {
            kind: ChunkKind::Content,
            item_id: Some(id.to_string()),
            text: text.to_string(),
        }
    }

    fn thinking_chunk(text: &str) -> BufferableChunk {
        BufferableChunk {
            kind: ChunkKind::Thinking,
            item_id: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn empty_buffer_flush_returns_nothing() {
        let mut buf = StreamBuffer::new(12, 1);
        assert!(buf.flush().is_empty());
        assert!(buf.flush_all().is_empty());
    }

    #[test]
    fn small_chunk_emits_fully_on_one_flush() {
        let mut buf = StreamBuffer::new(12, 1);
        buf.push(content_chunk("hello"));
        let out = buf.flush();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "hello");
        assert!(buf.is_empty());
    }

    #[test]
    fn large_chunk_splits_across_multiple_flushes() {
        let mut buf = StreamBuffer::new(12, 1);
        buf.push(content_chunk("the quick brown fox jumps over the lazy dog"));

        let first = buf.flush();
        assert!(!buf.is_empty());
        assert_eq!(first.len(), 1);
        // Should split at a word boundary near 12 chars
        // "the quick brown " is 16 chars - "brown " crosses overshoot
        // "the quick " is 10 chars - within budget
        // Actually: budget=12, text[12] is in "brown", scan forward for whitespace
        // "the quick br" -> scan forward finds 'o','w','n',' ' at offset 4 -> split at 16
        assert_eq!(first[0].text, "the quick brown ");

        let second = buf.flush();
        assert_eq!(second.len(), 1);
        // "fox jumps over the lazy dog" remaining, budget 12
        // "fox jumps ov" -> scan finds 'e','r',' ' at offset 3 -> split at 15
        assert_eq!(second[0].text, "fox jumps over ");

        let third = buf.flush();
        assert_eq!(third.len(), 1);
        // "the lazy dog" = 12 chars, fits in budget
        assert_eq!(third[0].text, "the lazy dog");
        assert!(buf.is_empty());
    }

    #[test]
    fn word_boundary_splitting_does_not_cut_mid_word() {
        let mut buf = StreamBuffer::new(5, 1);
        buf.push(content_chunk("hello world"));

        let out = buf.flush();
        assert_eq!(out[0].text, "hello ");
        // "hello" is 5 chars at budget, scan forward finds ' ' at offset 0 -> split at 6
    }

    #[test]
    fn tiny_chunks_aggregate_into_single_flush() {
        let mut buf = StreamBuffer::new(20, 1);
        buf.push(content_chunk("he"));
        buf.push(content_chunk("ll"));
        buf.push(content_chunk("o "));
        buf.push(content_chunk("world"));

        let out = buf.flush();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "hello world");
    }

    #[test]
    fn item_id_change_creates_separate_segments_drained_in_order() {
        let mut buf = StreamBuffer::new(50, 1);
        buf.push(content_chunk_with_id("first", "a"));
        buf.push(content_chunk_with_id("second", "b"));

        let out = buf.flush();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "first");
        assert_eq!(out[0].item_id.as_deref(), Some("a"));
        assert_eq!(out[1].text, "second");
        assert_eq!(out[1].item_id.as_deref(), Some("b"));
    }

    #[test]
    fn mixed_content_thinking_segments_drain_in_order() {
        let mut buf = StreamBuffer::new(50, 1);
        buf.push(content_chunk("visible"));
        buf.push(thinking_chunk("reasoning"));
        buf.push(content_chunk("more visible"));

        let out = buf.flush();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].kind, ChunkKind::Content);
        assert_eq!(out[0].text, "visible");
        assert_eq!(out[1].kind, ChunkKind::Thinking);
        assert_eq!(out[1].text, "reasoning");
        assert_eq!(out[2].kind, ChunkKind::Content);
        assert_eq!(out[2].text, "more visible");
    }

    #[test]
    fn flush_all_drains_everything() {
        let mut buf = StreamBuffer::new(5, 1); // Small budget - flush would split
        buf.push(content_chunk("hello world this is a long string"));

        let out = buf.flush_all();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "hello world this is a long string");
        assert!(buf.is_empty());
    }

    #[test]
    fn is_empty_tracks_state_correctly() {
        let mut buf = StreamBuffer::new(12, 1);
        assert!(buf.is_empty());

        buf.push(content_chunk("hello"));
        assert!(!buf.is_empty());

        buf.flush();
        assert!(buf.is_empty());
    }

    #[test]
    fn budget_spans_across_segments() {
        let mut buf = StreamBuffer::new(12, 1);
        buf.push(content_chunk_with_id("hi ", "a"));
        buf.push(content_chunk_with_id("hello world", "b"));

        let out = buf.flush();
        // First segment "hi " (3 chars) fully drained, leaving 9 budget
        // Second segment "hello world" - budget 9, scan from 9 forward finds ' ' already passed
        // "hello wor" -> scan finds 'l','d' no whitespace in overshoot -> cut at 9
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "hi ");
        assert_eq!(out[0].item_id.as_deref(), Some("a"));
        // Second chunk gets remaining budget
        assert_eq!(out[1].item_id.as_deref(), Some("b"));
        assert!(!out[1].text.is_empty());
    }

    #[test]
    fn long_unbreakable_token_cuts_at_budget() {
        let mut buf = StreamBuffer::new(5, 1);
        buf.push(content_chunk("abcdefghijklmnop"));

        let out = buf.flush();
        // No whitespace within overshoot (5+8=13), cuts at budget
        assert_eq!(out[0].text, "abcde");

        let out = buf.flush();
        assert_eq!(out[0].text, "fghij");
    }

    #[test]
    fn thinking_multiplier_scales_budget() {
        let mut buf = StreamBuffer::new(4, 3); // content: 4, thinking: 12
        buf.push(thinking_chunk("the quick brown fox"));

        let out = buf.flush();
        // Budget = 4 * 3 = 12, same word boundary logic as content
        assert_eq!(out[0].text, "the quick brown ");
        assert_eq!(out[0].kind, ChunkKind::Thinking);
    }

    #[test]
    fn thinking_multiplier_does_not_affect_content() {
        let mut buf = StreamBuffer::new(4, 3);
        buf.push(content_chunk("the quick brown fox"));

        let out = buf.flush();
        // Budget = 4 * 1 = 4, scans forward up to +8 for word boundary
        // "the " is at index 4, but scan starts at 4 and finds 'q','u','i','c','k',' '
        // -> splits at "the quick " (10 chars)
        assert_eq!(out[0].text, "the quick ");
        assert_eq!(out[0].kind, ChunkKind::Content);

        // Second flush also gets content budget (4)
        let out = buf.flush();
        // "brown fox" - budget 4, scan forward finds ' ' after "brown"
        assert_eq!(out[0].text, "brown ");
    }
}
