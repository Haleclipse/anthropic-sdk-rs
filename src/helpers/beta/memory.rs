// Maps to: TS helpers/beta/memory.ts
//
// Rust equivalent of `betaMemoryTool(handlers)`. The TypeScript helper accepts
// Promisable handlers returning string or structured content. Rust stores
// handlers as `BetaToolResultContent` producers while keeping ergonomic
// text-only builder methods for the common case.

use std::sync::Arc;

use crate::resources::beta::messages::types::{
    BetaMemoryTool20250818, BetaMemoryTool20250818Command, BetaMemoryTool20250818CreateCommand,
    BetaMemoryTool20250818DeleteCommand, BetaMemoryTool20250818InsertCommand,
    BetaMemoryTool20250818RenameCommand, BetaMemoryTool20250818StrReplaceCommand,
    BetaMemoryTool20250818ViewCommand, BetaToolResultContent,
};
use crate::sdk_lib::tools::{BetaRunnableTool, RunnableTool, ToolError};

pub type MemoryHandler<C> =
    Arc<dyn Fn(C) -> Result<BetaToolResultContent, ToolError> + Send + Sync>;

/// Maps to TS `MemoryToolHandlers`.
#[derive(Clone, Default)]
pub struct BetaMemoryToolHandlers {
    pub view: Option<MemoryHandler<BetaMemoryTool20250818ViewCommand>>,
    pub create: Option<MemoryHandler<BetaMemoryTool20250818CreateCommand>>,
    pub str_replace: Option<MemoryHandler<BetaMemoryTool20250818StrReplaceCommand>>,
    pub insert: Option<MemoryHandler<BetaMemoryTool20250818InsertCommand>>,
    pub delete: Option<MemoryHandler<BetaMemoryTool20250818DeleteCommand>>,
    pub rename: Option<MemoryHandler<BetaMemoryTool20250818RenameCommand>>,
}

impl BetaMemoryToolHandlers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_view(
        self,
        handler: impl Fn(BetaMemoryTool20250818ViewCommand) -> Result<String, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.with_view_content(move |cmd| handler(cmd).map(BetaToolResultContent::Text))
    }

    pub fn with_view_content(
        mut self,
        handler: impl Fn(BetaMemoryTool20250818ViewCommand) -> Result<BetaToolResultContent, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.view = Some(Arc::new(handler));
        self
    }

    pub fn with_create(
        self,
        handler: impl Fn(BetaMemoryTool20250818CreateCommand) -> Result<String, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.with_create_content(move |cmd| handler(cmd).map(BetaToolResultContent::Text))
    }

    pub fn with_create_content(
        mut self,
        handler: impl Fn(BetaMemoryTool20250818CreateCommand) -> Result<BetaToolResultContent, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.create = Some(Arc::new(handler));
        self
    }

    pub fn with_str_replace(
        self,
        handler: impl Fn(BetaMemoryTool20250818StrReplaceCommand) -> Result<String, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.with_str_replace_content(move |cmd| handler(cmd).map(BetaToolResultContent::Text))
    }

    pub fn with_str_replace_content(
        mut self,
        handler: impl Fn(BetaMemoryTool20250818StrReplaceCommand) -> Result<BetaToolResultContent, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.str_replace = Some(Arc::new(handler));
        self
    }

    pub fn with_insert(
        self,
        handler: impl Fn(BetaMemoryTool20250818InsertCommand) -> Result<String, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.with_insert_content(move |cmd| handler(cmd).map(BetaToolResultContent::Text))
    }

    pub fn with_insert_content(
        mut self,
        handler: impl Fn(BetaMemoryTool20250818InsertCommand) -> Result<BetaToolResultContent, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.insert = Some(Arc::new(handler));
        self
    }

    pub fn with_delete(
        self,
        handler: impl Fn(BetaMemoryTool20250818DeleteCommand) -> Result<String, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.with_delete_content(move |cmd| handler(cmd).map(BetaToolResultContent::Text))
    }

    pub fn with_delete_content(
        mut self,
        handler: impl Fn(BetaMemoryTool20250818DeleteCommand) -> Result<BetaToolResultContent, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.delete = Some(Arc::new(handler));
        self
    }

    pub fn with_rename(
        self,
        handler: impl Fn(BetaMemoryTool20250818RenameCommand) -> Result<String, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.with_rename_content(move |cmd| handler(cmd).map(BetaToolResultContent::Text))
    }

    pub fn with_rename_content(
        mut self,
        handler: impl Fn(BetaMemoryTool20250818RenameCommand) -> Result<BetaToolResultContent, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.rename = Some(Arc::new(handler));
        self
    }
}

