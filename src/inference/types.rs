use serde::{Deserialize, Serialize};

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
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Context {
    /// Polymorphic collection: messages, tool calls, and tool results.
    pub items: Vec<ContextItem>,
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
                "You are a helpful assistant guided by three principles: be genuinely useful, be honest about uncertainty, \
                 and be direct without being terse. Think before responding. Prefer clarity over hedging. \
                 If a tool is registered that could help you answer, call it with the appropriate arguments. \
                 Prefer tool enriched answers over purely internal ones. If you don't know the answer, say you don't know. \
                 If you need more information to answer, ask for it. Use all available tools to answer questions about recent events or information not in your training data. \
                 Always use tools when relevant information is outside your training data or if it would improve the quality of your response."
            ),
        };
        Context {
            items: vec![ContextItem::Message(sys_directive)],
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
    pub fn append_to_last_model_message(&mut self, content: &str) {
        let normalized = replace_typography(content);

        if let Some(ContextItem::Message(seg)) = self.items.last_mut()
            && seg.source == Source::Model {
                seg.content.push_str(&normalized);
                return;
            }

        // If we get here, either no items or last item is not a model message
        self.add(ContextSegment {
            source: Source::Model,
            content: normalized,
        });
    }

    /// Appends content to the last message if it is a thinking message.
    /// If the last message is not thinking, creates a new one.
    pub fn append_to_last_thinking_message(&mut self, content: &str) {
        let normalized = replace_typography(content);

        if let Some(ContextItem::Message(seg)) = self.items.last_mut()
            && seg.source == Source::Thinking {
                seg.content.push_str(&normalized);
                return;
            }

        self.add(ContextSegment {
            source: Source::Thinking,
            content: normalized,
        });
    }
}

/// Effort level for reasoning tokens
/// Higher effort = more reasoning tokens = better quality but higher cost
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
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
    pub id: String,      // API object ID (e.g. "fc_abc123") — needed for input array roundtrip
    pub call_id: String,  // Correlation ID (e.g. "call_xyz789") — links call to result
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
    Content(String),
    Thinking(String),
    ToolCall(ToolCall), // Complete tool call (arguments buffered by provider)
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
        ctx.append_to_last_thinking_message("thinking");
        assert_eq!(ctx.items.len(), 2);
        let seg = unwrap_message(&ctx.items[1]);
        assert_eq!(seg.source, Source::Thinking);
        assert_eq!(seg.content, "thinking");

        ctx.append_to_last_thinking_message(" more");
        assert_eq!(ctx.items.len(), 2);
        assert_eq!(unwrap_message(&ctx.items[1]).content, "thinking more");
    }

    #[test]
    fn test_context_append_to_last_model_message() {
        let mut ctx = Context::new();
        // Add user message
        ctx.add_user_message("hello".to_string());
        
        // Append to non-model message (should create new)
        ctx.append_to_last_model_message("start");
        assert_eq!(ctx.items.len(), 3); // System, User, Model
        let seg = unwrap_message(&ctx.items[2]);
        assert_eq!(seg.content, "start");
        assert_eq!(seg.source, Source::Model);

        // Append to model message (should append)
        ctx.append_to_last_model_message(" continued");
        assert_eq!(ctx.items.len(), 3);
        assert_eq!(unwrap_message(&ctx.items[2]).content, "start continued");
    }

    #[test]
    fn test_append_normalizes_content() {
        let mut ctx = Context::new();
        // Case 1: Typography in new message
        ctx.append_to_last_model_message("Hello “World”");
        assert_eq!(unwrap_message(ctx.items.last().unwrap()).content, "Hello \"World\"");

        // Case 2: Typography in appended chunk
        ctx.append_to_last_model_message("—WAIT");
        assert_eq!(unwrap_message(ctx.items.last().unwrap()).content, "Hello \"World\"--WAIT");
    }
}