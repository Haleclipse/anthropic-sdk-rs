// Maps to: TS helpers/beta/mcp.ts
//
// Minimal MCP (Model Context Protocol) duck-typed helpers. These helpers avoid a
// hard dependency on an MCP SDK while converting MCP-like values into Anthropic
// beta message/tool types.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::uploads::Uploadable;
use crate::resources::beta::messages::types::{
    BetaContentBlockParam, BetaImageBlockParam, BetaImageSource, BetaMessageContent,
    BetaMessageParam, BetaRequestDocumentBlock, BetaRequestDocumentSource, BetaTextBlockParam,
    BetaTool, BetaToolAllowedCaller, BetaToolInputExamples, BetaToolInputSchema,
    BetaToolResultContent, BetaToolResultContentBlockParam,
};
use crate::resources::messages::{CacheControlEphemeral, CitationsConfigParam, TextCitationParam};
pub use crate::sdk_lib::stainless_helper_header::{
    SDK_HELPER_SYMBOL, collect_stainless_helpers, collectStainlessHelpers, stainless_helper_header,
    stainlessHelperHeader,
};
use crate::sdk_lib::tools::{RunnableTool, ToolError};

const SUPPORTED_IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Error thrown when an MCP value cannot be represented by the Claude API.
/// Maps to TS `UnsupportedMCPValueError`.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{message}")]
pub struct UnsupportedMCPValueError {
    pub message: String,
}

impl UnsupportedMCPValueError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Represents an MCP tool definition. Maps to TS `MCPToolLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPToolLike {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: BetaToolInputSchema,
}

/// Additional Claude API properties for [`mcp_tool_definition`] / [`mcp_tool`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MCPToolExtraProps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
}

/// Additional Claude API content-block properties accepted by MCP content helpers.
///
/// Maps to the optional `extraProps` argument on TS `mcpMessage()`,
/// `mcpMessages()`, `mcpContent()`, and `mcpResourceToContent()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MCPContentExtraProps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    /// Document/search-result citation configuration (`citations` in TS document blocks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfigParam>,
    /// Text-block citations. Rust separates this from document citation config
    /// because those two TS `citations` fields have different wire shapes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_citations: Option<Vec<TextCitationParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Maps to TS `MCPCallToolResultLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPCallToolResultLike {
    pub content: Vec<MCPToolResultContentLike>,
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Maps to TS `MCPTextContentLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPTextContentLike {
    pub text: String,
}

/// Maps to TS `MCPImageContentLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPImageContentLike {
    pub data: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// Maps to TS `MCPAudioContentLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPAudioContentLike {
    pub data: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// Maps to TS `MCPEmbeddedResourceLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPEmbeddedResourceLike {
    pub resource: MCPResourceContentsLike,
}

/// Maps to TS `MCPResourceLinkLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPResourceLinkLike {
    pub uri: String,
    pub name: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Maps to TS `MCPToolResultContentLike` / prompt content unions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MCPToolResultContentLike {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "audio")]
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource { resource: MCPResourceContentsLike },
    #[serde(rename = "resource_link")]
    ResourceLink {
        uri: String,
        name: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
}

impl From<MCPTextContentLike> for MCPToolResultContentLike {
    fn from(value: MCPTextContentLike) -> Self {
        Self::Text { text: value.text }
    }
}

impl From<MCPImageContentLike> for MCPToolResultContentLike {
    fn from(value: MCPImageContentLike) -> Self {
        Self::Image {
            data: value.data,
            mime_type: value.mime_type,
        }
    }
}

impl From<MCPAudioContentLike> for MCPToolResultContentLike {
    fn from(value: MCPAudioContentLike) -> Self {
        Self::Audio {
            data: value.data,
            mime_type: value.mime_type,
        }
    }
}

impl From<MCPEmbeddedResourceLike> for MCPToolResultContentLike {
    fn from(value: MCPEmbeddedResourceLike) -> Self {
        Self::Resource {
            resource: value.resource,
        }
    }
}

impl From<MCPResourceLinkLike> for MCPToolResultContentLike {
    fn from(value: MCPResourceLinkLike) -> Self {
        Self::ResourceLink {
            uri: value.uri,
            name: value.name,
            mime_type: value.mime_type,
        }
    }
}