/// TS-style alias for [`BetaMemoryToolHandlers`].
pub type MemoryToolHandlers = BetaMemoryToolHandlers;

/// Runnable memory tool returned by [`beta_memory_tool`].
pub struct BetaMemoryRunnableTool {
    definition: BetaMemoryTool20250818,
    handlers: BetaMemoryToolHandlers,
}

/// Maps to TS `betaMemoryTool(handlers)`.
pub fn beta_memory_tool(handlers: BetaMemoryToolHandlers) -> BetaMemoryRunnableTool {
    BetaMemoryRunnableTool {
        definition: BetaMemoryTool20250818::default(),
        handlers,
    }
}

/// TS-style camelCase alias for [`beta_memory_tool`].
#[allow(non_snake_case)]
pub fn betaMemoryTool(handlers: MemoryToolHandlers) -> BetaMemoryRunnableTool {
    beta_memory_tool(handlers)
}

impl BetaMemoryRunnableTool {
    pub fn tool_definition(&self) -> &BetaMemoryTool20250818 {
        &self.definition
    }

    fn missing_handler(command: &str) -> ToolError {
        ToolError::new(format!(
            "No handler configured for memory command `{command}`"
        ))
    }

    fn run_command(
        &self,
        command: BetaMemoryTool20250818Command,
    ) -> Result<BetaToolResultContent, ToolError> {
        match command {
            BetaMemoryTool20250818Command::View(cmd) => {
                self.handlers
                    .view
                    .as_ref()
                    .ok_or_else(|| Self::missing_handler("view"))?(cmd)
            }
            BetaMemoryTool20250818Command::Create(cmd) => {
                self.handlers
                    .create
                    .as_ref()
                    .ok_or_else(|| Self::missing_handler("create"))?(cmd)
            }
            BetaMemoryTool20250818Command::StrReplace(cmd) => {
                self.handlers
                    .str_replace
                    .as_ref()
                    .ok_or_else(|| Self::missing_handler("str_replace"))?(cmd)
            }
            BetaMemoryTool20250818Command::Insert(cmd) => {
                self.handlers
                    .insert
                    .as_ref()
                    .ok_or_else(|| Self::missing_handler("insert"))?(cmd)
            }
            BetaMemoryTool20250818Command::Delete(cmd) => {
                self.handlers
                    .delete
                    .as_ref()
                    .ok_or_else(|| Self::missing_handler("delete"))?(cmd)
            }
            BetaMemoryTool20250818Command::Rename(cmd) => {
                self.handlers
                    .rename
                    .as_ref()
                    .ok_or_else(|| Self::missing_handler("rename"))?(cmd)
            }
        }
    }
}

#[async_trait::async_trait]
impl RunnableTool for BetaMemoryRunnableTool {
    fn name(&self) -> &str {
        "memory"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::to_value(&self.definition)
            .unwrap_or_else(|_| serde_json::json!({"type": "memory_20250818", "name": "memory"}))
    }

    fn parse(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let command: BetaMemoryTool20250818Command = serde_json::from_value(input)
            .map_err(|err| ToolError::new(format!("failed to parse memory command: {err}")))?;
        serde_json::to_value(command)
            .map_err(|err| ToolError::new(format!("failed to serialize memory command: {err}")))
    }

    async fn run_beta_tool_result_content(
        &self,
        input: serde_json::Value,
    ) -> Result<BetaToolResultContent, ToolError> {
        let parsed = <Self as RunnableTool>::parse(self, input).map_err(|err| {
            ToolError::with_content(
                format!("failed to parse memory command: {err}"),
                format!("Invalid memory command: {err}"),
            )
        })?;
        let command: BetaMemoryTool20250818Command = serde_json::from_value(parsed)
            .map_err(|err| ToolError::new(format!("failed to parse memory command: {err}")))?;
        self.run_command(command)
    }

    async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
        match self.run_beta_tool_result_content(input).await? {
            BetaToolResultContent::Text(text) => Ok(text),
            BetaToolResultContent::Blocks(blocks) => {
                serde_json::to_string(&blocks).map_err(|err| {
                    ToolError::new(format!(
                        "Failed to serialize structured memory tool result: {err}"
                    ))
                })
            }
        }
    }
}

impl BetaRunnableTool for BetaMemoryRunnableTool {}
