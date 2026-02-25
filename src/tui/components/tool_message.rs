//! # ToolGroup Component
//!
//! Renders a tool call paired with its result as a single bordered block.
//!
//! **Collapsed** (not selected):
//!   `╭─ ⚙ add ──────────────────╮`
//!   `│ a: 42, b: 8 → result: 50 │`
//!   `╰───────────────────────────╯`
//!
//! **Expanded** (selected, pretty-printed JSON capped at MAX_SECTION_LINES):
//!   `╭─ ⚙ add ──────────────────╮`
//!   `│ ▸ input                   │`
//!   `│   {                       │`
//!   `│     "a": 42,              │`
//!   `│     "b": 8                │`
//!   `│   }                       │`
//!   `│ ◂ output                  │`
//!   `│   {                       │`
//!   `│     "result": 50          │`
//!   `│   }                       │`
//!   `╰───────────────────────────╯`

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph, Widget};

use crate::inference::{ToolCall, ToolResult};

/// Horizontal padding (per side) inside the bordered view.
const CONTENT_PAD_H: u16 = 1;
/// Total horizontal overhead: borders (2) + padding (2 × CONTENT_PAD_H).
const HORIZONTAL_OVERHEAD: u16 = 2 + CONTENT_PAD_H * 2;
/// Total vertical overhead: top border + bottom border.
const VERTICAL_OVERHEAD: u16 = 2;
/// Max lines per section (args / result) in expanded mode.
const MAX_SECTION_LINES: usize = 8;
/// Max chars before truncating a nested value in the collapsed summary.
const MAX_VALUE_CHARS: usize = 20;

// ─── Styles ──────────────────────────────────────────────────────────
// Yellow = tool identity/input (action happening), White = output (the answer).

const fn tool_style() -> Style {
    Style::new().fg(Color::Yellow)
}
const fn result_style() -> Style {
    Style::new().fg(Color::White)
}
const fn pending_style() -> Style {
    Style::new().fg(Color::DarkGray)
}
const fn overflow_style() -> Style {
    Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM)
}
const fn sep_style() -> Style {
    Style::new().fg(Color::DarkGray)
}

// ─── ToolGroup ───────────────────────────────────────────────────────

/// A tool call grouped with its (optional) result for unified rendering.
pub struct ToolGroup<'a> {
    pub call: &'a ToolCall,
    pub result: Option<&'a ToolResult>,
    pub is_selected: bool,
}

impl<'a> ToolGroup<'a> {
    /// Calculate height needed to render this group at the given width.
    ///
    /// Collapsed (not selected): borders + 1 summary line.
    /// Expanded (selected): borders + pretty-printed args + result (capped per section).
    pub fn calculate_height(
        call: &ToolCall,
        result: Option<&ToolResult>,
        is_selected: bool,
        width: u16,
    ) -> u16 {
        let content_width = width.saturating_sub(HORIZONTAL_OVERHEAD) as usize;
        if content_width == 0 {
            return 1;
        }

        if !is_selected {
            return 1 + VERTICAL_OVERHEAD;
        }

        // Expanded: label line + pretty-printed content per section
        // "▸ input" label (1 line) + indented JSON content
        let args_content = format_json_pretty(&call.arguments, MAX_SECTION_LINES).len() as u16;
        let args_total = 1 + args_content; // label + content

        // "◂ output" label (1 line) + content, or just "◂ …" (1 line) if pending
        let result_total = match result {
            Some(tr) => 1 + format_json_pretty(&tr.output, MAX_SECTION_LINES).len() as u16,
            None => 1,
        };

        args_total + result_total + VERTICAL_OVERHEAD
    }
}

impl<'a> Widget for ToolGroup<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        if self.is_selected {
            self.render_expanded(area, buf);
        } else {
            self.render_collapsed(area, buf);
        }
    }
}

impl<'a> ToolGroup<'a> {
    /// Bordered block with a single summary line using colored spans.
    /// Args in dim yellow, ` → ` separator gray, result in dim white.
    fn render_collapsed(self, area: Rect, buf: &mut Buffer) {
        let title = format!("⚙ {}", self.call.name);
        let border_style = tool_style().add_modifier(Modifier::DIM);

        let block = Block::bordered()
            .title(title)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style)
            .title_style(border_style)
            .padding(Padding::horizontal(CONTENT_PAD_H));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let budget = inner.width as usize;
        let args_str = summarize_json(&self.call.arguments, budget);

        let spans = match &self.result {
            Some(tr) => {
                let sep = " → ";
                let args_len = args_str.chars().count();
                let sep_len = sep.chars().count();
                let result_budget = budget.saturating_sub(args_len + sep_len);
                let result_str = summarize_json(&tr.output, result_budget);

                let total = args_len + sep_len + result_str.chars().count();
                if total <= budget {
                    vec![
                        Span::styled(args_str, tool_style().add_modifier(Modifier::DIM)),
                        Span::styled(sep, sep_style()),
                        Span::styled(result_str, result_style().add_modifier(Modifier::DIM)),
                    ]
                } else {
                    // Not enough room for result — just show args truncated
                    vec![Span::styled(
                        truncate_to(&args_str, budget),
                        tool_style().add_modifier(Modifier::DIM),
                    )]
                }
            }
            None => {
                let suffix = " …";
                let args_budget = budget.saturating_sub(suffix.chars().count());
                let args_str = summarize_json(&self.call.arguments, args_budget);
                vec![
                    Span::styled(args_str, tool_style().add_modifier(Modifier::DIM)),
                    Span::styled(suffix, pending_style()),
                ]
            }
        };