pub type MCPPromptContentLike = MCPToolResultContentLike;

/// Maps to TS `TextResourceContents | BlobResourceContents`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MCPResourceContentsLike {
    Text(MCPTextResourceContentsLike),
    Blob(MCPBlobResourceContentsLike),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPTextResourceContentsLike {
    pub uri: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPBlobResourceContentsLike {
    pub uri: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub blob: String,
}

impl MCPResourceContentsLike {
    pub fn uri(&self) -> &str {
        match self {
            Self::Text(resource) => &resource.uri,
            Self::Blob(resource) => &resource.uri,
        }
    }

    pub fn mime_type(&self) -> Option<&str> {
        match self {
            Self::Text(resource) => resource.mime_type.as_deref(),
            Self::Blob(resource) => resource.mime_type.as_deref(),
        }
    }
}

/// Maps to TS `MCPPromptMessageLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPPromptMessageLike {
    pub role: String,
    pub content: MCPPromptContentLike,
}

/// Maps to TS `MCPReadResourceResultLike`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPReadResourceResultLike {
    pub contents: Vec<MCPResourceContentsLike>,
}

/// Parameters passed to an MCP client call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPCallToolParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, serde_json::Value>>,
}

/// Minimal async client trait matching TS `MCPClientLike`.
#[async_trait::async_trait]
pub trait MCPClientLike: Send + Sync {
    async fn call_tool(
        &self,
        params: MCPCallToolParams,
    ) -> Result<MCPCallToolResultLike, ToolError>;
}

/// Converts an MCP tool to a beta custom tool definition.
/// Maps to the definition-building part of TS `mcpTool()`.
pub fn mcp_tool_definition(tool: MCPToolLike, extra_props: Option<MCPToolExtraProps>) -> BetaTool {
    let extra = extra_props.unwrap_or_default();
    BetaTool {
        input_schema: tool.input_schema,
        name: tool.name,
        allowed_callers: extra.allowed_callers,
        cache_control: extra.cache_control,
        defer_loading: extra.defer_loading,
        description: tool.description,
        eager_input_streaming: extra.eager_input_streaming,
        input_examples: extra.input_examples,
        strict: extra.strict,
        type_name: extra.type_name,
        stainless_helper: Some("mcpTool".to_owned()),
    }
}

/// Runnable MCP tool equivalent.
///
/// Text results stay text, while non-text MCP content is preserved as structured
/// beta tool-result blocks through `run_beta_tool_result_content()`, matching TS
/// `mcpTool().run()` returning `string | Array<BetaToolResultContentBlockParam>`.
pub struct MCPRunnableTool<C> {
    definition: BetaTool,
    tool_name: String,
    client: Arc<C>,
}

/// Converts an MCP tool and client into a runnable tool.
pub fn mcp_tool<C: MCPClientLike + 'static>(
    tool: MCPToolLike,
    client: Arc<C>,
    extra_props: Option<MCPToolExtraProps>,
) -> MCPRunnableTool<C> {
    let tool_name = tool.name.clone();
    MCPRunnableTool {
        definition: mcp_tool_definition(tool, extra_props),
        tool_name,
        client,
    }
}

/// TS-style camelCase alias for [`mcp_tool`].
#[allow(non_snake_case)]
pub fn mcpTool<C: MCPClientLike + 'static>(
    tool: MCPToolLike,
    client: Arc<C>,
    extra_props: Option<MCPToolExtraProps>,
) -> MCPRunnableTool<C> {
    mcp_tool(tool, client, extra_props)
}

/// Converts many MCP tools into runnable tools. Maps to TS `mcpTools()`.
pub fn mcp_tools<C: MCPClientLike + 'static>(
    tools: Vec<MCPToolLike>,
    client: Arc<C>,
    extra_props: Option<MCPToolExtraProps>,
) -> Vec<MCPRunnableTool<C>> {
    tools
        .into_iter()
        .map(|tool| mcp_tool(tool, Arc::clone(&client), extra_props.clone()))
        .collect()
}

/// TS-style camelCase alias for [`mcp_tools`].
#[allow(non_snake_case)]
pub fn mcpTools<C: MCPClientLike + 'static>(
    tools: Vec<MCPToolLike>,
    client: Arc<C>,
    extra_props: Option<MCPToolExtraProps>,
) -> Vec<MCPRunnableTool<C>> {
    mcp_tools(tools, client, extra_props)
}

