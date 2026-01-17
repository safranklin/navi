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
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ModelSegment {
    #[serde(rename = "role")]
    pub source: Source,
    pub content: String,
}

impl ModelSegment {
    /// Returns a new ModelSegment with normalized content.
    /// Replaces typographic characters with ASCII equivalents.
    #[cfg(test)]
    pub fn normalized(&self) -> ModelSegment {
        ModelSegment {
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

/// Represents the model input context, holding a collection of ModelSegments.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Context {
    /// Collection of ModelSegments representing the model input in its entirety.
    pub items: Vec<ModelSegment>,
}

impl Context {
    /// Creates a new Context with the default system directive.
    pub fn new() -> Self {
        let sys_directive = ModelSegment {
            source: Source::Directive,
            content: String::from("You are Navi, a small helpful fairy. Like a guide in a fairy tale, you help travelers on their journey. You are cheerful, speak briefly, and sometimes say \"Hey!\" to get attention. You give useful advice without long explanations."),
        };
        Context {
            items: vec![sys_directive],
        }
    }
    /// Adds a new ModelSegment to the context and returns a reference to the newly added segment.
    pub fn add(&mut self, segment: ModelSegment) -> &ModelSegment {
        self.items.push(segment);
        self.items.last().expect("just added an element to the context so it must exist")
    }
    pub fn add_user_message(&mut self, content: String) -> &ModelSegment {
        let segment = ModelSegment {
            source: Source::User,
            content,
        };
        self.add(segment)
    }

    /// Appends content to the last message if it is from the model.
    /// If the last message is not from the model, creates a new one.
    pub fn append_to_last_model_message(&mut self, content: &str) {
        let normalized = replace_typography(content);
        
        if let Some(last) = self.items.last_mut() {
            if last.source == Source::Model {
                last.content.push_str(&normalized);
                return;
            }
        }
        
        // If we get here, either no items or last item is not model
        self.add(ModelSegment {
            source: Source::Model,
            content: normalized,
        });
    }

    /// Appends content to the last message if it is a thinking message.
    /// If the last message is not thinking, creates a new one.
    pub fn append_to_last_thinking_message(&mut self, content: &str) {
        let normalized = replace_typography(content);
        
        if let Some(last) = self.items.last_mut() {
            if last.source == Source::Thinking {
                last.content.push_str(&normalized);
                return;
            }
        }
        
        self.add(ModelSegment {
            source: Source::Thinking,
            content: normalized,
        });
    }
}

/// Effort level for reasoning tokens
/// Higher effort = more reasoning tokens = better quality but higher cost
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
pub enum Effort {
    /// ~95% of max_tokens for reasoning - maximum depth
    #[serde(rename = "xhigh")]
    XHigh,
    /// ~80% of max_tokens for reasoning - thorough analysis
    #[serde(rename = "high")]
    High,
    /// ~50% of max_tokens for reasoning - balanced (default)
    #[serde(rename = "medium")]
    #[default]
    Medium,
    /// ~20% of max_tokens for reasoning - quick thinking
    #[serde(rename = "low")]
    Low,
    /// Disables reasoning entirely
    #[serde(rename = "none")]
    None,
}

impl Effort {
    /// Cycles to the next effort level (wraps around)
    pub fn next(self) -> Effort {
        match self {
            Effort::None => Effort::Low,
            Effort::Low => Effort::Medium,
            Effort::Medium => Effort::High,
            Effort::High => Effort::XHigh,
            Effort::XHigh => Effort::None,
        }
    }

    /// Returns a human-readable label for display
    pub fn label(self) -> &'static str {
        match self {
            Effort::XHigh => "XHigh",
            Effort::High => "High",
            Effort::Medium => "Medium",
            Effort::Low => "Low",
            Effort::None => "Off",
        }
    }
}

/// Configuration for reasoning tokens in API requests
#[derive(Serialize, Debug, Default)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
}

#[derive(Serialize, Debug)]
pub struct ModelRequest {
    pub model: String,
    pub messages: Vec<ModelSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}

#[derive(Debug)]
pub enum StreamChunk {
    Content(String),
    Thinking(String),
}

// ModelResponse and Choice removed

/// Represents the streaming response from the Model API
#[derive(Deserialize, Debug)]
pub struct ModelStreamResponse {
    pub choices: Vec<StreamChoice>,
}

