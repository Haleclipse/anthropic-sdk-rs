// Ported from TS SDK: tests/helpers/beta/mcp.test.ts-style behavior.

use std::sync::{Arc, Mutex};

use anthropic_sdk::core::uploads::Uploadable;
use anthropic_sdk::helpers::beta::mcp::{
    MCPBlobResourceContentsLike, MCPCallToolParams, MCPCallToolResultLike, MCPClientLike,
    MCPContentExtraProps, MCPEmbeddedResourceLike, MCPPromptMessageLike, MCPReadResourceResultLike,
    MCPResourceContentsLike, MCPTextContentLike, MCPTextResourceContentsLike, MCPToolExtraProps,
    MCPToolLike, MCPToolResultContentLike, SDK_HELPER_SYMBOL, mcp_content, mcp_message,
    mcp_resource_to_content, mcp_resource_to_file, mcp_tool, mcp_tool_definition, mcpContent,
    mcpContentWithOptions, mcpMessage, mcpMessages, mcpResourceToContent,
    mcpResourceToContentWithOptions, mcpResourceToFile, mcpTool, mcpTools,
};
use anthropic_sdk::resources::beta::messages::types::{
    BetaContentBlockParam, BetaMessageContent, BetaRequestDocumentSource, BetaToolAllowedCaller,
    BetaToolInputSchema, BetaToolResultContent, BetaToolResultContentBlockParam,
};
use anthropic_sdk::resources::messages::CacheControlEphemeral;
use anthropic_sdk::sdk_lib::tools::{RunnableTool, ToolError};
use serde_json::json;

fn sample_tool(name: &str) -> MCPToolLike {
    MCPToolLike {
        name: name.to_owned(),
        description: Some("Get weather".to_owned()),
        input_schema: BetaToolInputSchema {
            type_name: "object".to_owned(),
            properties: Some(json!({"location": {"type": "string"}})),
            required: Some(vec!["location".to_owned()]),
            additional_properties: Default::default(),
        },
    }
}

#[test]
fn mcp_tool_definition_converts_mcp_tool_like() {
    let definition = mcp_tool_definition(
        sample_tool("get_weather"),
        Some(MCPToolExtraProps {
            allowed_callers: Some(vec![BetaToolAllowedCaller::Direct]),
            strict: Some(true),
            ..Default::default()
        }),
    );

    let json = serde_json::to_value(definition).unwrap();
    assert_eq!(json["name"], "get_weather");
    assert_eq!(json["description"], "Get weather");
    assert_eq!(json["input_schema"]["type"], "object");
    assert_eq!(json["input_schema"]["required"][0], "location");
    assert_eq!(json["allowed_callers"][0], "direct");
    assert_eq!(json["strict"], true);
}

#[test]
fn mcp_content_converts_text_and_image() {
    let text = mcp_content(MCPToolResultContentLike::Text {
        text: "hello".to_owned(),
    })
    .unwrap();
    match text {
        BetaContentBlockParam::Text(block) => assert_eq!(block.text, "hello"),
        other => panic!("unexpected text conversion: {other:?}"),
    }

    let image = mcp_content(MCPToolResultContentLike::Image {
        data: "aW1hZ2U=".to_owned(),
        mime_type: "image/png".to_owned(),
    })
    .unwrap();
    match image {
        BetaContentBlockParam::Image(block) => match block.source {
            anthropic_sdk::resources::beta::messages::types::BetaImageSource::Base64 {
                data,
                media_type,
            } => {
                assert_eq!(data, "aW1hZ2U=");
                assert_eq!(media_type, "image/png");
            }
            other => panic!("unexpected image source: {other:?}"),
        },
        other => panic!("unexpected image conversion: {other:?}"),
    }
}

