//! # Tool System
//!
//! Trait-based tool framework using schemars for automatic JSON Schema generation.
//! Each tool is a struct that implements the `Tool` trait, which ties together
//! name, description, args, output, and execution logic in one impl block.
//!
//! `DynTool` is the object-safe bridge trait that enables dynamic dispatch despite
//! `Tool` having associated types. A blanket impl converts any `T: Tool` into `dyn DynTool`.

pub mod bash;
pub mod io;
pub mod math;
pub mod permission;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::inference::types::{ToolCall, ToolDefinition};
pub use permission::ToolPermission;

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
    const PERMISSION: ToolPermission = ToolPermission::Safe;

    type Args: for<'de> Deserialize<'de> + JsonSchema + Send;
    type Output: Serialize;

    async fn call(&self, args: Self::Args) -> Result<Self::Output, ToolError>;
}

// ── Type-erased bridge ──────────────────────────────────────────────────────

#[async_trait]
trait DynTool: Send + Sync {
    fn name(&self) -> &'static str;
    fn permission(&self) -> ToolPermission;
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, args_json: &str) -> String;
}

#[async_trait]
impl<T: Tool> DynTool for T {
    fn name(&self) -> &'static str {
        T::NAME
    }

    fn permission(&self) -> ToolPermission {
        T::PERMISSION
    }

    fn definition(&self) -> ToolDefinition {
        let schema = schemars::schema_for!(T::Args);
        ToolDefinition {
            name: T::NAME.into(),
            description: T::DESCRIPTION.into(),
            parameters: serde_json::to_value(schema).unwrap_or_else(
                |e| serde_json::json!({"error": format!("Schema generation failed: {e}")}),
            ),
        }
    }

    async fn execute(&self, args_json: &str) -> String {
        match serde_json::from_str::<T::Args>(args_json) {
            Ok(args) => match self.call(args).await {
                Ok(output) => serde_json::to_string(&output).unwrap_or_else(|e| {
                    serde_json::json!({"error": format!("Serialization failed: {e}")}).to_string()
                }),
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

    /// Returns the permission level for a tool by name, or None if unknown.
    pub fn permission(&self, name: &str) -> Option<ToolPermission> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.permission())
    }

    pub async fn execute(&self, tool_call: &ToolCall) -> String {
        for tool in &self.tools {
            if tool.name() == tool_call.name {
                return tool.execute(&tool_call.arguments).await;
            }
        }
        serde_json::json!({ "error": format!("Unknown tool: {}", tool_call.name) }).to_string()
    }
}

/// Creates a registry with all built-in tools.
pub fn default_registry() -> ToolRegistry {
    use crate::core::sandbox::{DockerSandbox, LocalSandbox};
    use std::sync::Arc;

    let mut registry = ToolRegistry::new();
    registry.register(math::MathOperation);
    registry.register(io::ReadFileTool);

    // BashTool with Docker or local fallback
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let max_output = 100_000;

    let sandbox: Arc<dyn crate::core::sandbox::Sandbox> = if DockerSandbox::is_available() {
        Arc::new(DockerSandbox::new(cwd, "ubuntu:24.04", max_output))
    } else {
        log::warn!("Docker not found, using local sandbox (no isolation)");
        Arc::new(LocalSandbox::new(cwd, max_output))
    };

    registry.register(bash::BashTool::new(
        sandbox,
        std::time::Duration::from_secs(120),
    ));

    registry
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_execute() {
        let registry = default_registry();
        let tc = ToolCall {
            id: "fc_1".into(),
            call_id: "call_1".into(),
            name: "math_operation".into(),
            arguments: r#"{"operation": "add", "a": 3, "b": 7}"#.into(),
        };
        let result = registry.execute(&tc).await;
        assert_eq!(result, r#"{"result":10.0}"#);
    }

    #[tokio::test]
    async fn test_registry_bad_args() {
        let registry = default_registry();
        let tc = ToolCall {
            id: "fc_2".into(),
            call_id: "call_2".into(),
            name: "math_operation".into(),
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
    fn test_definitions_lists_all_tools() {
        let registry = default_registry();
        let defs = registry.definitions();
        assert_eq!(defs.len(), 3);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"math_operation"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"bash"));
    }

    #[test]
    fn test_permission_lookup() {
        let registry = default_registry();
        assert_eq!(
            registry.permission("math_operation"),
            Some(ToolPermission::Safe)
        );
        assert_eq!(registry.permission("read_file"), Some(ToolPermission::Safe));
        assert_eq!(registry.permission("bash"), Some(ToolPermission::Prompt));
        assert_eq!(registry.permission("nonexistent"), None);
    }

    #[test]
    fn test_math_schema_has_properties_and_required() {
        let registry = default_registry();
        let defs = registry.definitions();
        let math_def = defs.iter().find(|d| d.name == "math_operation").unwrap();
        let params = &math_def.parameters;
        let props = params.get("properties").expect("should have properties");
        assert!(props.get("operation").is_some());
        assert!(props.get("a").is_some());
        assert!(props.get("b").is_some());
        let required = params.get("required").expect("should have required");
        let required_arr: Vec<String> = serde_json::from_value(required.clone()).unwrap();
        assert!(required_arr.contains(&"operation".to_string()));
        assert!(required_arr.contains(&"a".to_string()));
        assert!(required_arr.contains(&"b".to_string()));
    }
}
