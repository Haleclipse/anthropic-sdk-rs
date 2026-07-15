// Maps to: TS helpers/beta/

pub mod json_schema;
pub mod mcp;
pub mod memory;
pub mod schemars;

// TS-style helper names from helpers/beta/json-schema.ts.
pub use json_schema::{
    betaJSONSchemaOutputFormat, betaJSONSchemaOutputFormatWithOptions, betaTool,
};

// TS-style helper names from helpers/beta/mcp.ts and helpers/beta/memory.ts.
pub use mcp::{
    collectStainlessHelpers, mcpContent, mcpMessage, mcpMessages, mcpResourceToContent,
    mcpResourceToFile, mcpTool, mcpTools, stainlessHelperHeader, MCPAudioContentLike,
    MCPEmbeddedResourceLike, MCPImageContentLike, MCPResourceLinkLike, MCPTextContentLike,
    SDK_HELPER_SYMBOL,
};
pub use memory::{betaMemoryTool, MemoryToolHandlers};

// Rust-native equivalents of TS helpers/beta/zod.ts. We intentionally expose
// these under `beta_schemars_*` names instead of `betaZod*` because Rust
// validation is based on `schemars::JsonSchema` + `serde`.
pub use schemars::{
    beta_schemars_output_config, beta_schemars_output_config_with_options,
    beta_schemars_output_format, beta_schemars_output_format_with_options,
    beta_schemars_parse_output, beta_schemars_tool, BetaSchemarsTool, BetaSchemarsToolOptions,
};