#[test]
fn mcp_content_and_resource_helpers_apply_extra_props_like_ts() {
    let cache_control = CacheControlEphemeral::default();
    let text = mcpContentWithOptions(
        MCPToolResultContentLike::Text {
            text: "hello".to_owned(),
        },
        Some(MCPContentExtraProps {
            cache_control: Some(cache_control.clone()),
            ..Default::default()
        }),
    )
    .unwrap();
    match text {
        BetaContentBlockParam::Text(block) => {
            assert_eq!(block.text, "hello");
            assert_eq!(block.cache_control.unwrap().type_name, "ephemeral");
        }
        other => panic!("unexpected text conversion: {other:?}"),
    }

    let doc = mcpResourceToContentWithOptions(
        MCPReadResourceResultLike {
            contents: vec![MCPResourceContentsLike::Text(MCPTextResourceContentsLike {
                uri: "file:///docs/readme.txt".to_owned(),
                mime_type: Some("text/plain".to_owned()),
                text: "Readme".to_owned(),
            })],
        },
        Some(MCPContentExtraProps {
            cache_control: Some(cache_control),
            context: Some("local docs".to_owned()),
            title: Some("README".to_owned()),
            ..Default::default()
        }),
    )
    .unwrap();
    match doc {
        BetaContentBlockParam::Document(block) => {
            assert_eq!(block.cache_control.unwrap().type_name, "ephemeral");
            assert_eq!(block.context.as_deref(), Some("local docs"));
            assert_eq!(block.title.as_deref(), Some("README"));
            assert_eq!(block.stainless_helpers, vec!["mcpResourceToContent"]);
        }
        other => panic!("unexpected resource conversion: {other:?}"),
    }
}

#[test]
fn mcp_content_rejects_unsupported_audio_and_image_mime() {
    let audio_err = mcp_content(MCPToolResultContentLike::Audio {
        data: "abc".to_owned(),
        mime_type: "audio/wav".to_owned(),
    })
    .unwrap_err();
    assert!(
        audio_err
            .to_string()
            .contains("Unsupported MCP content type: audio")
    );

    let image_err = mcp_content(MCPToolResultContentLike::Image {
        data: "abc".to_owned(),
        mime_type: "image/tiff".to_owned(),
    })
    .unwrap_err();
    assert!(
        image_err
            .to_string()
            .contains("Unsupported image MIME type: image/tiff")
    );

    let unsupported_resource_err = mcp_content(MCPToolResultContentLike::Resource {
        resource: MCPResourceContentsLike::Text(MCPTextResourceContentsLike {
            uri: "file:///docs/sound.wav".to_owned(),
            mime_type: Some("audio/wav".to_owned()),
            text: "sound".to_owned(),
        }),
    })
    .unwrap_err();
    assert!(
        unsupported_resource_err
            .to_string()
            .contains("Unsupported MIME type \"audio/wav\" for resource: file:///docs/sound.wav")
    );
}

#[test]
fn mcp_resource_to_content_converts_pdf_blob_and_text() {
    let pdf = mcp_resource_to_content(MCPReadResourceResultLike {
        contents: vec![MCPResourceContentsLike::Blob(MCPBlobResourceContentsLike {
            uri: "file:///docs/report.pdf".to_owned(),
            mime_type: Some("application/pdf".to_owned()),
            blob: "cGRm".to_owned(),
        })],
    })
    .unwrap();
    match pdf {
        BetaContentBlockParam::Document(block) => {
            assert_eq!(block.stainless_helpers, vec!["mcpResourceToContent"]);
            match block.source {
                BetaRequestDocumentSource::Base64PDF { data, media_type } => {
                    assert_eq!(data, "cGRm");
                    assert_eq!(media_type, "application/pdf");
                }
                other => panic!("unexpected pdf source: {other:?}"),
            }
        }
        other => panic!("unexpected pdf conversion: {other:?}"),
    }

    let text = mcp_resource_to_content(MCPReadResourceResultLike {
        contents: vec![MCPResourceContentsLike::Text(MCPTextResourceContentsLike {
            uri: "file:///docs/readme.txt".to_owned(),
            mime_type: Some("text/markdown".to_owned()),
            text: "# Readme".to_owned(),
        })],
    })
    .unwrap();
    match text {
        BetaContentBlockParam::Document(block) => {
            assert_eq!(block.stainless_helpers, vec!["mcpResourceToContent"]);
            match block.source {
                BetaRequestDocumentSource::PlainText { data, media_type } => {
                    assert_eq!(data, "# Readme");
                    assert_eq!(media_type, "text/plain");
                }
                other => panic!("unexpected text source: {other:?}"),
            }
        }
        other => panic!("unexpected text conversion: {other:?}"),
    }

    let err = mcp_resource_to_content(MCPReadResourceResultLike {
        contents: vec![MCPResourceContentsLike::Text(MCPTextResourceContentsLike {
            uri: "file:///docs/sound.wav".to_owned(),
            mime_type: Some("audio/wav".to_owned()),
            text: "sound".to_owned(),
        })],
    })
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("No supported MIME type found in resource contents. Available: audio/wav")
    );
}