#[async_trait::async_trait]
impl<C: MCPClientLike + 'static> RunnableTool for MCPRunnableTool<C> {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn stainless_helper(&self) -> Option<&str> {
        Some("mcpTool")
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::to_value(&self.definition).unwrap_or_else(|_| serde_json::json!({}))
    }

    async fn run_beta_tool_result_content(
        &self,
        input: serde_json::Value,
    ) -> Result<BetaToolResultContent, ToolError> {
        let arguments = mcp_arguments_from_input(input)?;

        let result = self
            .client
            .call_tool(MCPCallToolParams {
                name: self.tool_name.clone(),
                arguments,
            })
            .await?;

        if result.is_error.unwrap_or(false) {
            let blocks = mcp_tool_result_content_blocks(result.content)
                .map_err(|err| ToolError::with_content(err.to_string(), err.to_string()))?;
            return Err(ToolError::with_content_blocks(blocks));
        }

        mcp_tool_result_to_beta_content(result)
            .map_err(|err| ToolError::with_content(err.to_string(), err.to_string()))
    }

    async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
        match self.run_beta_tool_result_content(input).await? {
            BetaToolResultContent::Text(text) => Ok(text),
            BetaToolResultContent::Blocks(blocks) => serde_json::to_string(&blocks)
                .map_err(|err| ToolError::new(format!("failed to serialize MCP content: {err}"))),
        }
    }
}

/// Converts an MCP prompt message to a beta message param. Maps to TS `mcpMessage()`.
pub fn mcp_message(
    message: MCPPromptMessageLike,
) -> Result<BetaMessageParam, UnsupportedMCPValueError> {
    mcp_message_with_options(message, None)
}

