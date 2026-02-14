//! # Arithmetic Tools
//!
//! Math-related tool implementations.
//! Each tool is a unit struct implementing the `Tool` trait.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Tool, ToolError};

// ── Add ─────────────────────────────────────────────────────────────────────

pub struct AddTool;

#[derive(Deserialize, JsonSchema)]
pub struct AddArgs {
    /// First number
    pub a: f64,
    /// Second number
    pub b: f64,
}

#[derive(Serialize)]
pub struct AddOutput {
    pub result: f64,
}

#[async_trait]
impl Tool for AddTool {
    const NAME: &'static str = "add";
    const DESCRIPTION: &'static str = "Adds two numbers and returns their sum.";
    type Args = AddArgs;
    type Output = AddOutput;

    async fn call(&self, args: AddArgs) -> Result<AddOutput, ToolError> {
        Ok(AddOutput { result: args.a + args.b })
    }
}

// ── Subtract ────────────────────────────────────────────────────────────────

pub struct SubtractTool;

#[derive(Deserialize, JsonSchema)]
pub struct SubtractArgs {
    /// Number to subtract from
    pub a: f64,
    /// Number to subtract
    pub b: f64,
}

#[async_trait]
impl Tool for SubtractTool {
    const NAME: &'static str = "subtract";
    const DESCRIPTION: &'static str = "Subtracts the second number from the first.";
    type Args = SubtractArgs;
    type Output = AddOutput;

    async fn call(&self, args: SubtractArgs) -> Result<AddOutput, ToolError> {
        Ok(AddOutput { result: args.a - args.b })
    }
}

// ── Multiply ────────────────────────────────────────────────────────────────

pub struct MultiplyTool;

#[derive(Deserialize, JsonSchema)]
pub struct MultiplyArgs {
    /// First factor
    pub a: f64,
    /// Second factor
    pub b: f64,
}

#[async_trait]
impl Tool for MultiplyTool {
    const NAME: &'static str = "multiply";
    const DESCRIPTION: &'static str = "Multiplies two numbers.";
    type Args = MultiplyArgs;
    type Output = AddOutput;

    async fn call(&self, args: MultiplyArgs) -> Result<AddOutput, ToolError> {
        Ok(AddOutput { result: args.a * args.b })
    }
}

// ── Divide ──────────────────────────────────────────────────────────────────

pub struct DivideTool;

#[derive(Deserialize, JsonSchema)]
pub struct DivideArgs {
    /// Dividend
    pub a: f64,
    /// Divisor
    pub b: f64,
}

#[derive(Serialize)]
pub struct DivideOutput {
    pub result: f64
}

#[async_trait]
impl Tool for DivideTool {
    const NAME: &'static str = "divide";
    const DESCRIPTION: &'static str = "Divides the first number by the second.";
    type Args = DivideArgs;
    type Output = DivideOutput;

    async fn call(&self, args: DivideArgs) -> Result<DivideOutput, ToolError> {
        if args.b == 0.0 {
            return Err(ToolError("Division by zero".into()));
        }
        Ok(DivideOutput {
            result: args.a / args.b,
        })
    }
}