#[test]
fn mcp_message_wraps_converted_content_block() {
    let message = mcp_message(MCPPromptMessageLike {
        role: "user".to_owned(),
        content: MCPToolResultContentLike::Text {
            text: "Use this context".to_owned(),
        },
    })
    .unwrap();

    assert_eq!(message.role, "user");
    match message.content {
        BetaMessageContent::Blocks(blocks) => match &blocks[0] {
            BetaContentBlockParam::Text(block) => assert_eq!(block.text, "Use this context"),
            other => panic!("unexpected message block: {other:?}"),
        },
        other => panic!("unexpected message content: {other:?}"),
    }
}

#[test]
fn mcp_resource_to_file_decodes_blob_and_uses_uri_filename() {
    let upload = mcp_resource_to_file(MCPReadResourceResultLike {
        contents: vec![MCPResourceContentsLike::Blob(MCPBlobResourceContentsLike {
            uri: "file:///tmp/data.txt".to_owned(),
            mime_type: Some("text/plain".to_owned()),
            blob: "SGk=".to_owned(),
        })],
    })
    .unwrap();

    assert_eq!(upload.stainless_helper(), Some("mcpResourceToFile"));
    match upload {
        Uploadable::WithHelper { upload, .. } => match *upload {
            Uploadable::Bytes {
                filename,
                bytes,
                mime_type,
            } => {
                assert_eq!(filename, "data.txt");
                assert_eq!(bytes, b"Hi");
                assert_eq!(mime_type.as_deref(), Some("text/plain"));
            }
            other => panic!("unexpected wrapped uploadable: {other:?}"),
        },
        other => panic!("unexpected uploadable: {other:?}"),
    }
}

struct FakeMcpClient {
    result: MCPCallToolResultLike,
    calls: Mutex<Vec<MCPCallToolParams>>,
}

#[async_trait::async_trait]
impl MCPClientLike for FakeMcpClient {
    async fn call_tool(
        &self,
        params: MCPCallToolParams,
    ) -> Result<MCPCallToolResultLike, ToolError> {
        self.calls.lock().unwrap().push(params);
        Ok(self.result.clone())
    }
}

#[tokio::test]
async fn mcp_ts_style_aliases_are_available_like_helpers_beta_mcp_exports() {
    assert_eq!(SDK_HELPER_SYMBOL, "__stainless_helper");

    let text = mcpContent(
        MCPTextContentLike {
            text: "hello".to_owned(),
        }
        .into(),
    )
    .unwrap();
    match text {
        BetaContentBlockParam::Text(block) => assert_eq!(block.text, "hello"),
        other => panic!("unexpected text conversion: {other:?}"),
    }

    let message = mcpMessage(MCPPromptMessageLike {
        role: "user".to_owned(),
        content: MCPToolResultContentLike::Text {
            text: "ctx".to_owned(),
        },
    })
    .unwrap();
    assert_eq!(message.role, "user");

    let messages = mcpMessages(vec![MCPPromptMessageLike {
        role: "assistant".to_owned(),
        content: MCPToolResultContentLike::Text {
            text: "reply".to_owned(),
        },
    }])
    .unwrap();
    assert_eq!(messages.len(), 1);

    let content = mcpResourceToContent(MCPReadResourceResultLike {
        contents: vec![MCPResourceContentsLike::Text(MCPTextResourceContentsLike {
            uri: "file:///docs/readme.txt".to_owned(),
            mime_type: Some("text/plain".to_owned()),
            text: "readme".to_owned(),
        })],
    })
    .unwrap();
    assert!(matches!(content, BetaContentBlockParam::Document(_)));

    let upload = mcpResourceToFile(MCPReadResourceResultLike {
        contents: vec![MCPResourceContentsLike::Text(MCPTextResourceContentsLike {
            uri: "file:///docs/readme.txt".to_owned(),
            mime_type: Some("text/plain".to_owned()),
            text: "readme".to_owned(),
        })],
    })
    .unwrap();
    assert_eq!(upload.stainless_helper(), Some("mcpResourceToFile"));

    let client = Arc::new(FakeMcpClient {
        result: MCPCallToolResultLike {
            content: vec![
                MCPTextContentLike {
                    text: "ok".to_owned(),
                }
                .into(),
            ],
            structured_content: None,
            is_error: None,
        },
        calls: Mutex::new(vec![]),
    });
    let tool = mcpTool(sample_tool("get_weather"), Arc::clone(&client), None);
    assert_eq!(tool.name(), "get_weather");
    let tools = mcpTools(
        vec![sample_tool("get_weather"), sample_tool("get_stock")],
        Arc::clone(&client),
        None,
    );
    assert_eq!(tools.len(), 2);

    let embedded = MCPEmbeddedResourceLike {
        resource: MCPResourceContentsLike::Text(MCPTextResourceContentsLike {
            uri: "file:///docs/readme.txt".to_owned(),
            mime_type: Some("text/plain".to_owned()),
            text: "readme".to_owned(),
        }),
    };
    let _: MCPToolResultContentLike = embedded.into();
}

