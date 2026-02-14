//! # Tool Executor
//!
//! Executes tool calls requested by the model. Each tool is a simple async function
//! that takes arguments (JSON string) and returns output (JSON string).
//!
//! Tools are registered in `available()` and dispatched by name in `execute()`.

use crate::inference::types::{ToolCall, ToolDefinition};

/// Executes a tool call and returns the output as a string.
pub async fn execute(tool_call: &ToolCall) -> String {
    match tool_call.name.as_str() {
        "add" => {
            #[derive(serde::Deserialize)]
            struct Args { a: i64, b: i64 }

            match serde_json::from_str::<Args>(&tool_call.arguments) {
                Ok(args) => serde_json::json!({ "result": args.a + args.b }).to_string(),
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            }
        }
        _ => serde_json::json!({ "error": format!("Unknown tool: {}", tool_call.name) }).to_string(),
    }
}

/// Returns the list of tool definitions available to the model.
pub fn available() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "add".into(),
            description: "Adds two integers and returns their sum. Use this tool to perform addition using a calculator.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "a": { "type": "integer", "description": "First number" },
                    "b": { "type": "integer", "description": "Second number" }
                },
                "required": ["a", "b"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_tool() {
        let tc = ToolCall {
            id: "fc_1".into(),
            call_id: "call_1".into(),
            name: "add".into(),
            arguments: r#"{"a": 3, "b": 7}"#.into(),
        };
        let result = execute(&tc).await;
        assert_eq!(result, r#"{"result":10}"#);
    }

    #[tokio::test]
    async fn test_add_tool_bad_args() {
        let tc = ToolCall {
            id: "fc_2".into(),
            call_id: "call_2".into(),
            name: "add".into(),
            arguments: r#"{"a": "not a number"}"#.into(),
        };
        let result = execute(&tc).await;
        assert!(result.contains("error"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let tc = ToolCall {
            id: "fc_3".into(),
            call_id: "call_3".into(),
            name: "nonexistent".into(),
            arguments: "{}".into(),
        };
        let result = execute(&tc).await;
        assert!(result.contains("Unknown tool"));
    }

    #[test]
    fn test_available_includes_add() {
        let tools = available();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "add");
    }
}