        Paragraph::new(Line::from(spans)).render(inner, buf);
    }

    /// Bordered block with labeled sections and pretty-printed JSON.
    fn render_expanded(self, area: Rect, buf: &mut Buffer) {
        let title = format!("⚙ {}", self.call.name);
        let border_style = tool_style();

        let block = Block::bordered()
            .title(title)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(border_style)
            .title_style(border_style)
            .padding(Padding::horizontal(CONTENT_PAD_H));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let args_content = format_json_pretty(&self.call.arguments, MAX_SECTION_LINES);
        let mut lines = Vec::new();

        // ▸ input label
        lines.push(Line::from(Span::styled("▸ input", tool_style())));
        // Indented args content
        for text in &args_content {
            let style = if text.starts_with("… +") {
                overflow_style()
            } else {
                tool_style()
            };
            lines.push(Line::from(Span::styled(format!("  {text}"), style)));
        }

        // Result section
        match &self.result {
            Some(tr) => {
                let result_content = format_json_pretty(&tr.output, MAX_SECTION_LINES);
                // ◂ output label
                lines.push(Line::from(Span::styled("◂ output", result_style())));
                // Indented result content
                for text in &result_content {
                    let style = if text.starts_with("… +") {
                        overflow_style()
                    } else {
                        result_style()
                    };
                    lines.push(Line::from(Span::styled(format!("  {text}"), style)));
                }
            }
            None => {
                lines.push(Line::from(Span::styled("◂ …", pending_style())));
            }
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

// ─── JSON Formatting Helpers ─────────────────────────────────────────

/// Summarize a JSON string as `k: v, k: v, …` for collapsed display.
/// Falls back to raw truncation for non-object JSON or invalid JSON.
fn summarize_json(raw: &str, budget: usize) -> String {
    let trimmed = raw.trim();
    if budget == 0 || trimmed.is_empty() {
        return String::new();
    }

    let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return truncate_to(trimmed, budget);
    };

    match val {
        serde_json::Value::Object(map) if !map.is_empty() => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}: {}", summarize_value(v)))
                .collect();
            join_with_ellipsis(&pairs, budget)
        }
        other => truncate_to(&other.to_string(), budget),
    }
}

/// Compact string representation of a JSON value for inline display.
fn summarize_value(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) if s.chars().count() > MAX_VALUE_CHARS => {
            let trunc: String = s.chars().take(MAX_VALUE_CHARS - 3).collect();
            format!("\"{trunc}…\"")
        }
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Object(map) => format!("{{…{} keys}}", map.len()),
        serde_json::Value::Array(arr) => format!("[…{} items]", arr.len()),
        other => other.to_string(),
    }
}

/// Join parts with `, ` separators, truncating with `…` if the result exceeds budget.
fn join_with_ellipsis(parts: &[String], budget: usize) -> String {
    let mut result = String::new();
    let total = parts.len();

    for (i, part) in parts.iter().enumerate() {
        let sep = if i > 0 { ", " } else { "" };
        let remaining = total - i - 1;

        let candidate_len = result.chars().count() + sep.chars().count() + part.chars().count();
        let ellipsis_overhead = if remaining > 0 { ", …".chars().count() } else { 0 };

        if candidate_len + ellipsis_overhead > budget && i > 0 {
            result.push_str(", …");
            return result;
        }

        result.push_str(sep);
        result.push_str(part);
    }

    // Edge case: single part that itself exceeds budget
    if result.chars().count() > budget {
        return truncate_to(&result, budget);
    }

    result
}

/// Pretty-print a JSON string, capping output at `max_lines`.
/// Falls back to raw text split by newlines for invalid JSON.
fn format_json_pretty(raw: &str, max_lines: usize) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return vec!["(empty)".to_string()];
    }

    let formatted = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|_| trimmed.to_string()),
        Err(_) => trimmed.to_string(),
    };

    let all_lines: Vec<&str> = formatted.lines().collect();
    let total = all_lines.len();

    if total <= max_lines {
        all_lines.iter().map(|l| l.to_string()).collect()
    } else {
        let take = max_lines - 1;
        let mut result: Vec<String> = all_lines[..take].iter().map(|l| l.to_string()).collect();
        result.push(format!("… +{} lines", total - take));
        result
    }
}

