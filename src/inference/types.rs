use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Source {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Model,
    #[serde(rename = "system")]
    Directive,
    #[serde(rename = "thought")]
    Thinking,
    /// UI-only status indicator (e.g. "Preparing..."). Never sent to the model.
    #[serde(rename = "status")]
    Status,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ContextSegment {
    #[serde(rename = "role")]
    pub source: Source,
    pub content: String,
}

impl ContextSegment {
    /// Returns a new ContextSegment with normalized content.
    /// Replaces typographic characters with ASCII equivalents.
    #[cfg(test)]
    pub fn normalized(&self) -> ContextSegment {
        ContextSegment {
            source: self.source.clone(),
            content: replace_typography(&self.content).trim().to_string(),
        }
    }
}

/// Helper function to replace typographic characters with ASCII equivalents.
fn replace_typography(text: &str) -> String {
    text.replace(['‘', '’'], "'") // Single quotes
        .replace(['“', '”'], "\"") // Double quotes
        .replace('—', "--") // Em dash
        .replace('…', "...") // Ellipsis
}

/// A single item in the context — either a message, tool call, or tool result.
/// The Responses API input array is polymorphic; this enum mirrors that structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContextItem {
    Message(ContextSegment),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

/// Represents the model input context, holding a collection of context items.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Context {
    /// Polymorphic collection: messages, tool calls, and tool results.
    pub items: Vec<ContextItem>,
    /// Routes streaming item_ids to their index in `items`.
    /// Transient — only meaningful during an active stream.
    #[serde(skip)]
    active_streams: HashMap<String, usize>,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    /// Creates a new Context with the default system directive.
    pub fn new() -> Self {
        let sys_directive = ContextSegment {
            source: Source::Directive,
            content: String::from(
                "You are a helpful assistant. \
                 TOOL USE IS MANDATORY: if a registered tool can perform a computation, lookup, or action, you MUST call it. \
                 NEVER perform arithmetic, math, or calculations yourself — always delegate to the appropriate tool. \
                 When independent sub-expressions can be computed simultaneously, call multiple tools in parallel. \
                 When a result depends on a previous tool's output, wait for that result before proceeding. \
                 Your text responses should only interpret and present tool results, never substitute for them. \
                 Be direct, be honest about uncertainty, and prefer clarity over hedging.",
            ),
        };
        Context {
            items: vec![ContextItem::Message(sys_directive)],
            active_streams: HashMap::new(),
        }
    }

    /// Creates a new Context with a custom system directive.
    pub fn with_system_prompt(prompt: String) -> Self {
        let sys_directive = ContextSegment {
            source: Source::Directive,
            content: prompt,
        };
        Context {
            items: vec![ContextItem::Message(sys_directive)],
            active_streams: HashMap::new(),
        }
    }

    /// Adds a new ContextSegment (wrapped in ContextItem::Message) and returns a reference to it.
    pub fn add(&mut self, segment: ContextSegment) -> &ContextSegment {
        self.items.push(ContextItem::Message(segment));
        match self.items.last().expect("just pushed") {
            ContextItem::Message(seg) => seg,
            _ => unreachable!(),
        }
    }

    pub fn add_user_message(&mut self, content: String) -> &ContextSegment {
        let segment = ContextSegment {
            source: Source::User,
            content,
        };
        self.add(segment)
    }

    /// Adds a tool call to the context.
    pub fn add_tool_call(&mut self, tc: ToolCall) {
        self.items.push(ContextItem::ToolCall(tc));
    }

    /// Adds a tool result to the context.
    pub fn add_tool_result(&mut self, tr: ToolResult) {
        self.items.push(ContextItem::ToolResult(tr));
    }

    /// Appends content to the last message if it is from the model.
    /// If the last message is not from the model, creates a new one.
    ///
    /// When `item_id` is provided, routes via `active_streams` so interleaved
    /// thinking/content deltas don't fragment a single message into many.
    pub fn append_to_last_model_message(&mut self, content: &str, item_id: Option<&str>) {
        let normalized = replace_typography(content);

        // Route via active_streams when we have an item_id
        if let Some(id) = item_id
            && let Some(&idx) = self.active_streams.get(id)
            && let Some(ContextItem::Message(seg)) = self.items.get_mut(idx)
            && seg.source == Source::Model
        {
            seg.content.push_str(&normalized);
            return;
        }

        // Fallback: append to last item if it's a Model message
        let last_idx = self.items.len().wrapping_sub(1);
        if let Some(ContextItem::Message(seg)) = self.items.last_mut()
            && seg.source == Source::Model
        {
            seg.content.push_str(&normalized);
            if let Some(id) = item_id {
                self.active_streams.insert(id.to_string(), last_idx);
            }
            return;
        }

        // Don't create a new model message for whitespace-only content.
        // The API sometimes sends empty text deltas before reasoning/tool calls.
        if normalized.trim().is_empty() {
            return;
        }

        self.add(ContextSegment {
            source: Source::Model,
            content: normalized,
        });
        if let Some(id) = item_id {
            self.active_streams
                .insert(id.to_string(), self.items.len() - 1);
        }
    }

    /// Appends content to the last message if it is a thinking message.
    /// If the last message is not thinking, creates a new one.
    ///
    /// When `item_id` is provided, routes via `active_streams` so interleaved
    /// thinking/content deltas don't fragment a single message into many.
    pub fn append_to_last_thinking_message(&mut self, content: &str, item_id: Option<&str>) {
        let normalized = replace_typography(content);

        // Route via active_streams when we have an item_id
        if let Some(id) = item_id
            && let Some(&idx) = self.active_streams.get(id)
            && let Some(ContextItem::Message(seg)) = self.items.get_mut(idx)
            && seg.source == Source::Thinking
        {
            seg.content.push_str(&normalized);
            return;
        }

        // Fallback: append to last item if it's a Thinking message
        let last_idx = self.items.len().wrapping_sub(1);
        if let Some(ContextItem::Message(seg)) = self.items.last_mut()
            && seg.source == Source::Thinking
        {
            seg.content.push_str(&normalized);
            if let Some(id) = item_id {
                self.active_streams.insert(id.to_string(), last_idx);
            }
            return;
        }

        self.add(ContextSegment {
            source: Source::Thinking,
            content: normalized,
        });
        if let Some(id) = item_id {
            self.active_streams
                .insert(id.to_string(), self.items.len() - 1);
        }
    }

    /// Clears the active stream routing map. Called when a response completes.
    pub fn clear_active_streams(&mut self) {
        self.active_streams.clear();
    }
}

