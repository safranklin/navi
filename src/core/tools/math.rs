//! # Math Tools
//!
//! Math-related tool implementations.
//! Each tool is a unit struct implementing the `Tool` trait.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Tool, ToolError};

// ── Add ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MathOperationType {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

pub struct MathOperation;

#[derive(Deserialize, JsonSchema)]
pub struct MathOperationArgs {
    /// Operation to perform, e.g. "add", "subtract", "multiply", "divide"
    pub operation: MathOperationType,
    /// left operand
    pub a: f64,
    /// right operand
    pub b: f64,
}

#[derive(Debug, Serialize)]
pub struct MathOperationOutput {
    pub result: f64,
}

#[async_trait]
impl Tool for MathOperation {
    const NAME: &'static str = "math_operation";
    const DESCRIPTION: &'static str = "Computes a mathematical operation (defined by the `operation` parameter) on two numbers. result = a {operation} b. Produces a deterministic result or an error if the operation is invalid.";
    type Args = MathOperationArgs;
    type Output = MathOperationOutput;

    async fn call(&self, args: MathOperationArgs) -> Result<MathOperationOutput, ToolError> {
        let result = match args.operation {
            MathOperationType::Add => args.a + args.b,
            MathOperationType::Subtract => args.a - args.b,
            MathOperationType::Multiply => args.a * args.b,
            MathOperationType::Divide => {
                if args.b == 0.0 {
                    return Err(ToolError("Division by zero".into()));
                }
                args.a / args.b
            }
            MathOperationType::Power => args.a.powf(args.b),
        };

        if result.is_nan() {
            return Err(ToolError(format!(
                "Result is undefined (NaN) for {} {:?} {}",
                args.a, args.operation, args.b
            )));
        }
        if result.is_infinite() {
            return Err(ToolError(format!(
                "Result overflowed to infinity for {} {:?} {}",
                args.a, args.operation, args.b
            )));
        }

        Ok(MathOperationOutput { result })
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to call with typed args directly.
    async fn run(op: MathOperationType, a: f64, b: f64) -> f64 {
        MathOperation
            .call(MathOperationArgs {
                operation: op,
                a,
                b,
            })
            .await
            .unwrap()
            .result
    }

    /// Helper for error cases.
    async fn run_err(op: MathOperationType, a: f64, b: f64) -> ToolError {
        MathOperation
            .call(MathOperationArgs {
                operation: op,
                a,
                b,
            })
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn add() {
        assert_eq!(run(MathOperationType::Add, 3.0, 7.0).await, 10.0);
    }

    #[tokio::test]
    async fn subtract() {
        assert_eq!(run(MathOperationType::Subtract, 10.0, 4.0).await, 6.0);
    }

    #[tokio::test]
    async fn multiply() {
        assert_eq!(run(MathOperationType::Multiply, 3.0, 5.0).await, 15.0);
    }

    #[tokio::test]
    async fn divide() {
        assert_eq!(run(MathOperationType::Divide, 10.0, 4.0).await, 2.5);
    }

    #[tokio::test]
    async fn divide_by_zero_returns_error() {
        let err = run_err(MathOperationType::Divide, 1.0, 0.0).await;
        assert!(err.0.contains("Division by zero"), "got: {err}");
    }

    #[tokio::test]
    async fn nan_returns_error() {
        let err = run_err(MathOperationType::Power, -1.0, 0.5).await;
        assert!(err.0.contains("NaN"), "got: {err}");
    }

    #[tokio::test]
    async fn overflow_returns_error() {
        let err = run_err(MathOperationType::Power, f64::MAX, 2.0).await;
        assert!(err.0.contains("infinity"), "got: {err}");
    }

    #[tokio::test]
    async fn power() {
        assert_eq!(run(MathOperationType::Power, 2.0, 10.0).await, 1024.0);
    }

    #[tokio::test]
    async fn power_fractional_exponent() {
        let result = run(MathOperationType::Power, 9.0, 0.5).await;
        assert!((result - 3.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn negative_operands() {
        assert_eq!(run(MathOperationType::Add, -3.0, -7.0).await, -10.0);
        assert_eq!(run(MathOperationType::Multiply, -3.0, 5.0).await, -15.0);
    }

    #[tokio::test]
    async fn operation_deserializes_from_json() {
        let json = r#"{"operation": "add", "a": 1, "b": 2}"#;
        let args: MathOperationArgs = serde_json::from_str(json).unwrap();
        let result = MathOperation.call(args).await.unwrap();
        assert_eq!(result.result, 3.0);
    }

    #[tokio::test]
    async fn all_operations_deserialize() {
        for op in ["add", "subtract", "multiply", "divide", "power"] {
            let json = format!(r#"{{"operation": "{op}", "a": 2, "b": 3}}"#);
            let args: MathOperationArgs = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("failed to deserialize '{op}': {e}"));
            MathOperation.call(args).await.unwrap();
        }
    }

    #[test]
    fn invalid_operation_fails_deserialization() {
        let json = r#"{"operation": "modulo", "a": 1, "b": 2}"#;
        assert!(serde_json::from_str::<MathOperationArgs>(json).is_err());
    }
}
