// Maps to: TS helpers/beta/zod.ts, using Rust's `schemars` + `serde` stack.
//
//! Rust equivalent of beta Zod helpers.
//!
//! Public Rust APIs intentionally use `schemars` naming instead of `zod`:
//! - schema generation uses `schemars::JsonSchema` on Rust types
//! - validation/parsing uses `serde::Deserialize`
//! - output formats are serializable API payloads; parsing is performed by
//!   [`beta_schemars_parse_output`] or `sdk_lib::beta_parser::parse_message::<T>()`
//! - beta tool handlers can return text or structured beta tool-result content
//!   blocks, matching TS beta runnable tools

use std::marker::PhantomData;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::core::error::ApiError;
use crate::helpers::json_schema::JsonSchemaOutputFormatOptions;
use crate::helpers::schemars::{schemars_output_format_with_options, schemars_parse_output};
use crate::resources::beta::messages::BetaToolResultContent;
use crate::resources::messages::{JsonOutputFormat, OutputConfig};
use crate::sdk_lib::tools::{BetaRunnableTool, RunnableTool, ToolError};

/// Rust equivalent of TS `betaZodOutputFormat(zodObject)`.
pub fn beta_schemars_output_format<T>() -> Result<JsonOutputFormat, ApiError>
where
    T: JsonSchema,
{
    beta_schemars_output_format_with_options::<T>(JsonSchemaOutputFormatOptions::default())
}

/// Same as [`beta_schemars_output_format`] with explicit transform options.
pub fn beta_schemars_output_format_with_options<T>(
    options: JsonSchemaOutputFormatOptions,
) -> Result<JsonOutputFormat, ApiError>
where
    T: JsonSchema,
{
    schemars_output_format_with_options::<T>(options)
}

/// Convenience wrapper returning an [`OutputConfig`] for beta message params.
pub fn beta_schemars_output_config<T>() -> Result<OutputConfig, ApiError>
where
    T: JsonSchema,
{
    beta_schemars_output_config_with_options::<T>(JsonSchemaOutputFormatOptions::default())
}

/// Same as [`beta_schemars_output_config`] with explicit transform options.
pub fn beta_schemars_output_config_with_options<T>(
    options: JsonSchemaOutputFormatOptions,
) -> Result<OutputConfig, ApiError>
where
    T: JsonSchema,
{
    Ok(OutputConfig {
        effort: None,
        format: Some(beta_schemars_output_format_with_options::<T>(options)?),
    })
}

/// Parses structured-output text into `T` using serde.
pub fn beta_schemars_parse_output<T>(content: &str) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    schemars_parse_output(content)
}

/// Options for [`beta_schemars_tool`].
pub struct BetaSchemarsToolOptions<T> {
    pub name: String,
    pub description: String,
    pub run: Arc<dyn Fn(T) -> Result<BetaToolResultContent, ToolError> + Send + Sync>,
}

impl<T> BetaSchemarsToolOptions<T> {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        run: impl Fn(T) -> Result<String, ToolError> + Send + Sync + 'static,
    ) -> Self {
        Self::new_content(name, description, move |input| {
            run(input).map(BetaToolResultContent::Text)
        })
    }

    pub fn new_content(
        name: impl Into<String>,
        description: impl Into<String>,
        run: impl Fn(T) -> Result<BetaToolResultContent, ToolError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            run: Arc::new(run),
        }
    }
}

/// Runnable tool returned by [`beta_schemars_tool`].
pub struct BetaSchemarsTool<T> {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    run: Arc<dyn Fn(T) -> Result<BetaToolResultContent, ToolError> + Send + Sync>,
    _marker: PhantomData<T>,
}

/// Rust equivalent of TS `betaZodTool({ name, inputSchema, description, run })`.
///
/// The Rust equivalent takes `T` as the schema/input type. `T` must derive
/// `JsonSchema` and `Deserialize`; tool input is deserialized before invoking
/// the handler.
pub fn beta_schemars_tool<T>(
    options: BetaSchemarsToolOptions<T>,
) -> Result<BetaSchemarsTool<T>, ApiError>
where
    T: JsonSchema + DeserializeOwned + Send + Sync + 'static,
{
    let format = beta_schemars_output_format_with_options::<T>(JsonSchemaOutputFormatOptions {
        transform: false,
    })?;
    let schema_type = format
        .schema
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if schema_type != "object" {
        return Err(ApiError::Sdk(format!(
            "schemars schema for tool \"{}\" must be an object, but got {schema_type}",
            options.name
        )));
    }

    Ok(BetaSchemarsTool {
        name: options.name,
        description: options.description,
        input_schema: format.schema,
        run: options.run,
        _marker: PhantomData,
    })
}

#[async_trait::async_trait]
impl<T> RunnableTool for BetaSchemarsTool<T>
where
    T: JsonSchema + DeserializeOwned + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "custom",
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema,
        })
    }

    fn parse(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        serde_json::from_value::<T>(input.clone())
            .map_err(|err| ToolError::new(format!("Failed to parse tool input: {err}")))?;
        Ok(input)
    }

    async fn run_beta_tool_result_content(
        &self,
        input: serde_json::Value,
    ) -> Result<BetaToolResultContent, ToolError> {
        let parsed: T = serde_json::from_value(input).map_err(|err| {
            ToolError::with_content(
                format!("Failed to parse tool input: {err}"),
                format!("Invalid tool input: {err}"),
            )
        })?;
        (self.run)(parsed)
    }

    async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
        match self.run_beta_tool_result_content(input).await? {
            BetaToolResultContent::Text(text) => Ok(text),
            BetaToolResultContent::Blocks(blocks) => {
                serde_json::to_string(&blocks).map_err(|err| {
                    ToolError::new(format!("Failed to serialize structured tool result: {err}"))
                })
            }
        }
    }
}

impl<T> BetaRunnableTool for BetaSchemarsTool<T> where
    T: JsonSchema + DeserializeOwned + Send + Sync + 'static
{
}