/// Effort level for reasoning tokens
/// Higher effort = more reasoning tokens = better quality but higher cost
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub enum Effort {
    /// Thorough analysis - model takes more time to reason
    #[serde(rename = "high")]
    High,
    /// Balanced reasoning
    #[serde(rename = "medium")]
    Medium,
    /// Quick thinking - faster but less thorough
    #[serde(rename = "low")]
    Low,
    /// Model decides whether and how much to reason (default)
    #[serde(rename = "auto")]
    #[default]
    Auto,
    /// Disables reasoning entirely
    #[serde(rename = "none")]
    None,
}

impl Effort {
    /// Cycles to the next effort level (wraps around)
    pub fn next(self) -> Effort {
        match self {
            Effort::None => Effort::Auto,
            Effort::Auto => Effort::Low,
            Effort::Low => Effort::Medium,
            Effort::Medium => Effort::High,
            Effort::High => Effort::None,
        }
    }

    /// Returns a human-readable label for display
    pub fn label(self) -> &'static str {
        match self {
            Effort::High => "High",
            Effort::Medium => "Medium",
            Effort::Low => "Low",
            Effort::Auto => "Auto",
            Effort::None => "Off",
        }
    }
}

/// A tool the model can call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

/// A completed tool call from the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String, // API object ID (e.g. "fc_abc123") — needed for input array roundtrip
    pub call_id: String, // Correlation ID (e.g. "call_xyz789") — links call to result
    pub name: String,
    pub arguments: String, // JSON string
}

/// Result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub call_id: String, // Matches ToolCall.call_id
    pub output: String,
}

/// Represents a chunk of streamed content from the model.
#[derive(Debug)]
pub enum StreamChunk {
    Content {
        text: String,
        item_id: Option<String>,
    },
    Thinking {
        text: String,
        item_id: Option<String>,
    },
    ToolCall(ToolCall), // Complete tool call (arguments buffered by provider)
    /// Signals stream completion. Providers send this as their final chunk before returning Ok(()).
    /// Carries usage statistics parsed from the `response.completed` payload, if available.
    Completed(Option<UsageStats>),
}

