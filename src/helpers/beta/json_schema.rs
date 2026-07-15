// Maps to: TS helpers/beta/json-schema.ts
//
// Beta structured-output helpers. TypeScript returns an AutoParseable format
// object with a JS parse closure; Rust uses the same serializable JSON schema
// format and parses response text via serde (`sdk_lib::beta_parser`).

use std::sync::Arc;

use crate::core::error::ApiError;
use crate::helpers::json_schema::{json_schema_format_with_options, JsonSchemaOutputFormatOptions};
use crate::resources::beta::messages::BetaToolResultContent;
use crate::resources::messages::{JsonOutputFormat, OutputConfig};
use crate::sdk_lib::tools::{BetaRunnableTool, RunnableTool, ToolError};

/// Maps to TS `betaJSONSchemaOutputFormat(schema, options?)`.
pub fn beta_json_schema_format(schema: serde_json::Value) -> Result<JsonOutputFormat, ApiError> {
    beta_json_schema_format_with_options(schema, JsonSchemaOutputFormatOptions::default())
}

/// TS-style camelCase alias for [`beta_json_schema_format`].
///
/// TypeScript's `betaJSONSchemaOutputFormat()` returns the beta JSON-schema
/// format object itself. Rust also keeps [`beta_json_schema_output_format`] as
/// a convenience that wraps the format in an [`OutputConfig`].
#[allow(non_snake_case)]
pub fn betaJSONSchemaOutputFormat(schema: serde_json::Value) -> Result<JsonOutputFormat, ApiError> {
    beta_json_schema_format(schema)
}

/// TS-style camelCase alias for [`beta_json_schema_format_with_options`].
#[allow(non_snake_case)]
pub fn betaJSONSchemaOutputFormatWithOptions(
    schema: serde_json::Value,
    options: JsonSchemaOutputFormatOptions,
) -> Result<JsonOutputFormat, ApiError> {
    beta_json_schema_format_with_options(schema, options)
}

/// Same as [`beta_json_schema_format`] with explicit transform options.
pub fn beta_json_schema_format_with_options(
    schema: serde_json::Value,
    options: JsonSchemaOutputFormatOptions,
) -> Result<JsonOutputFormat, ApiError> {
    json_schema_format_with_options(schema, options)
}

/// Rust convenience wrapper returning an [`OutputConfig`] ready for
/// `BetaMessageCreateParams.output_config`.
pub fn beta_json_schema_output_format(schema: serde_json::Value) -> Result<OutputConfig, ApiError> {
    beta_json_schema_output_format_with_options(schema, JsonSchemaOutputFormatOptions::default())
}

/// Same as [`beta_json_schema_output_format`] with explicit transform options.
pub fn beta_json_schema_output_format_with_options(
    schema: serde_json::Value,
    options: JsonSchemaOutputFormatOptions,
) -> Result<OutputConfig, ApiError> {
    Ok(OutputConfig {
        effort: None,
        format: Some(beta_json_schema_format_with_options(schema, options)?),
    })
}

/// Options for [`beta_json_schema_tool`].
///
/// Maps to TS `betaTool({ name, inputSchema, description, run })` from
/// `helpers/beta/json-schema.ts`. The handler receives raw JSON input because
/// Rust cannot infer a static type from an arbitrary runtime JSON schema.
pub struct BetaJsonSchemaToolOptions {
    pub name: String,
    pub input_schema: serde_json::Value,
    pub description: String,
    pub run:
        Arc<dyn Fn(serde_json::Value) -> Result<BetaToolResultContent, ToolError> + Send + Sync>,
}

impl BetaJsonSchemaToolOptions {
    pub fn new(
        name: impl Into<String>,
        input_schema: serde_json::Value,
        description: impl Into<String>,
        run: impl Fn(serde_json::Value) -> Result<BetaToolResultContent, ToolError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            input_schema,
            description: description.into(),
            run: Arc::new(run),
        }
    }

    pub fn new_text(
        name: impl Into<String>,
        input_schema: serde_json::Value,
        description: impl Into<String>,
        run: impl Fn(serde_json::Value) -> Result<String, ToolError> + Send + Sync + 'static,
    ) -> Self {
        Self::new(name, input_schema, description, move |input| {
            run(input).map(BetaToolResultContent::Text)
        })
    }
}

/// Runnable beta JSON-schema tool returned by [`beta_json_schema_tool`].
pub struct BetaJsonSchemaTool {
    name: String,
    input_schema: serde_json::Value,
    description: String,
    run: Arc<dyn Fn(serde_json::Value) -> Result<BetaToolResultContent, ToolError> + Send + Sync>,
}

/// Rust equivalent of TS `betaTool()`.
///
/// The schema must be a JSON object schema. Like the TS JSON-schema helper,
/// runtime parsing is an identity cast; callers that want typed validation can
/// use `helpers::beta::schemars::beta_schemars_tool`.
pub fn beta_json_schema_tool(
    options: BetaJsonSchemaToolOptions,
) -> Result<BetaJsonSchemaTool, ApiError> {
    let schema_type = options
        .input_schema
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if schema_type != "object" {
        return Err(ApiError::Sdk(format!(
            "JSON schema for tool \"{}\" must be an object, but got {schema_type}",
            options.name
        )));
    }

    Ok(BetaJsonSchemaTool {
        name: options.name,
        input_schema: options.input_schema,
        description: options.description,
        run: options.run,
    })
}

/// TS-style camelCase alias for [`beta_json_schema_tool`].
#[allow(non_snake_case)]
pub fn betaTool(options: BetaJsonSchemaToolOptions) -> Result<BetaJsonSchemaTool, ApiError> {
    beta_json_schema_tool(options)
}

#[async_trait::async_trait]
impl RunnableTool for BetaJsonSchemaTool {
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

    async fn run_beta_tool_result_content(
        &self,
        input: serde_json::Value,
    ) -> Result<BetaToolResultContent, ToolError> {
        (self.run)(input)
    }

    async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
        match (self.run)(input)? {
            BetaToolResultContent::Text(text) => Ok(text),
            BetaToolResultContent::Blocks(blocks) => {
                serde_json::to_string(&blocks).map_err(|err| {
                    ToolError::new(format!("Failed to serialize structured tool result: {err}"))
                })
            }
        }
    }
}

impl BetaRunnableTool for BetaJsonSchemaTool {}
