//! # Tool System
//!
//! Trait-based tool framework using schemars for automatic JSON Schema generation.
//! Each tool is a struct that implements the `Tool` trait, which ties together
//! name, description, args, output, and execution logic in one impl block.
//!
//! //! `DynTool` is the object-safe bridge trait that enables dynamic dispatch despite
//! `Tool` having associated types. A blanket impl converts any `T: Tool` into `dyn DynTool`.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::inference::types::{ToolCall, ToolDefinition};

// ── Error type ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ToolError {}

// ── Tool trait ──────────────────────────────────────────────────────────────

#[async_trait]
pub trait Tool: Send + Sync {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;

    type Args: for<'de> Deserialize<'de> + JsonSchema + Send;
    type Output: Serialize;

    async fn call(&self, args: Self::Args) -> Result<Self::Output, ToolError>;
}

// ── Type-erased bridge ──────────────────────────────────────────────────────

#[async_trait]
trait DynTool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, args_json: &str) -> String;
}

#[async_trait]
impl<T: Tool> DynTool for T {
    fn definition(&self) -> ToolDefinition {
        let schema = schemars::schema_for!(T::Args);
        ToolDefinition {
            name: T::NAME.into(),
            description: T::DESCRIPTION.into(),
            parameters: serde_json::to_value(schema).unwrap(),
        }
    }

    async fn execute(&self, args_json: &str) -> String {
        match serde_json::from_str::<T::Args>(args_json) {
            Ok(args) => match self.call(args).await {
                Ok(output) => serde_json::to_string(&output).unwrap(),
                Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
            },
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }
}

// ── Registry ────────────────────────────────────────────────────────────────

pub struct ToolRegistry {
    tools: Vec<Box<dyn DynTool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.push(Box::new(tool));
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    pub async fn execute(&self, tool_call: &ToolCall) -> String {
        for tool in &self.tools {
            let def = tool.definition();
            if def.name == tool_call.name {
                return tool.execute(&tool_call.arguments).await;
            }
        }
        serde_json::json!({ "error": format!("Unknown tool: {}", tool_call.name) }).to_string()
    }
}

/// Creates a registry with all built-in tools.
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(AddTool);
    registry
}

// ── Built-in tools ──────────────────────────────────────────────────────────

pub struct AddTool;

#[derive(Deserialize, JsonSchema)]
pub struct AddArgs {
    /// First number
    a: i64,
    /// Second number
    b: i64,
}

#[derive(Serialize)]
pub struct AddOutput {
    result: i64,
}

#[async_trait]
impl Tool for AddTool {
    const NAME: &'static str = "add";
    const DESCRIPTION: &'static str = "Adds two integers and returns their sum.";
    type Args = AddArgs;
    type Output = AddOutput;

    async fn call(&self, args: AddArgs) -> Result<AddOutput, ToolError> {
        Ok(AddOutput { result: args.a + args.b })
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_tool_direct() {
        let result = AddTool.call(AddArgs { a: 3, b: 7 }).await.unwrap();
        assert_eq!(result.result, 10);
    }

    #[tokio::test]
    async fn test_registry_execute() {
        let registry = default_registry();
        let tc = ToolCall {
            id: "fc_1".into(),
            call_id: "call_1".into(),
            name: "add".into(),
            arguments: r#"{"a": 3, "b": 7}"#.into(),
        };
        let result = registry.execute(&tc).await;
        assert_eq!(result, r#"{"result":10}"#);
    }

    #[tokio::test]
    async fn test_registry_bad_args() {
        let registry = default_registry();
        let tc = ToolCall {
            id: "fc_2".into(),
            call_id: "call_2".into(),
            name: "add".into(),
            arguments: r#"{"a": "not a number"}"#.into(),
        };
        let result = registry.execute(&tc).await;
        assert!(result.contains("error"));
    }

    #[tokio::test]
    async fn test_registry_unknown_tool() {
        let registry = default_registry();
        let tc = ToolCall {
            id: "fc_3".into(),
            call_id: "call_3".into(),
            name: "nonexistent".into(),
            arguments: "{}".into(),
        };
        let result = registry.execute(&tc).await;
        assert!(result.contains("Unknown tool"));
    }

    #[test]
    fn test_definitions_include_add() {
        let registry = default_registry();
        let defs = registry.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "add");
    }

    #[test]
    fn test_schema_has_properties_and_required() {
        let registry = default_registry();
        let defs = registry.definitions();
        let params = &defs[0].parameters;
        let props = params.get("properties").expect("should have properties");
        assert!(props.get("a").is_some());
        assert!(props.get("b").is_some());
        let required = params.get("required").expect("should have required");
        let required_arr: Vec<String> = serde_json::from_value(required.clone()).unwrap();
        assert!(required_arr.contains(&"a".to_string()));
        assert!(required_arr.contains(&"b".to_string()));
    }
}
