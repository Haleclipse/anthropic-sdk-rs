// Maps to: TS lib/tools/BetaRunnableTool.ts
//
// Beta-specific extension of the `RunnableTool` trait that adds a `parse`
// method for pre-processing tool input before execution.
//
// In the TypeScript SDK, `BetaRunnableTool<Input>` extends the base tool
// type with `run` and `parse` methods. In Rust we model this as a sub-trait
// of `RunnableTool` with a default `parse` implementation that passes the
// input through unchanged.

use std::future::Future;
use std::pin::Pin;

use super::tool_error::ToolError;

/// Rust future equivalent of TS `Promisable<T>`.
pub type Promisable<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Maps to: TS `BetaRunnableTool<Input>` -- a tool with optional input parsing
///
/// Extends [`RunnableTool`](super::tool_runner::RunnableTool) with a
/// [`parse`](BetaRunnableTool::parse) method that can validate and
/// transform the raw JSON input before it reaches
/// [`run`](super::tool_runner::RunnableTool::run).
///
/// The default implementation passes the input through unchanged (identity
/// transform), so implementors only need to override `parse` when they
/// require custom deserialization or validation.
pub trait BetaRunnableTool: super::tool_runner::RunnableTool {
    /// Parse and optionally transform the raw JSON input before execution.
    ///
    /// The default implementation delegates to [`RunnableTool::parse`], which
    /// is an identity transform unless implementors override it.
    ///
    /// Maps to: TS `BetaRunnableTool.parse`
    fn parse(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        super::tool_runner::RunnableTool::parse(self, input)
    }
}

/// Rust trait-object equivalent of TS `BetaClientRunnableToolType`.
pub type BetaClientRunnableToolType = dyn BetaRunnableTool;
