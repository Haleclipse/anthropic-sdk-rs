// Maps to: TS lib/tools/
//
// Tool execution infrastructure: error type, runnable-tool trait, the
// iterative tool-runner loop, and beta-specific extensions.

pub mod beta_runnable_tool;
pub mod compaction_control;
pub mod tool_error;
pub mod tool_runner;

pub use beta_runnable_tool::{BetaClientRunnableToolType, BetaRunnableTool, Promisable};
pub use compaction_control::{CompactionControl, DEFAULT_SUMMARY_PROMPT, DEFAULT_TOKEN_THRESHOLD};
pub use tool_error::ToolError;
pub use tool_runner::{
    BetaMessageCreateClient, BetaToolRunner, BetaToolRunnerParams, BetaToolRunnerRequestOptions,
    RunnableTool, ToolRunner, ToolRunnerParams,
};