#[derive(Deserialize, Debug)]
pub struct StreamChoice {
    pub delta: Delta,
}

#[derive(Deserialize, Debug)]
pub struct Delta {
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub _role: Option<String>,
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
                    let segment = ModelSegment {
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

    #[test]
    fn test_context_init_with_directive() {
        let context = Context::new();
        assert!(!context.items.is_empty());
        assert_eq!(context.items[0].source, Source::Directive);
        assert!(context.items[0].content.starts_with("You are Navi"));
    }

    /// Tests adding ModelSegments to the Context.
    #[test]
    fn test_context_add() {
        let mut ctx = Context::new();
        let segment = ModelSegment {
            source: Source::User,
            content: "test".to_string(),
        };
        let added = ctx.add(segment);
        // When a ModelSegment is added, it a reference to it shouild be added and the length of items should increase.
        assert_eq!(added.content, "test");
        assert_eq!(ctx.items.len(), 2);
        ctx.add(ModelSegment {
            source: Source::Model,
            content: "response".to_string(),
        });
        // Verify the second addition
        assert_eq!(ctx.items.len(), 3);
        assert_eq!(ctx.items[2].content, "response");
    }
    
    /// This is a contract test to ensure that the ModelRequest serializes correctly to JSON.
    #[test]
    fn test_model_request_serialization() {
        let req = ModelRequest {
            model: "test-model".to_string(),
            messages: vec![
                ModelSegment {
                    source: Source::User,
                    content: "hello".to_string(),
                },
                ModelSegment {
                    source: Source::Model,
                    content: "hi there".to_string(),
                },
            ],
            stream: None,
            reasoning: None,
        };

        let serialized = serde_json::to_string(&req).unwrap();
        let expected = r#"{"model":"test-model","messages":[{"role":"user","content":"hello"},{"role":"assistant","content":"hi there"}]}"#;
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_reasoning_config_serialization() {
        let config = ReasoningConfig {
            effort: Some(Effort::High),
            exclude: None,
        };
        let serialized = serde_json::to_string(&config).unwrap();
        assert_eq!(serialized, r#"{"effort":"high"}"#);
    }

    #[test]
    fn test_effort_cycle() {
        assert_eq!(Effort::None.next(), Effort::Low);
        assert_eq!(Effort::Low.next(), Effort::Medium);
        assert_eq!(Effort::Medium.next(), Effort::High);
        assert_eq!(Effort::High.next(), Effort::XHigh);
        assert_eq!(Effort::XHigh.next(), Effort::None);
    }

    #[test]
    fn test_context_append_to_last_thinking_message() {
        let mut ctx = Context::new();
        ctx.append_to_last_thinking_message("thinking");
        assert_eq!(ctx.items.len(), 2);
        assert_eq!(ctx.items[1].source, Source::Thinking);
        assert_eq!(ctx.items[1].content, "thinking");

        ctx.append_to_last_thinking_message(" more");
        assert_eq!(ctx.items.len(), 2);
        assert_eq!(ctx.items[1].content, "thinking more");
    }

    #[test]
    fn test_context_append_to_last_model_message() {
        let mut ctx = Context::new();
        // Add user message
        ctx.add_user_message("hello".to_string());
        
        // Append to non-model message (should create new)
        ctx.append_to_last_model_message("start");
        assert_eq!(ctx.items.len(), 3); // System, User, Model
        assert_eq!(ctx.items[2].content, "start");
        assert_eq!(ctx.items[2].source, Source::Model);
        
        // Append to model message (should append)
        ctx.append_to_last_model_message(" continued");
        assert_eq!(ctx.items.len(), 3);
        assert_eq!(ctx.items[2].content, "start continued");
    }

    #[test]
    fn test_append_normalizes_content() {
        let mut ctx = Context::new();
        // Case 1: Typography in new message
        ctx.append_to_last_model_message("Hello “World”");
        assert_eq!(ctx.items.last().unwrap().content, "Hello \"World\"");

        // Case 2: Typography in appended chunk
        ctx.append_to_last_model_message("—WAIT");
        assert_eq!(ctx.items.last().unwrap().content, "Hello \"World\"--WAIT");
    }
}