/// Same as [`mcp_message`] with TS `extraProps` content-block overrides.
pub fn mcp_message_with_options(
    message: MCPPromptMessageLike,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<BetaMessageParam, UnsupportedMCPValueError> {
    let mut content = mcp_content_with_options(message.content, extra_props)?;
    add_stainless_helper_to_content_block(&mut content, "mcpMessage");
    Ok(BetaMessageParam {
        role: message.role,
        content: BetaMessageContent::Blocks(vec![content]),
    })
}

/// TS-style camelCase alias for [`mcp_message`].
#[allow(non_snake_case)]
pub fn mcpMessage(
    message: MCPPromptMessageLike,
) -> Result<BetaMessageParam, UnsupportedMCPValueError> {
    mcp_message(message)
}

/// TS-style camelCase alias for [`mcp_message_with_options`].
#[allow(non_snake_case)]
pub fn mcpMessageWithOptions(
    message: MCPPromptMessageLike,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<BetaMessageParam, UnsupportedMCPValueError> {
    mcp_message_with_options(message, extra_props)
}

/// Converts many MCP prompt messages. Maps to TS `mcpMessages()`.
pub fn mcp_messages(
    messages: Vec<MCPPromptMessageLike>,
) -> Result<Vec<BetaMessageParam>, UnsupportedMCPValueError> {
    mcp_messages_with_options(messages, None)
}

/// Same as [`mcp_messages`] with TS `extraProps` content-block overrides.
pub fn mcp_messages_with_options(
    messages: Vec<MCPPromptMessageLike>,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<Vec<BetaMessageParam>, UnsupportedMCPValueError> {
    messages
        .into_iter()
        .map(|message| mcp_message_with_options(message, extra_props.clone()))
        .collect()
}

/// TS-style camelCase alias for [`mcp_messages`].
#[allow(non_snake_case)]
pub fn mcpMessages(
    messages: Vec<MCPPromptMessageLike>,
) -> Result<Vec<BetaMessageParam>, UnsupportedMCPValueError> {
    mcp_messages(messages)
}

/// TS-style camelCase alias for [`mcp_messages_with_options`].
#[allow(non_snake_case)]
pub fn mcpMessagesWithOptions(
    messages: Vec<MCPPromptMessageLike>,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<Vec<BetaMessageParam>, UnsupportedMCPValueError> {
    mcp_messages_with_options(messages, extra_props)
}

/// Converts one MCP prompt content item to a beta content block. Maps to TS `mcpContent()`.
pub fn mcp_content(
    content: MCPPromptContentLike,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    mcp_content_with_options(content, None)
}

/// Same as [`mcp_content`] with TS `extraProps` content-block overrides.
pub fn mcp_content_with_options(
    content: MCPPromptContentLike,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    match content {
        MCPToolResultContentLike::Text { text } => {
            Ok(BetaContentBlockParam::Text(BetaTextBlockParam {
                text,
                stainless_helpers: vec!["mcpContent".to_owned()],
                cache_control: extra_props
                    .as_ref()
                    .and_then(|props| props.cache_control.clone()),
                citations: extra_props
                    .as_ref()
                    .and_then(|props| props.text_citations.clone()),
            }))
        }
        MCPToolResultContentLike::Image { data, mime_type } => {
            if !is_supported_image_type(&mime_type) {
                return Err(UnsupportedMCPValueError::new(format!(
                    "Unsupported image MIME type: {mime_type}"
                )));
            }
            Ok(BetaContentBlockParam::Image(BetaImageBlockParam {
                source: BetaImageSource::Base64 {
                    data,
                    media_type: mime_type,
                },
                stainless_helpers: vec!["mcpContent".to_owned()],
                cache_control: extra_props
                    .as_ref()
                    .and_then(|props| props.cache_control.clone()),
            }))
        }
        MCPToolResultContentLike::Resource { resource } => {
            let mut block = mcp_resource_content_to_content_block(resource, extra_props.as_ref())?;
            add_stainless_helper_to_content_block(&mut block, "mcpContent");
            Ok(block)
        }
        MCPToolResultContentLike::ResourceLink { .. } => Err(UnsupportedMCPValueError::new(
            "Unsupported MCP content type: resource_link",
        )),
        MCPToolResultContentLike::Audio { .. } => Err(UnsupportedMCPValueError::new(
            "Unsupported MCP content type: audio",
        )),
    }
}

fn add_stainless_helper_to_content_block(block: &mut BetaContentBlockParam, helper: &str) {
    let helpers = match block {
        BetaContentBlockParam::Text(block) => &mut block.stainless_helpers,
        BetaContentBlockParam::Image(block) => &mut block.stainless_helpers,
        BetaContentBlockParam::Document(block) => &mut block.stainless_helpers,
        _ => return,
    };
    if !helpers.iter().any(|existing| existing == helper) {
        helpers.push(helper.to_owned());
    }
}

/// TS-style camelCase alias for [`mcp_content`].
#[allow(non_snake_case)]
pub fn mcpContent(
    content: MCPPromptContentLike,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    mcp_content(content)
}

/// TS-style camelCase alias for [`mcp_content_with_options`].
#[allow(non_snake_case)]
pub fn mcpContentWithOptions(
    content: MCPPromptContentLike,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    mcp_content_with_options(content, extra_props)
}

/// Converts MCP resource contents to an Anthropic content block.
pub fn mcp_resource_to_content(
    result: MCPReadResourceResultLike,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    mcp_resource_to_content_with_options(result, None)
}

/// Same as [`mcp_resource_to_content`] with TS `extraProps` content-block overrides.
pub fn mcp_resource_to_content_with_options(
    result: MCPReadResourceResultLike,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    if result.contents.is_empty() {
        return Err(UnsupportedMCPValueError::new(
            "Resource contents array must contain at least one item",
        ));
    }

    let mime_types = result
        .contents
        .iter()
        .filter_map(|resource| resource.mime_type())
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .join(", ");
    let supported = result
        .contents
        .into_iter()
        .find(|resource| is_supported_resource_mime_type(resource.mime_type()));

    match supported {
        Some(resource) => {
            let mut block = mcp_resource_content_to_content_block(resource, extra_props.as_ref())?;
            add_stainless_helper_to_content_block(&mut block, "mcpResourceToContent");
            Ok(block)
        }
        None => Err(UnsupportedMCPValueError::new(format!(
            "No supported MIME type found in resource contents. Available: {mime_types}"
        ))),
    }
}

/// TS-style camelCase alias for [`mcp_resource_to_content`].
#[allow(non_snake_case)]
pub fn mcpResourceToContent(
    result: MCPReadResourceResultLike,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    mcp_resource_to_content(result)
}

/// TS-style camelCase alias for [`mcp_resource_to_content_with_options`].
#[allow(non_snake_case)]
pub fn mcpResourceToContentWithOptions(
    result: MCPReadResourceResultLike,
    extra_props: Option<MCPContentExtraProps>,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    mcp_resource_to_content_with_options(result, extra_props)
}

/// Converts an MCP resource to an uploadable file. Maps to TS `mcpResourceToFile()`.
pub fn mcp_resource_to_file(
    result: MCPReadResourceResultLike,
) -> Result<Uploadable, UnsupportedMCPValueError> {
    let resource = result.contents.into_iter().next().ok_or_else(|| {
        UnsupportedMCPValueError::new("Resource contents array must contain at least one item")
    })?;

    let filename = filename_from_uri(resource.uri());
    let mime_type = resource.mime_type().map(str::to_owned);
    let bytes = bytes_from_resource(&resource)?;

    let upload = match mime_type {
        Some(mime) => Uploadable::from_bytes_with_mime(bytes, filename, mime),
        None => Uploadable::from_bytes(bytes, filename),
    };

    Ok(upload.with_stainless_helper("mcpResourceToFile"))
}

/// TS-style camelCase alias for [`mcp_resource_to_file`].
#[allow(non_snake_case)]
pub fn mcpResourceToFile(
    result: MCPReadResourceResultLike,
) -> Result<Uploadable, UnsupportedMCPValueError> {
    mcp_resource_to_file(result)
}

fn mcp_resource_content_to_content_block(
    resource: MCPResourceContentsLike,
    extra_props: Option<&MCPContentExtraProps>,
) -> Result<BetaContentBlockParam, UnsupportedMCPValueError> {
    let uri = resource.uri().to_owned();
    let mime_type = resource.mime_type().map(str::to_owned);

    if mime_type.as_deref().is_some_and(is_supported_image_type) {
        return match resource {
            MCPResourceContentsLike::Blob(blob) => {
                Ok(BetaContentBlockParam::Image(BetaImageBlockParam {
                    source: BetaImageSource::Base64 {
                        data: blob.blob,
                        media_type: mime_type.unwrap_or_default(),
                    },
                    stainless_helpers: Vec::new(),
                    cache_control: extra_props.and_then(|props| props.cache_control.clone()),
                }))
            }
            MCPResourceContentsLike::Text(text) => Err(UnsupportedMCPValueError::new(format!(
                "Image resource must have blob data, not text. URI: {}",
                text.uri
            ))),
        };
    }

    if mime_type.as_deref() == Some("application/pdf") {
        return match resource {
            MCPResourceContentsLike::Blob(blob) => {
                Ok(BetaContentBlockParam::Document(BetaRequestDocumentBlock {
                    source: BetaRequestDocumentSource::Base64PDF {
                        data: blob.blob,
                        media_type: "application/pdf".to_owned(),
                    },
                    stainless_helpers: Vec::new(),
                    cache_control: extra_props.and_then(|props| props.cache_control.clone()),
                    citations: None,
                    context: extra_props.and_then(|props| props.context.clone()),
                    title: extra_props.and_then(|props| props.title.clone()),
                }))
            }
            MCPResourceContentsLike::Text(text) => Err(UnsupportedMCPValueError::new(format!(
                "PDF resource must have blob data, not text. URI: {}",
                text.uri
            ))),
        };
    }

    if mime_type
        .as_deref()
        .is_none_or(|mime| mime.starts_with("text/"))
    {
        let data = text_from_resource(resource)?;
        return Ok(BetaContentBlockParam::Document(BetaRequestDocumentBlock {
            source: BetaRequestDocumentSource::PlainText {
                data,
                media_type: "text/plain".to_owned(),
            },
            stainless_helpers: Vec::new(),
            cache_control: extra_props.and_then(|props| props.cache_control.clone()),
            citations: None,
            context: extra_props.and_then(|props| props.context.clone()),
            title: extra_props.and_then(|props| props.title.clone()),
        }));
    }

    Err(UnsupportedMCPValueError::new(format!(
        "Unsupported MIME type \"{}\" for resource: {uri}",
        mime_type.unwrap_or_else(|| "unknown".to_owned())
    )))
}

fn mcp_arguments_from_input(
    input: serde_json::Value,
) -> Result<Option<HashMap<String, serde_json::Value>>, ToolError> {
    match input {
        serde_json::Value::Object(map) => Ok(Some(map.into_iter().collect())),
        serde_json::Value::Null => Ok(None),
        other => Err(ToolError::with_content(
            "MCP tool input must be a JSON object",
            format!("Invalid MCP tool input: {other}"),
        )),
    }
}

fn mcp_tool_result_to_beta_content(
    result: MCPCallToolResultLike,
) -> Result<BetaToolResultContent, UnsupportedMCPValueError> {
    if result.content.is_empty() {
        if let Some(value) = result.structured_content {
            if value.is_object() {
                return Ok(BetaToolResultContent::Text(value.to_string()));
            }
        }
        return Ok(BetaToolResultContent::Text(String::new()));
    }

    if result.content.len() == 1 {
        if let MCPToolResultContentLike::Text { text } = &result.content[0] {
            return Ok(BetaToolResultContent::Text(text.clone()));
        }
    }

    Ok(BetaToolResultContent::Blocks(
        mcp_tool_result_content_blocks(result.content)?,
    ))
}

fn mcp_tool_result_content_blocks(
    content: Vec<MCPToolResultContentLike>,
) -> Result<Vec<BetaToolResultContentBlockParam>, UnsupportedMCPValueError> {
    content.into_iter().map(mcp_tool_result_content).collect()
}

fn mcp_tool_result_content(
    content: MCPToolResultContentLike,
) -> Result<BetaToolResultContentBlockParam, UnsupportedMCPValueError> {
    match mcp_content(content)? {
        BetaContentBlockParam::Text(block) => Ok(BetaToolResultContentBlockParam::Text(block)),
        BetaContentBlockParam::Image(block) => Ok(BetaToolResultContentBlockParam::Image(block)),
        BetaContentBlockParam::Document(block) => {
            Ok(BetaToolResultContentBlockParam::Document(block))
        }
        other => Err(UnsupportedMCPValueError::new(format!(
            "Unsupported MCP content block for tool result: {other:?}"
        ))),
    }
}

fn is_supported_image_type(mime_type: &str) -> bool {
    SUPPORTED_IMAGE_TYPES.contains(&mime_type)
}

fn is_supported_resource_mime_type(mime_type: Option<&str>) -> bool {
    mime_type.is_none_or(|mime| {
        mime.starts_with("text/") || mime == "application/pdf" || is_supported_image_type(mime)
    })
}

fn text_from_resource(
    resource: MCPResourceContentsLike,
) -> Result<String, UnsupportedMCPValueError> {
    match resource {
        MCPResourceContentsLike::Text(text) => Ok(text.text),
        MCPResourceContentsLike::Blob(blob) => {
            let bytes = decode_base64(&blob.blob)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

fn bytes_from_resource(
    resource: &MCPResourceContentsLike,
) -> Result<Vec<u8>, UnsupportedMCPValueError> {
    match resource {
        MCPResourceContentsLike::Text(text) => Ok(text.text.as_bytes().to_vec()),
        MCPResourceContentsLike::Blob(blob) => decode_base64(&blob.blob),
    }
}

fn filename_from_uri(uri: &str) -> String {
    url::Url::parse(uri)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_owned))
        })
        .filter(|name| !name.is_empty())
        .or_else(|| uri.rsplit('/').next().map(str::to_owned))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "file".to_owned())
}

fn decode_base64(input: &str) -> Result<Vec<u8>, UnsupportedMCPValueError> {
    let mut buffer: u32 = 0;
    let mut bits: u8 = 0;
    let mut out = Vec::new();
    let mut seen_padding = false;

    for byte in input.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if byte == b'=' {
            seen_padding = true;
            continue;
        }
        if seen_padding {
            return Err(UnsupportedMCPValueError::new("invalid base64 padding"));
        }
        let value = base64_value(byte).ok_or_else(|| {
            UnsupportedMCPValueError::new(format!("invalid base64 character: {}", byte as char))
        })? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Ok(out)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
