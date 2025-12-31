use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Source {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Model,
    #[serde(rename = "system")]
    Directive,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelSegment {
    #[serde(rename = "role")]
    pub source: Source,
    pub content: String,
}

impl ModelSegment {
    /// Returns a new ModelSegment with normalized content.
    /// Replaces typographic characters with ASCII equivalents.
    pub fn normalized(&self) -> ModelSegment {

        // Replace curly quotes, em dashes, and ellipses with ASCII equivalents
        let normalized_content = self.content
            .replace(['‘', '’'], "'")
            .replace(['“', '”'], "\"")
            .replace('—', "--")
            .replace('…', "...");

        ModelSegment {
            source: self.source.clone(),
            content: normalized_content,
        }
    }
}

/// Represents the model input context, holding a collection of ModelSegments.
#[derive(Serialize, Debug)]
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
    /// Adds a user message to the context.
    pub fn add_user_message(&mut self, content: String) -> &ModelSegment {
        let segment = ModelSegment {
            source: Source::User,
            content,
        };
        self.add(segment)
    }
}

/// Represents the request payload sent to the Model API
#[derive(Serialize, Debug)]
pub struct ModelRequest {
    pub model: String,
    pub messages: Vec<ModelSegment>,
}

/// Represents the response from the Model API
#[derive(Deserialize, Debug)]
pub struct ModelResponse {
    pub choices: Vec<Choice>,
}

/// Represents a single sampled path through the probability space. Because the model doesn't always return the most probable set of tokens,
/// multiple choices can be returned if requested.
/// 
/// This response is a Token Segment
#[derive(Deserialize, Debug)]
pub struct Choice {
    pub message: ModelSegment,
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
        };

        let serialized = serde_json::to_string(&req).unwrap();
        let expected = r#"{"model":"test-model","messages":[{"role":"user","content":"hello"},{"role":"assistant","content":"hi there"}]}"#;
        assert_eq!(serialized, expected);
    }
}