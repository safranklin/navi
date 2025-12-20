use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
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

impl fmt::Display for ModelSegment {
    /// Formats the ModelSegment for display in the terminal.
    /// TODO: Replace with ratatui rendering later.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Take a chat message, if the role is not user, for now we will assume it's from navi. This should be the case, for now.
        let role_str = match self.source {
            Source::User => "user",
            Source::Model => "navi",
            Source::Directive => "system",
        };

        let content = &self.content;
        write!(f, "{}> {}", role_str, content)
    }
}

/// Represents the model input context, holding a collection of ModelSegments.
#[derive(Serialize, Debug)]
pub struct Context {
    /// Collection of ModelSegments representing the model input in it's entirety.
    pub items: Vec<ModelSegment>,
}

impl Context {
    /// Creates a new, empty Context.
    pub fn new() -> Self {
        Context { items: Vec::new() }
    }
    /// Adds a new ModelSegment to the context and returns a reference to the newly added segment.
    pub fn add(&mut self, segment: ModelSegment) -> &ModelSegment {
        self.items.push(segment);
        self.items.last().expect("just added an element to the context so it must exist")
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

    #[test]
    fn test_chat_message_display() {
        let msg = ModelSegment {
            source: Source::User,
            content: "hello".to_string(),
        };
        assert_eq!(msg.to_string(), "user> hello");
    }

    #[test]
    fn test_chat_message_display_model() {
        let msg = ModelSegment {
            source: Source::Model,
            content: "hi there".to_string(),
        };
        assert_eq!(msg.to_string(), "navi> hi there");
    }

    #[test]
    fn test_chat_message_display_directive() {
        let msg = ModelSegment {
            source: Source::Directive,
            content: "system message".to_string(),
        };
        assert_eq!(msg.to_string(), "system> system message");
    }

    #[test]
    fn test_collection_display() {
        let msgs = vec![
            ModelSegment {
                source: Source::User, // This segment of the context is from the user
                content: "hello".to_string(),
            },
            ModelSegment {
                source: Source::Model, // This segment of the context is from the model
                content: "hi there".to_string(),
            },
        ];
        let collection = Context { items: msgs };

        let display_output: Vec<String> = collection.items.iter().map(|m| m.to_string()).collect();
        assert_eq!(display_output, vec!["user> hello", "navi> hi there"]);
    }

    #[test]
    fn test_context_init_empty() {
        let context = Context::new();
        assert!(context.items.is_empty());
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
        assert_eq!(ctx.items.len(), 1);
        let another_added = ctx.add(ModelSegment {
            source: Source::Model,
            content: "response".to_string(),
        });
        // Verify the second addition
        assert_eq!(another_added.content, "response");
        assert_eq!(ctx.items.len(), 2);
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