/// Token usage and timing statistics from a single inference round.
///
/// All fields are optional — providers vary in what they report.
/// Accumulated across agentic rounds via `accumulate()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
    pub finish_reason: Option<String>,
    pub ttft_ms: Option<u64>,
    pub tokens_per_sec: Option<f32>,
    pub generation_duration_ms: Option<u64>,
}

/// Adds two `Option<u32>` values: None + None = None, otherwise sum.
fn add_opt(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

impl UsageStats {
    /// Accumulates another round's stats into this one.
    /// Sums token counts, preserves the first TTFT, and recalculates tok/s.
    pub fn accumulate(&mut self, other: &UsageStats) {
        self.input_tokens = add_opt(self.input_tokens, other.input_tokens);
        self.output_tokens = add_opt(self.output_tokens, other.output_tokens);
        self.total_tokens = add_opt(self.total_tokens, other.total_tokens);
        self.cache_creation_input_tokens = add_opt(
            self.cache_creation_input_tokens,
            other.cache_creation_input_tokens,
        );
        self.cache_read_input_tokens =
            add_opt(self.cache_read_input_tokens, other.cache_read_input_tokens);

        // Keep first TTFT (most meaningful for the user's perceived latency)
        if self.ttft_ms.is_none() {
            self.ttft_ms = other.ttft_ms;
        }

        // Sum durations
        self.generation_duration_ms =
            add_opt_u64(self.generation_duration_ms, other.generation_duration_ms);

        // Recalculate tok/s from accumulated output_tokens / total_duration
        if let (Some(tokens), Some(duration_ms)) = (self.output_tokens, self.generation_duration_ms)
            && duration_ms > 0
        {
            self.tokens_per_sec = Some(tokens as f32 / (duration_ms as f32 / 1000.0));
        }

        // Last finish_reason wins
        if other.finish_reason.is_some() {
            self.finish_reason.clone_from(&other.finish_reason);
        }
    }

    /// Formats a human-readable summary for the status bar.
    /// e.g. "150 in / 42 out (80 cached) | TTFT 340ms | 28.5 tok/s | 1.2s"
    pub fn display_summary(&self) -> String {
        let mut parts = Vec::new();

        // Token counts
        let mut token_part = String::new();
        if let Some(input) = self.input_tokens {
            token_part.push_str(&format!("{input} in"));
        }
        if let Some(output) = self.output_tokens {
            if !token_part.is_empty() {
                token_part.push_str(" / ");
            }
            token_part.push_str(&format!("{output} out"));
        }
        if let Some(cached) = self.cache_read_input_tokens
            && cached > 0
        {
            token_part.push_str(&format!(" ({cached} cached)"));
        }
        if !token_part.is_empty() {
            parts.push(token_part);
        }

        // TTFT
        if let Some(ttft) = self.ttft_ms {
            parts.push(format!("TTFT {ttft}ms"));
        }

        // Tokens per second
        if let Some(tps) = self.tokens_per_sec {
            parts.push(format!("{tps:.1} tok/s"));
        }

        // Total generation duration
        if let Some(duration_ms) = self.generation_duration_ms {
            let secs = duration_ms as f64 / 1000.0;
            parts.push(format!("{secs:.1}s"));
        }

        if parts.is_empty() {
            "Response complete.".to_string()
        } else {
            parts.join(" | ")
        }
    }
}

/// Adds two `Option<u64>` values: None + None = None, otherwise sum.
fn add_opt_u64(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Macro to generate multiple normalization test cases.
    /// $name:ident is the name of the test function (use a name that describes the rule; this can be helpful for identifying failing tests)
    /// $input:expr is the input string to be normalized
    /// $expected:expr is the expected output string after normalization
    /// The macro generates a test function that asserts the normalization result matches the expected output.
    macro_rules! test_normalize_rules {
        ( $($name:ident: $input:expr => $expected:expr,)+ ) => {
            $(
                #[test]
                fn $name() {
                    let segment = ContextSegment {
                        source: Source::User,
                        content: $input.to_string(),
                    };
                    let normalized = segment.normalized();
                    assert_eq!(normalized.content, $expected);
                }
            )+
        };
    }

    test_normalize_rules! {
        test_normalize_rules_right_single_quote: "It’s a test." => "It's a test.",
        test_normalize_rules_left_single_quote: "‘Hello’" => "'Hello'",
        test_normalize_rules_right_double_quote: "She said, “Hello!”" => "She said, \"Hello!\"",
        test_normalize_rules_left_double_quote: "“Quote”" => "\"Quote\"",
        test_normalize_rules_em_dash: "Wait—what?" => "Wait--what?",
        test_normalize_rules_ellipsis: "And then…" => "And then...",
        test_normalize_rules_mixed_content: "‘Hello’—world…" => "'Hello'--world...",
        test_normalize_rules_no_special_chars: "Hello world" => "Hello world",
    }

    /// Helper to extract the ContextSegment from a ContextItem::Message, panicking otherwise.
    fn unwrap_message(item: &ContextItem) -> &ContextSegment {
        match item {
            ContextItem::Message(seg) => seg,
            other => panic!("Expected ContextItem::Message, got {:?}", other),
        }
    }

    #[test]
    fn test_context_init_with_directive() {
        let context = Context::new();
        assert!(!context.items.is_empty());
        let first = unwrap_message(&context.items[0]);
        assert_eq!(first.source, Source::Directive);
        assert!(first.content.starts_with("You are a helpful assistant"));
    }

    /// Tests adding ContextSegments to the Context.
    #[test]
    fn test_context_add() {
        let mut ctx = Context::new();
        let segment = ContextSegment {
            source: Source::User,
            content: "test".to_string(),
        };
        let added = ctx.add(segment);
        assert_eq!(added.content, "test");
        assert_eq!(ctx.items.len(), 2);
        ctx.add(ContextSegment {
            source: Source::Model,
            content: "response".to_string(),
        });
        assert_eq!(ctx.items.len(), 3);
        assert_eq!(unwrap_message(&ctx.items[2]).content, "response");
    }

    #[test]
    fn test_effort_cycle() {
        assert_eq!(Effort::None.next(), Effort::Auto);
        assert_eq!(Effort::Auto.next(), Effort::Low);
        assert_eq!(Effort::Low.next(), Effort::Medium);
        assert_eq!(Effort::Medium.next(), Effort::High);
        assert_eq!(Effort::High.next(), Effort::None);
    }

    #[test]
    fn test_context_append_to_last_thinking_message() {
        let mut ctx = Context::new();
        ctx.append_to_last_thinking_message("thinking", None);
        assert_eq!(ctx.items.len(), 2);
        let seg = unwrap_message(&ctx.items[1]);
        assert_eq!(seg.source, Source::Thinking);
        assert_eq!(seg.content, "thinking");

        ctx.append_to_last_thinking_message(" more", None);
        assert_eq!(ctx.items.len(), 2);
        assert_eq!(unwrap_message(&ctx.items[1]).content, "thinking more");
    }

    #[test]
    fn test_context_append_to_last_model_message() {
        let mut ctx = Context::new();
        // Add user message
        ctx.add_user_message("hello".to_string());

        // Append to non-model message (should create new)
        ctx.append_to_last_model_message("start", None);
        assert_eq!(ctx.items.len(), 3); // System, User, Model
        let seg = unwrap_message(&ctx.items[2]);
        assert_eq!(seg.content, "start");
        assert_eq!(seg.source, Source::Model);

        // Append to model message (should append)
        ctx.append_to_last_model_message(" continued", None);
        assert_eq!(ctx.items.len(), 3);
        assert_eq!(unwrap_message(&ctx.items[2]).content, "start continued");
    }

    #[test]
    fn test_append_model_message_skips_whitespace_only_creation() {
        let mut ctx = Context::new();
        // Whitespace-only should NOT create a new model message
        ctx.append_to_last_model_message("\n\n", None);
        assert_eq!(ctx.items.len(), 1); // Only system directive

        // Real content should create one
        ctx.append_to_last_model_message("Hello", None);
        assert_eq!(ctx.items.len(), 2);
        assert_eq!(unwrap_message(&ctx.items[1]).content, "Hello");

        // Whitespace APPENDED to existing model message is fine
        ctx.append_to_last_model_message("\n\n", None);
        assert_eq!(ctx.items.len(), 2); // Still 2, appended to existing
        assert_eq!(unwrap_message(&ctx.items[1]).content, "Hello\n\n");
    }

    #[test]
    fn test_interleaved_streaming_with_item_id() {
        let mut ctx = Context::new();
        let think_id = "item_think_0";
        let content_id = "item_content_1";

        // Simulate interleaved SSE deltas (thinking and content arriving alternately)
        ctx.append_to_last_thinking_message("Let me ", Some(think_id));
        ctx.append_to_last_model_message("Hello", Some(content_id));
        ctx.append_to_last_thinking_message("think...", Some(think_id));
        ctx.append_to_last_model_message(" world", Some(content_id));

        // Should produce exactly 2 messages (Thinking + Model), not 4 fragmented ones
        // items[0] = system directive, items[1] = Thinking, items[2] = Model
        assert_eq!(ctx.items.len(), 3);

        let thinking = unwrap_message(&ctx.items[1]);
        assert_eq!(thinking.source, Source::Thinking);
        assert_eq!(thinking.content, "Let me think...");

        let model = unwrap_message(&ctx.items[2]);
        assert_eq!(model.source, Source::Model);
        assert_eq!(model.content, "Hello world");
    }

    #[test]
    fn test_append_normalizes_content() {
        let mut ctx = Context::new();
        // Case 1: Typography in new message
        ctx.append_to_last_model_message("Hello “World”", None);
        assert_eq!(
            unwrap_message(ctx.items.last().unwrap()).content,
            "Hello \"World\""
        );

        // Case 2: Typography in appended chunk
        ctx.append_to_last_model_message("—WAIT", None);
        assert_eq!(
            unwrap_message(ctx.items.last().unwrap()).content,
            "Hello \"World\"--WAIT"
        );
    }

    // =====================================================================
    // UsageStats tests
    // =====================================================================

    #[test]
    fn test_accumulate_sums_tokens() {
        let mut base = UsageStats {
            input_tokens: Some(100),
            output_tokens: Some(20),
            total_tokens: Some(120),
            ..Default::default()
        };
        let round2 = UsageStats {
            input_tokens: Some(150),
            output_tokens: Some(30),
            total_tokens: Some(180),
            ..Default::default()
        };
        base.accumulate(&round2);
        assert_eq!(base.input_tokens, Some(250));
        assert_eq!(base.output_tokens, Some(50));
        assert_eq!(base.total_tokens, Some(300));
    }

    #[test]
    fn test_accumulate_preserves_first_ttft() {
        let mut base = UsageStats {
            ttft_ms: Some(340),
            ..Default::default()
        };
        let round2 = UsageStats {
            ttft_ms: Some(120),
            ..Default::default()
        };
        base.accumulate(&round2);
        assert_eq!(base.ttft_ms, Some(340)); // First TTFT preserved

        // When base has no TTFT, takes the other's
        let mut empty = UsageStats::default();
        empty.accumulate(&UsageStats {
            ttft_ms: Some(200),
            ..Default::default()
        });
        assert_eq!(empty.ttft_ms, Some(200));
    }

    #[test]
    fn test_accumulate_recalculates_tok_per_sec() {
        let mut base = UsageStats {
            output_tokens: Some(30),
            generation_duration_ms: Some(1000),
            ..Default::default()
        };
        let round2 = UsageStats {
            output_tokens: Some(20),
            generation_duration_ms: Some(1000),
            ..Default::default()
        };
        base.accumulate(&round2);
        // 50 tokens / 2.0 seconds = 25.0 tok/s
        assert!((base.tokens_per_sec.unwrap() - 25.0).abs() < 0.1);
    }

    #[test]
    fn test_display_summary_all_fields() {
        let stats = UsageStats {
            input_tokens: Some(150),
            output_tokens: Some(42),
            cache_read_input_tokens: Some(80),
            ttft_ms: Some(340),
            tokens_per_sec: Some(28.5),
            generation_duration_ms: Some(1200),
            ..Default::default()
        };
        let summary = stats.display_summary();
        assert!(summary.contains("150 in"));
        assert!(summary.contains("42 out"));
        assert!(summary.contains("80 cached"));
        assert!(summary.contains("TTFT 340ms"));
        assert!(summary.contains("28.5 tok/s"));
        assert!(summary.contains("1.2s"));
    }

    #[test]
    fn test_display_summary_empty() {
        let stats = UsageStats::default();
        assert_eq!(stats.display_summary(), "Response complete.");
    }

    #[test]
    fn test_display_summary_partial() {
        let stats = UsageStats {
            output_tokens: Some(42),
            generation_duration_ms: Some(1500),
            ..Default::default()
        };
        let summary = stats.display_summary();
        assert!(summary.contains("42 out"));
        assert!(summary.contains("1.5s"));
        assert!(!summary.contains("in")); // no input tokens
    }
}
