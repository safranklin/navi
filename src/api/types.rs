use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Role {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Model,
    #[serde(rename = "system")]
    Directive,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Take a chat message, if the role is not user, for now we will assume it's from navi. This should be the case, for now.
        let role_str = match self.role {
            Role::User => "user",
            Role::Model => "navi",
            Role::Directive => "system",
        };

        let content = &self.content;
        write!(f, "{}> {}", role_str, content)
    }
}

#[derive(Serialize, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Deserialize, Debug)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
pub struct Choice {
    pub message: ChatMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_display() {
        let msg = ChatMessage {
            role: Role::User,
            content: "hello".to_string(),
        };
        assert_eq!(msg.to_string(), "user> hello");
    }

    #[test]
    fn test_chat_message_display_model() {
        let msg = ChatMessage {
            role: Role::Model,
            content: "hi there".to_string(),
        };
        assert_eq!(msg.to_string(), "navi> hi there");
    }

    #[test]
    fn test_chat_message_display_directive() {
        let msg = ChatMessage {
            role: Role::Directive,
            content: "system message".to_string(),
        };
        assert_eq!(msg.to_string(), "system> system message");
    }
}