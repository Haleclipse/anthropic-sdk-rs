// Maps to: TS helpers/json-schema.ts
//
// Helper for constructing a JSON-schema-based output format configuration
// that can be passed to `MessageCreateParams.output_config` to request
// structured JSON output from the API.

use crate::resources::messages::{JsonOutputFormat, OutputConfig};
use crate::sdk_lib::transform_json_schema::transform_json_schema;

/// Options for [`json_schema_output_format_with_options`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonSchemaOutputFormatOptions {
    /// Whether to apply the TS SDK's strict-schema transformation first.
    /// Defaults to `true`, matching `helpers/json-schema.ts`.
    pub transform: bool,
}

impl Default for JsonSchemaOutputFormatOptions {
    fn default() -> Self {
        Self { transform: true }
    }
}

/// Creates a [`JsonOutputFormat`] from a JSON Schema.
///
/// Maps to TS `jsonSchemaOutputFormat(schema, options?)` in
/// `helpers/json-schema.ts`. Rust parsing remains serde-based via
/// `sdk_lib::parser::parse_message::<T>()` instead of carrying a JS-style
/// `parse` closure on the format object.
pub fn json_schema_format(
    schema: serde_json::Value,
) -> Result<JsonOutputFormat, crate::core::error::ApiError> {
    json_schema_format_with_options(schema, JsonSchemaOutputFormatOptions::default())
}

/// TS-style camelCase alias for [`json_schema_format`].
///
/// TypeScript's `jsonSchemaOutputFormat()` returns the JSON-schema format
/// object itself. Rust also keeps [`json_schema_output_format`] as a
/// convenience that wraps the format in an [`OutputConfig`].
#[allow(non_snake_case)]
pub fn jsonSchemaOutputFormat(
    schema: serde_json::Value,
) -> Result<JsonOutputFormat, crate::core::error::ApiError> {
    json_schema_format(schema)
}

/// TS-style camelCase alias for [`json_schema_format_with_options`].
#[allow(non_snake_case)]
pub fn jsonSchemaOutputFormatWithOptions(
    schema: serde_json::Value,
    options: JsonSchemaOutputFormatOptions,
) -> Result<JsonOutputFormat, crate::core::error::ApiError> {
    json_schema_format_with_options(schema, options)
}

/// Same as [`json_schema_format`] with explicit transform options.
pub fn json_schema_format_with_options(
    schema: serde_json::Value,
    options: JsonSchemaOutputFormatOptions,
) -> Result<JsonOutputFormat, crate::core::error::ApiError> {
    validate_object_schema(&schema, "JSON schema")?;
    let schema = if options.transform {
        transform_json_schema(schema)?
    } else {
        schema
    };

    Ok(JsonOutputFormat {
        schema,
        type_name: "json_schema".to_owned(),
    })
}

/// Creates an [`OutputConfig`] with `format` set to [`JsonOutputFormat`],
/// instructing the model to produce structured output conforming to the given
/// JSON Schema.
///
/// This preserves the older Rust convenience API while aligning the default
/// transformation behavior with TS `jsonSchemaOutputFormat(schema)`.
pub fn json_schema_output_format(
    schema: serde_json::Value,
) -> Result<OutputConfig, crate::core::error::ApiError> {
    json_schema_output_format_with_options(schema, JsonSchemaOutputFormatOptions::default())
}

/// Same as [`json_schema_output_format`] with explicit transform options.
pub fn json_schema_output_format_with_options(
    schema: serde_json::Value,
    options: JsonSchemaOutputFormatOptions,
) -> Result<OutputConfig, crate::core::error::ApiError> {
    Ok(OutputConfig {
        effort: None,
        format: Some(json_schema_format_with_options(schema, options)?),
    })
}

fn validate_object_schema(
    schema: &serde_json::Value,
    label: &str,
) -> Result<(), crate::core::error::ApiError> {
    let schema_type = schema.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if schema_type != "object" {
        return Err(crate::core::error::ApiError::Sdk(format!(
            "{label} must be an object, but got {schema_type}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn creates_output_config_from_valid_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });

        let result = json_schema_output_format(schema.clone());
        assert!(result.is_ok());

        let config = result.unwrap();
        assert!(config.effort.is_none());

        let fmt = config.format.as_ref().unwrap();
        assert_eq!(fmt.type_name, "json_schema");
        assert_eq!(fmt.schema["type"], "object");
        assert_eq!(fmt.schema["properties"]["name"]["type"], "string");
        assert_eq!(fmt.schema["required"], schema["required"]);
        assert_eq!(fmt.schema["additionalProperties"], false);
    }

    #[test]
    fn can_disable_transform_to_preserve_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 2 }
            }
        });

        let config = json_schema_output_format_with_options(
            schema.clone(),
            JsonSchemaOutputFormatOptions { transform: false },
        )
        .unwrap();

        assert_eq!(config.format.unwrap().schema, schema);
    }

    #[test]
    fn rejects_non_object_schema() {
        let schema = json!({
            "type": "array",
            "items": { "type": "string" }
        });

        let result = json_schema_output_format(schema);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_schema_without_type() {
        let schema = json!({
            "properties": {
                "name": { "type": "string" }
            }
        });

        let result = json_schema_output_format(schema);
        assert!(result.is_err());
    }
}