/// Truncate a string to fit within `budget` characters, appending `…` if truncated.
fn truncate_to(s: &str, budget: usize) -> String {
    if s.chars().count() <= budget {
        return s.to_string();
    }
    let truncated: String = s.chars().take(budget.saturating_sub(1)).collect();
    format!("{truncated}…")
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: String::new(),
            call_id: "call_1".into(),
            name: name.into(),
            arguments: args.into(),
        }
    }

    fn make_result(output: &str) -> ToolResult {
        ToolResult {
            call_id: "call_1".into(),
            output: output.into(),
        }
    }

    // ── Height tests ─────────────────────────────────────────────────

    #[test]
    fn collapsed_height_is_one_plus_borders() {
        let call = make_call("add", r#"{"a": 1}"#);
        let result = make_result(r#"{"sum": 2}"#);
        assert_eq!(
            ToolGroup::calculate_height(&call, Some(&result), false, 80),
            1 + VERTICAL_OVERHEAD
        );
    }

    #[test]
    fn collapsed_height_same_without_result() {
        let call = make_call("add", r#"{"a": 1}"#);
        assert_eq!(
            ToolGroup::calculate_height(&call, None, false, 80),
            1 + VERTICAL_OVERHEAD
        );
    }

    #[test]
    fn expanded_height_includes_labels_and_content() {
        let call = make_call("add", r#"{"a": 1, "b": 2}"#);
        let result = make_result(r#"{"sum": 3}"#);
        let height = ToolGroup::calculate_height(&call, Some(&result), true, 80);
        // ▸ input (1) + args 4 lines + ◂ output (1) + result 3 lines + borders
        assert_eq!(height, (1 + 4) + (1 + 3) + VERTICAL_OVERHEAD);
    }

    #[test]
    fn expanded_large_json_capped() {
        let mut obj = serde_json::Map::new();
        for i in 0..20 {
            obj.insert(format!("key_{i}"), serde_json::Value::Number(i.into()));
        }
        let args = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();
        let call = make_call("big_tool", &args);
        let result = make_result(r#"{"ok": true}"#);
        let height = ToolGroup::calculate_height(&call, Some(&result), true, 80);
        // ▸ input (1) + args capped (8) + ◂ output (1) + result 3 lines + borders
        assert_eq!(
            height,
            (1 + MAX_SECTION_LINES as u16) + (1 + 3) + VERTICAL_OVERHEAD
        );
    }

    #[test]
    fn expanded_pending_shows_placeholder() {
        let call = make_call("add", r#"{"a": 1}"#);
        let height = ToolGroup::calculate_height(&call, None, true, 80);
        // ▸ input (1) + args 3 pretty lines + ◂ … (1) + borders
        assert_eq!(height, (1 + 3) + 1 + VERTICAL_OVERHEAD);
    }

    #[test]
    fn zero_width_returns_minimum() {
        let call = make_call("add", r#"{"a": 1}"#);
        assert_eq!(ToolGroup::calculate_height(&call, None, false, 0), 1);
        assert_eq!(ToolGroup::calculate_height(&call, None, true, 0), 1);
    }

    // ── summarize_json tests ─────────────────────────────────────────

    #[test]
    fn summarize_small_object() {
        let s = summarize_json(r#"{"a": 42, "b": 8}"#, 80);
        assert_eq!(s, "a: 42, b: 8");
    }

    #[test]
    fn summarize_object_with_string_value() {
        let s = summarize_json(r#"{"name": "Alice"}"#, 80);
        assert_eq!(s, r#"name: "Alice""#);
    }

    #[test]
    fn summarize_overflow_truncates_at_key_boundary() {
        let s = summarize_json(r#"{"alpha": 1, "beta": 2, "gamma": 3}"#, 20);
        // "alpha: 1, beta: 2" = 18 chars, adding ", gamma: 3" would exceed 20
        assert!(s.ends_with(", …"), "got: {s}");
        assert!(s.starts_with("alpha: 1"));
    }

    #[test]
    fn summarize_nested_object_shows_key_count() {
        let s = summarize_json(r#"{"data": {"x": 1, "y": 2}}"#, 80);
        assert_eq!(s, "data: {…2 keys}");
    }

    #[test]
    fn summarize_array_shows_item_count() {
        let s = summarize_json(r#"{"items": [1, 2, 3]}"#, 80);
        assert_eq!(s, "items: […3 items]");
    }

    #[test]
    fn summarize_non_object_falls_back() {
        let s = summarize_json(r#""just a string""#, 80);
        assert_eq!(s, r#""just a string""#);
    }

    #[test]
    fn summarize_invalid_json_falls_back() {
        let s = summarize_json("not json at all", 80);
        assert_eq!(s, "not json at all");
    }

    // ── format_json_pretty tests ─────────────────────────────────────

    #[test]
    fn pretty_small_json() {
        let lines = format_json_pretty(r#"{"a": 1}"#, 10);
        assert_eq!(lines.len(), 3); // {, "a": 1, }
    }

    #[test]
    fn pretty_caps_at_max_lines() {
        let mut obj = serde_json::Map::new();
        for i in 0..20 {
            obj.insert(format!("k{i}"), serde_json::Value::Number(i.into()));
        }
        let json = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();
        let lines = format_json_pretty(&json, 5);
        assert_eq!(lines.len(), 5);
        assert!(lines.last().unwrap().starts_with("… +"));
    }

    #[test]
    fn pretty_non_json_returns_raw() {
        let lines = format_json_pretty("plain text", 10);
        assert_eq!(lines, vec!["plain text"]);
    }
}