#[tokio::test]
async fn mcp_tool_calls_client_and_returns_text_result() {
    let client = Arc::new(FakeMcpClient {
        result: MCPCallToolResultLike {
            content: vec![MCPToolResultContentLike::Text {
                text: "sunny".to_owned(),
            }],
            structured_content: None,
            is_error: None,
        },
        calls: Mutex::new(vec![]),
    });
    let tool = mcp_tool(sample_tool("get_weather"), Arc::clone(&client), None);

    let output = tool
        .run(json!({"location": "San Francisco"}))
        .await
        .unwrap();
    assert_eq!(output, "sunny");

    let calls = client.calls.lock().unwrap();
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(
        calls[0].arguments.as_ref().unwrap()["location"],
        "San Francisco"
    );
}

#[tokio::test]
async fn mcp_tool_returns_structured_content_when_no_text_content() {
    let client = Arc::new(FakeMcpClient {
        result: MCPCallToolResultLike {
            content: vec![],
            structured_content: Some(json!({"ok": true})),
            is_error: None,
        },
        calls: Mutex::new(vec![]),
    });
    let tool = mcp_tool(sample_tool("structured"), client, None);

    let output = tool.run(json!({})).await.unwrap();
    assert_eq!(output, json!({"ok": true}).to_string());
}

#[tokio::test]
async fn mcp_tool_preserves_non_text_content_blocks_like_ts() {
    let client = Arc::new(FakeMcpClient {
        result: MCPCallToolResultLike {
            content: vec![MCPToolResultContentLike::Image {
                data: "aW1hZ2U=".to_owned(),
                mime_type: "image/png".to_owned(),
            }],
            structured_content: None,
            is_error: None,
        },
        calls: Mutex::new(vec![]),
    });
    let tool = mcp_tool(sample_tool("image_tool"), client, None);

    let output = tool.run_beta_tool_result_content(json!({})).await.unwrap();
    match output {
        BetaToolResultContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            assert!(matches!(
                blocks[0],
                BetaToolResultContentBlockParam::Image(_)
            ));
        }
        BetaToolResultContent::Text(text) => panic!("expected blocks, got {text}"),
    }
}

#[tokio::test]
async fn mcp_tool_error_preserves_structured_content_blocks_like_ts_tool_error() {
    let client = Arc::new(FakeMcpClient {
        result: MCPCallToolResultLike {
            content: vec![MCPToolResultContentLike::Text {
                text: "bad input".to_owned(),
            }],
            structured_content: None,
            is_error: Some(true),
        },
        calls: Mutex::new(vec![]),
    });
    let tool = mcp_tool(sample_tool("failing_tool"), client, None);

    let err = tool
        .run_beta_tool_result_content(json!({}))
        .await
        .expect_err("MCP isError result should become ToolError");
    assert_eq!(err.to_string(), "bad input");
    let blocks = err
        .content_blocks
        .expect("structured blocks should be preserved");
    assert_eq!(blocks.len(), 1);
    assert!(matches!(
        blocks[0],
        BetaToolResultContentBlockParam::Text(_)
    ));
}
