use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anthropic_sdk::client::Nullable;
use anthropic_sdk::resources::beta::messages::{
    BetaMessageContent, BetaMessageCreateParams, BetaMessageParam,
};
use anthropic_sdk::resources::completions::CompletionCreateParams;
use anthropic_sdk::resources::messages::MessageStreamEvent;
use anthropic_sdk::sdk_lib::tools::BetaToolRunnerParams;
use anthropic_sdk::{
    ClientOptions as CoreClientOptions, MessageContent, MessageCreateParams, MessageParam,
    RequestOptions,
};
use anthropic_sdk_bedrock::core::streaming::Stream as BedrockCoreStream;
use anthropic_sdk_bedrock::credential_providers::{
    Environment, NodeProviderChainOptions, from_node_provider_chain,
};
use anthropic_sdk_bedrock::{
    ANTHROPIC_VERSION, AnthropicBedrock, AwsCredentialProvider, AwsCredentials, BedrockConfig,
    ClientOptions as BedrockClientOptions, create_client, create_client_with_core_options,
    get_auth_headers, rewrite_url,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use futures::StreamExt;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "../../../tests/support/child_env.rs"]
mod child_env;

fn bedrock_config(region: &str) -> BedrockConfig {
    BedrockConfig {
        aws_region: region.to_owned(),
        aws_access_key: None,
        aws_secret_key: None,
        aws_session_token: None,
        base_url: None,
        credential_provider: None,
        skip_auth: true,
    }
}

#[test]
fn bedrock_client_options_alias_matches_ts_export_name() {
    let _: BedrockClientOptions = bedrock_config("us-east-1");
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn expected_authorization(
    method: &str,
    url: &str,
    body: &[u8],
    config: &BedrockConfig,
    amz_date: &str,
) -> String {
    let date = &amz_date[..8];
    let parsed = url::Url::parse(url).unwrap();
    let mut headers = HashMap::new();
    headers.insert("host".to_owned(), parsed.host_str().unwrap().to_owned());
    headers.insert("x-amz-date".to_owned(), amz_date.to_owned());
    headers.insert("x-amz-content-sha256".to_owned(), sha256_hex(body));
    if let Some(token) = &config.aws_session_token {
        headers.insert("x-amz-security-token".to_owned(), token.clone());
    }

    let mut sorted_headers: Vec<_> = headers.iter().collect();
    sorted_headers.sort_by_key(|(name, _)| *name);
    let signed_headers = sorted_headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers = sorted_headers
        .iter()
        .map(|(name, value)| {
            format!(
                "{}:{}\n",
                name,
                value.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect::<String>();
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        canonical_path_for_test(&parsed),
        canonical_query_for_test(&parsed),
        canonical_headers,
        signed_headers,
        headers["x-amz-content-sha256"],
    );
    let credential_scope = format!("{date}/{}/bedrock/aws4_request", config.aws_region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = signing_key_for_test(
        config.aws_secret_key.as_deref().unwrap(),
        date,
        &config.aws_region,
    );
    let signature = hex::encode(hmac_sha256_for_test(
        &signing_key,
        string_to_sign.as_bytes(),
    ));
    format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        config.aws_access_key.as_deref().unwrap(),
    )
}

/// Smithy's `getCanonicalPath`: the normalized path, escaped again with `/`
/// kept (`auth.rs` checks the signer against the TS one's signatures).
fn canonical_path_for_test(url: &url::Url) -> String {
    let path = url.path();
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    let trailing = if !segments.is_empty() && path.ends_with('/') {
        "/"
    } else {
        ""
    };
    sigv4_encode_for_test(&format!("/{}{trailing}", segments.join("/"))).replace("%2F", "/")
}

fn canonical_query_for_test(url: &url::Url) -> String {
    let Some(query) = url.query() else {
        return String::new();
    };
    let mut pairs: Vec<_> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (sigv4_encode_for_test(&key), sigv4_encode_for_test(&value)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn sigv4_encode_for_test(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn hmac_sha256_for_test(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn signing_key_for_test(secret: &str, date: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256_for_test(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256_for_test(&k_date, region.as_bytes());
    let k_service = hmac_sha256_for_test(&k_region, b"bedrock");
    hmac_sha256_for_test(&k_service, b"aws4_request")
}

#[test]
fn get_auth_headers_signs_bedrock_request_with_static_credentials() {
    let config = BedrockConfig {
        aws_region: "us-east-1".to_owned(),
        aws_access_key: Some("AKIDEXAMPLE".to_owned()),
        aws_secret_key: Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned()),
        aws_session_token: Some("session-token".to_owned()),
        base_url: None,
        credential_provider: None,
        skip_auth: false,
    };
    let body = br#"{"anthropic_version":"bedrock-2023-05-31","messages":[]}"#;
    let headers = get_auth_headers(
        "POST",
        "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke?b=2&a=1",
        body,
        &config,
    )
    .unwrap();

    assert_eq!(
        headers.get("host").unwrap(),
        "bedrock-runtime.us-east-1.amazonaws.com"
    );
    assert_eq!(
        headers.get("x-amz-security-token").unwrap(),
        "session-token"
    );
    assert_eq!(
        headers.get("x-amz-content-sha256").unwrap(),
        &sha256_hex(body)
    );

    let authorization = headers.get("authorization").unwrap();
    assert!(authorization.starts_with("AWS4-HMAC-SHA256 "));
    assert!(authorization.contains("Credential=AKIDEXAMPLE/"));
    assert!(authorization.contains("/us-east-1/bedrock/aws4_request"));
    assert!(
        authorization
            .contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token")
    );
    let signature = authorization
        .split("Signature=")
        .nth(1)
        .expect("authorization should include signature");
    assert_eq!(signature.len(), 64);
    assert!(signature.chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn get_auth_headers_requires_static_credentials() {
    let config = bedrock_config("us-east-1");
    let err = get_auth_headers(
        "POST",
        "https://bedrock-runtime.us-east-1.amazonaws.com/model/test/invoke",
        b"{}",
        &config,
    )
    .unwrap_err();
    assert!(err.to_string().contains("Missing AWS access key"));
}

#[test]
fn rewrite_url_non_streaming() {
    let url = rewrite_url(
        "/v1/messages",
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
        false,
    );
    assert_eq!(
        url,
        "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke"
    );
}

#[test]
fn rewrite_url_streaming() {
    let url = rewrite_url(
        "/v1/messages",
        "anthropic.claude-3-5-sonnet-20241022-v2:0",
        true,
    );
    assert_eq!(
        url,
        "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke-with-response-stream"
    );
}

#[test]
fn rewrite_url_encodes_slashes_in_model_id() {
    let url = rewrite_url("/v1/messages", "us.anthropic/claude-3-5-sonnet", false);
    assert_eq!(url, "/model/us.anthropic%2Fclaude-3-5-sonnet/invoke");
}

#[test]
fn rewrite_url_encodes_arn_slashes() {
    let model_arn = "arn:aws:bedrock:us-east-2:1234:inference-profile/us.anthropic.claude-3-7-sonnet-20250219-v1:0";
    let url = rewrite_url("/v1/messages", model_arn, false);
    assert_eq!(
        url,
        "/model/arn:aws:bedrock:us-east-2:1234:inference-profile%2Fus.anthropic.claude-3-7-sonnet-20250219-v1:0/invoke"
    );
}

#[test]
fn rewrite_url_encodes_bedrock_model_id_like_ts_path_tag() {
    let url = rewrite_url("/v1/messages", "provider/model with snowman ☃%", false);
    assert_eq!(
        url,
        "/model/provider%2Fmodel%20with%20snowman%20%E2%98%83%25/invoke"
    );
}

// Environment tests run in a child process whose environment is exactly the
// listed variables (`child_env`), so no test writes the process environment.

#[test]
fn from_env_defaults_region_and_leaves_credentials_to_aws_sdk_chain() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_defaults_region_and_leaves_credentials_to_aws_sdk_chain",
        &[
            ("AWS_ACCESS_KEY_ID", "env-key"),
            ("AWS_SECRET_ACCESS_KEY", "env-secret"),
            ("AWS_SESSION_TOKEN", "env-token"),
        ],
    ) {
        return;
    }
    let cfg = BedrockConfig::from_env();
    assert_eq!(cfg.aws_region, "us-east-1");
    assert!(cfg.base_url.is_none());
    assert!(cfg.aws_access_key.is_none());
    assert!(cfg.aws_secret_key.is_none());
    assert!(cfg.aws_session_token.is_none());
    assert!(!cfg.skip_auth);
}

#[test]
fn from_env_matches_ts_env_names_and_trimming() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_matches_ts_env_names_and_trimming",
        &[
            ("AWS_DEFAULT_REGION", "ignored-region"),
            ("ANTHROPIC_BEDROCK_BASE_URL", " https://bedrock.local "),
        ],
    ) {
        return;
    }
    let cfg = BedrockConfig::from_env();
    assert_eq!(cfg.aws_region, "us-east-1");
    assert_eq!(cfg.base_url.as_deref(), Some("https://bedrock.local"));
}

#[test]
fn from_env_trims_aws_region() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_trims_aws_region",
        &[("AWS_REGION", " eu-west-3 ")],
    ) {
        return;
    }
    let cfg = BedrockConfig::from_env();
    assert_eq!(cfg.aws_region, "eu-west-3");
}

#[test]
fn base_url_uses_region_or_explicit_override() {
    let client = create_client(bedrock_config("eu-west-1")).unwrap();
    assert_eq!(
        client.base_url(),
        "https://bedrock-runtime.eu-west-1.amazonaws.com"
    );

    let mut cfg = bedrock_config("eu-west-1");
    cfg.base_url = Some("http://localhost:4010".to_owned());
    let client = create_client(cfg.clone()).unwrap();
    assert_eq!(client.base_url(), "http://localhost:4010");

    cfg.base_url = Some(String::new());
    let client = create_client(cfg).unwrap();
    assert_eq!(client.base_url(), "https://api.anthropic.com");
}

#[tokio::test]
async fn request_rejects_empty_region_like_official_prepare_request() {
    let mut cfg = bedrock_config("");
    cfg.skip_auth = false;
    cfg.aws_access_key = Some("AKIDEXAMPLE".to_owned());
    cfg.aws_secret_key = Some("secret".to_owned());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let error = client
        .messages()
        .create(&MessageCreateParams {
            model: "test-model".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "SDK error: Expected `awsRegion` option to be passed to the client or the `AWS_REGION` environment variable to be present"
    );
}

#[test]
fn core_options_are_preserved_like_ts_provider_extends_core_client_options() {
    // The core reads ANTHROPIC_API_KEY for Bedrock, so the host's must not
    // decide the `x-api-key` assertion below.
    if child_env::run_in_child_env(
        module_path!(),
        "core_options_are_preserved_like_ts_provider_extends_core_client_options",
        &[],
    ) {
        return;
    }
    let mut default_headers = std::collections::HashMap::new();
    default_headers.insert("x-bedrock-default".to_owned(), Some("yes".to_owned()));
    let mut default_query = std::collections::HashMap::new();
    default_query.insert("provider_query".to_owned(), Some("1".to_owned()));

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some("https://bedrock.local".to_owned());
    let client = create_client_with_core_options(
        cfg,
        CoreClientOptions {
            max_retries: Some(5),
            timeout: Some(12_345),
            default_headers: Some(default_headers),
            default_query: Some(default_query),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(client.base_url(), "https://bedrock.local");
    assert_eq!(client.max_retries(), 5);
    assert_eq!(client.timeout(), 12_345);
    assert_eq!(
        client
            .build_url("/model/test/invoke", None)
            .unwrap()
            .as_str(),
        "https://bedrock.local/model/test/invoke?provider_query=1"
    );
    let headers = client.build_headers(0, None).unwrap();
    assert_eq!(headers.get("x-bedrock-default").unwrap(), "yes");
    assert!(headers.get("x-api-key").is_none());
}

/// A caller that read the environment itself passes the credentials it read:
/// `Null` sends none and a value replaces the ambient one.
#[test]
fn caller_api_key_and_token_replace_the_core_defaults() {
    if child_env::run_in_child_env(
        module_path!(),
        "caller_api_key_and_token_replace_the_core_defaults",
        &[
            ("ANTHROPIC_API_KEY", "ambient-key"),
            ("ANTHROPIC_AUTH_TOKEN", "ambient-token"),
        ],
    ) {
        return;
    }
    let build = |api_key, auth_token| {
        let mut cfg = bedrock_config("us-east-1");
        cfg.base_url = Some("https://bedrock.local".to_owned());
        create_client_with_core_options(
            cfg,
            CoreClientOptions {
                api_key,
                auth_token,
                ..Default::default()
            },
        )
        .unwrap()
        .build_headers(0, None)
        .unwrap()
    };
    let headers = build(Nullable::Null, Nullable::Null);
    assert!(headers.get("x-api-key").is_none(), "{headers:?}");
    assert!(headers.get("authorization").is_none(), "{headers:?}");
    let headers = build(
        Nullable::Set("explicit-key".into()),
        Nullable::Set("explicit-token".into()),
    );
    assert_eq!(headers.get("x-api-key").unwrap(), "explicit-key");
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer explicit-token"
    );
    let headers = build(Nullable::Unset, Nullable::Unset);
    assert_eq!(headers.get("x-api-key").unwrap(), "ambient-key");
}

#[test]
fn anthropic_bedrock_wrapper_derefs_to_core_client() {
    let client = AnthropicBedrock::new(bedrock_config("us-west-2")).unwrap();
    assert_eq!(
        client.base_url(),
        "https://bedrock-runtime.us-west-2.amazonaws.com"
    );
    assert_eq!(
        client.as_client().base_url(),
        "https://bedrock-runtime.us-west-2.amazonaws.com"
    );
}

#[tokio::test]
async fn inherited_beta_resources_are_sigv4_signed_like_official_prepare_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/test-model"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "test-model",
            "created_at": "2025-01-01T00:00:00Z",
            "display_name": "Test model",
            "type": "model"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.aws_access_key = Some("AKIDEXAMPLE".to_owned());
    cfg.aws_secret_key = Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    let model = client
        .beta()
        .models()
        .retrieve("test-model", None)
        .await
        .unwrap();
    assert_eq!(model.id, "test-model");

    let requests = server.received_requests().await.unwrap();
    let authorization = requests[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(authorization.starts_with("AWS4-HMAC-SHA256 "));
    assert!(requests[0].headers.get("x-amz-date").is_some());
}

/// TS `AnthropicBedrock` passes neither credential to `super`, so the core
/// sends the ambient key next to the SigV4 headers, as in TS and Go. SigV4's
/// `Authorization` replaces the ambient token as in Go. TS merges the request
/// headers after the signature (`client.ts:113`), so there the token
/// overwrites SigV4 and the request fails AWS auth; that is not ported.
#[tokio::test]
async fn ambient_key_is_sent_next_to_sigv4_and_sigv4_replaces_the_token_like_go() {
    if child_env::run_in_child_env(
        module_path!(),
        "ambient_key_is_sent_next_to_sigv4_and_sigv4_replaces_the_token_like_go",
        &[
            ("ANTHROPIC_API_KEY", "ambient-key"),
            ("ANTHROPIC_AUTH_TOKEN", "ambient-token"),
        ],
    ) {
        return;
    }
    let server = MockServer::start().await;
    mount_invoke(&server, 1).await;
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.aws_access_key = Some("AKIDEXAMPLE".to_owned());
    cfg.aws_secret_key = Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    client.messages().create(&hello_message()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].headers.get("x-api-key").unwrap(), "ambient-key");
    assert!(
        requests[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("AWS4-HMAC-SHA256 ")
    );
}

/// `validateHeaders() {}`: under `skipAuth` TS sends a request with no auth
/// header at all.
#[tokio::test]
async fn skip_auth_without_credentials_is_sent_like_ts() {
    if child_env::run_in_child_env(
        module_path!(),
        "skip_auth_without_credentials_is_sent_like_ts",
        &[],
    ) {
        return;
    }
    let server = MockServer::start().await;
    mount_invoke(&server, 1).await;
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    client.messages().create(&hello_message()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("x-api-key").is_none());
    assert!(requests[0].headers.get("authorization").is_none());
}

/// Under `skipAuth` TS sends the core's ambient credentials unchanged.
#[tokio::test]
async fn skip_auth_sends_the_ambient_anthropic_credentials_like_ts() {
    if child_env::run_in_child_env(
        module_path!(),
        "skip_auth_sends_the_ambient_anthropic_credentials_like_ts",
        &[
            ("ANTHROPIC_API_KEY", "ambient-key"),
            ("ANTHROPIC_AUTH_TOKEN", "ambient-token"),
        ],
    ) {
        return;
    }
    let server = MockServer::start().await;
    mount_invoke(&server, 1).await;
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    client.messages().create(&hello_message()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].headers.get("x-api-key").unwrap(), "ambient-key");
    assert_eq!(
        requests[0].headers.get("authorization").unwrap(),
        "Bearer ambient-token"
    );
}

#[tokio::test]
async fn messages_create_uses_the_default_provider_chain_env_credentials() {
    // Without keys or a provider, the client's default chain reads the
    // process environment, as TS reads `process.env`.
    if child_env::run_in_child_env(
        module_path!(),
        "messages_create_uses_the_default_provider_chain_env_credentials",
        &[
            ("AWS_ACCESS_KEY_ID", "AWSCHAINKEY"),
            ("AWS_SECRET_ACCESS_KEY", "aws-chain-secret"),
            ("AWS_SESSION_TOKEN", "aws-chain-session"),
        ],
    ) {
        return;
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    let _ = client
        .messages()
        .create(&MessageCreateParams {
            model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let authorization = requests[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(authorization.contains("Credential=AWSCHAINKEY/"));
    assert_eq!(
        requests[0].headers.get("x-amz-security-token").unwrap(),
        "aws-chain-session"
    );
}

fn completion_params(model: &str) -> CompletionCreateParams {
    CompletionCreateParams {
        model: model.to_owned(),
        max_tokens_to_sample: 16,
        prompt: "\n\nHuman: hello\n\nAssistant:".to_owned(),
        metadata: None,
        stop_sequences: None,
        stream: Some(true),
        temperature: None,
        top_k: None,
        top_p: None,
        betas: None,
    }
}

#[tokio::test]
async fn completions_create_rewrites_model_path_and_body_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/model/anthropic.claude-v2:1/invoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "compl_test",
            "type": "completion",
            "model": "anthropic.claude-v2:1",
            "completion": " hi",
            "stop_reason": "stop_sequence"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let _ = client
        .completions()
        .create(&completion_params("anthropic.claude-v2:1"))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
    assert!(body.get("model").is_none());
    assert!(body.get("stream").is_none());
}

#[tokio::test]
async fn completions_create_moves_beta_header_into_bedrock_body_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/model/anthropic.claude-v2:1/invoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "compl_test",
            "type": "completion",
            "model": "anthropic.claude-v2:1",
            "completion": " hi",
            "stop_reason": "stop_sequence"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let mut params = completion_params("anthropic.claude-v2:1");
    params.betas = Some(vec!["beta-a".to_owned(), "beta-b".to_owned()]);
    let _ = client.completions().create(&params).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["anthropic_beta"],
        serde_json::json!(["beta-a", "beta-b"])
    );
    assert!(body.get("betas").is_none());
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-a,beta-b"
    );
}

#[tokio::test]
async fn completions_options_beta_header_overrides_param_betas_like_ts_build_headers_order() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/model/anthropic.claude-v2:1/invoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "compl_test",
            "type": "completion",
            "model": "anthropic.claude-v2:1",
            "completion": " hi",
            "stop_reason": "stop_sequence"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let mut params = completion_params("anthropic.claude-v2:1");
    params.betas = Some(vec!["param-beta".to_owned()]);
    let options = RequestOptions {
        headers: Some(HashMap::from([(
            "anthropic-beta".to_owned(),
            Some("option-beta,second".to_owned()),
        )])),
        ..Default::default()
    };

    let _ = client
        .completions()
        .create_with_options(&params, Some(&options))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["anthropic_beta"],
        serde_json::json!(["option-beta", "second"])
    );
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "option-beta,second"
    );
}

fn beta_message_params(model: &str) -> BetaMessageCreateParams {
    BetaMessageCreateParams {
        model: model.to_owned(),
        max_tokens: 16,
        messages: vec![BetaMessageParam {
            role: "user".to_owned(),
            content: BetaMessageContent::Text("hello".to_owned()),
        }],
        betas: Some(vec!["beta-flag".to_owned(), "another-beta".to_owned()]),
        container: None,
        context_management: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: None,
        output_config: None,
        output_format: None,
        service_tier: None,
        speed: None,
        stop_sequences: None,
        stream: Some(true),
        system: None,
        temperature: None,
        thinking: None,
        tool_choice: None,
        tools: None,
        top_k: None,
        top_p: None,
    }
}

#[tokio::test]
async fn beta_messages_create_rewrites_path_body_and_beta_values_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let _ = client
        .beta()
        .messages()
        .create(&beta_message_params(
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
        ))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
    assert_eq!(
        body["anthropic_beta"],
        serde_json::json!(["beta-flag", "another-beta"])
    );
    assert!(body.get("model").is_none());
    assert!(body.get("stream").is_none());
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-flag,another-beta"
    );
}

#[tokio::test]
async fn beta_messages_tool_runner_uses_bedrock_rewrite() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let mut create_params = beta_message_params("anthropic.claude-3-5-sonnet-20241022-v2:0");
    create_params.stream = None;
    let mut runner = client.beta().messages().toolRunner(BetaToolRunnerParams {
        create_params,
        tools: vec![],
        max_iterations: Some(1),
        compaction_control: None,
    });
    let _ = runner.run_until_done().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].headers.get("x-stainless-helper").unwrap(),
        "BetaToolRunner"
    );
}

#[tokio::test]
async fn messages_create_with_static_credentials_adds_sigv4_headers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.aws_access_key = Some("AKIDEXAMPLE".to_owned());
    cfg.aws_secret_key = Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned());
    cfg.aws_session_token = Some("session-token".to_owned());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    let _ = client
        .messages()
        .create(&MessageCreateParams {
            model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let headers = &requests[0].headers;
    assert!(
        headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("AWS4-HMAC-SHA256 ")
    );
    assert!(headers.get("x-amz-date").is_some());
    assert!(headers.get("x-amz-content-sha256").is_some());
    assert_eq!(
        headers.get("x-amz-security-token").unwrap(),
        "session-token"
    );
}

#[tokio::test]
async fn messages_create_signs_final_url_with_request_options_query_like_ts_prepare_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.aws_access_key = Some("AKIDEXAMPLE".to_owned());
    cfg.aws_secret_key = Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned());
    cfg.aws_session_token = Some("session-token".to_owned());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg.clone()).unwrap();

    let mut query = HashMap::new();
    query.insert("z".to_owned(), Some("last value".to_owned()));
    query.insert("a".to_owned(), Some("first/value".to_owned()));
    let options = RequestOptions {
        query: Some(query),
        ..Default::default()
    };

    let _ = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("hello".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.url.query(), Some("a=first%2Fvalue&z=last%20value"));
    let amz_date = request.headers.get("x-amz-date").unwrap().to_str().unwrap();
    let actual_authorization = request
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    let signed_url_with_query = format!(
        "{}{}?{}",
        server.uri(),
        request.url.path(),
        request.url.query().unwrap()
    );
    assert_eq!(
        actual_authorization,
        expected_authorization(
            "POST",
            &signed_url_with_query,
            &request.body,
            &cfg,
            amz_date,
        )
    );

    let unsigned_url = format!("{}{}", server.uri(), request.url.path());
    assert_ne!(
        actual_authorization,
        expected_authorization("POST", &unsigned_url, &request.body, &cfg, amz_date),
        "Bedrock SigV4 auth must include request-options query params in the canonical request"
    );
}

#[tokio::test]
async fn messages_create_request_options_path_bypasses_bedrock_model_rewrite_like_ts_build_request()
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/custom/bedrock/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_custom_path",
            "type": "message",
            "role": "assistant",
            "model": "original-model",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let options = RequestOptions {
        path: Some("/custom/bedrock/messages".to_owned()),
        ..Default::default()
    };

    let response = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "original-model".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("original body".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(response.id, "msg_custom_path");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(requests[0].url.path(), "/custom/bedrock/messages");
    assert_eq!(body["model"], "original-model");
    assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
}

#[tokio::test]
async fn messages_create_custom_path_falsy_body_suppresses_body_and_signs_empty_payload_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/custom/bedrock/no-body"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_no_body",
            "type": "message",
            "role": "assistant",
            "model": "original-model",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.aws_access_key = Some("AKIDEXAMPLE".to_owned());
    cfg.aws_secret_key = Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg.clone()).unwrap();

    let options = RequestOptions {
        path: Some("/custom/bedrock/no-body".to_owned()),
        body: Some(serde_json::Value::Null),
        ..Default::default()
    };

    let response = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "original-model".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("generated body".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(response.id, "msg_no_body");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.url.path(), "/custom/bedrock/no-body");
    assert!(request.body.is_empty(), "body was {:?}", request.body);
    assert!(request.headers.get("content-type").is_none());
    assert_eq!(
        request
            .headers
            .get("x-amz-content-sha256")
            .unwrap()
            .to_str()
            .unwrap(),
        sha256_hex(b"")
    );
    let amz_date = request.headers.get("x-amz-date").unwrap().to_str().unwrap();
    let actual_authorization = request
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    let signed_url = format!("{}{}", server.uri(), request.url.path());
    assert_eq!(
        actual_authorization,
        expected_authorization("POST", &signed_url, b"", &cfg, amz_date)
    );
}

#[tokio::test]
async fn messages_create_applies_request_options_body_before_bedrock_rewrite_like_ts_build_request()
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/model/override-model/invoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_override",
            "type": "message",
            "role": "assistant",
            "model": "override-model",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.aws_access_key = Some("AKIDEXAMPLE".to_owned());
    cfg.aws_secret_key = Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg.clone()).unwrap();

    let options = RequestOptions {
        body: Some(serde_json::json!({
            "model": "override-model",
            "max_tokens": 7,
            "messages": [{"role": "user", "content": "override body"}],
            "anthropic_version": "custom-version",
            "anthropic_beta": ["keep-me"]
        })),
        ..Default::default()
    };

    let response = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "original-model".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("original body".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(response.id, "msg_override");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(requests[0].url.path(), "/model/override-model/invoke");
    assert!(body.get("model").is_none());
    assert!(body.get("stream").is_none());
    assert_eq!(body["max_tokens"], 7);
    assert_eq!(body["messages"][0]["content"], "override body");
    assert_eq!(body["anthropic_version"], "custom-version");
    assert_eq!(body["anthropic_beta"], serde_json::json!(["keep-me"]));
    assert_eq!(
        requests[0]
            .headers
            .get("x-amz-content-sha256")
            .unwrap()
            .to_str()
            .unwrap(),
        sha256_hex(&requests[0].body)
    );
    let amz_date = requests[0]
        .headers
        .get("x-amz-date")
        .unwrap()
        .to_str()
        .unwrap();
    let actual_authorization = requests[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    let signed_url = format!("{}{}", server.uri(), requests[0].url.path());
    assert_eq!(
        actual_authorization,
        expected_authorization("POST", &signed_url, &requests[0].body, &cfg, amz_date)
    );
}

struct CountingAwsProvider {
    counter: Arc<AtomicUsize>,
}

impl AwsCredentialProvider for CountingAwsProvider {
    fn get_credentials(
        &self,
    ) -> futures::future::BoxFuture<'_, Result<AwsCredentials, anthropic_sdk::ApiError>> {
        let counter = Arc::clone(&self.counter);
        Box::pin(async move {
            let next = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(AwsCredentials {
                access_key_id: format!("AKIDEXAMPLE{next}"),
                secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned(),
                session_token: Some(format!("provider-session-{next}")),
            })
        })
    }
}

#[tokio::test]
async fn messages_create_uses_custom_aws_credential_provider_per_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(2)
        .mount(&server)
        .await;

    let counter = Arc::new(AtomicUsize::new(0));
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.credential_provider = Some(Arc::new(CountingAwsProvider {
        counter: Arc::clone(&counter),
    }));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    for _ in 0..2 {
        let _ = client
            .messages()
            .create(&MessageCreateParams {
                model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("hello".to_owned()),
                }],
                ..Default::default()
            })
            .await
            .unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), 2);
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Credential=AKIDEXAMPLE1/")
    );
    assert_eq!(
        requests[0].headers.get("x-amz-security-token").unwrap(),
        "provider-session-1"
    );
    assert!(
        requests[1]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Credential=AKIDEXAMPLE2/")
    );
    assert_eq!(
        requests[1].headers.get("x-amz-security-token").unwrap(),
        "provider-session-2"
    );
}

/// A container credentials endpoint (`AWS_CONTAINER_CREDENTIALS_FULL_URI`)
/// that serves one key id per lookup, the last one from then on, each
/// expiring after `lifetime`.
async fn mount_container_credentials(
    server: &MockServer,
    key_ids: &[&str],
    lifetime: std::time::Duration,
    delay: std::time::Duration,
) {
    let expiration = (time::OffsetDateTime::now_utc() + lifetime)
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for (index, key_id) in key_ids.iter().enumerate() {
        let mock = Mock::given(method("GET"))
            .and(path("/credentials"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "AccessKeyId": key_id,
                        "SecretAccessKey": "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
                        "Token": format!("{key_id}-session"),
                        "Expiration": expiration,
                    }))
                    .set_delay(delay),
            );
        let mock = if index + 1 < key_ids.len() {
            mock.up_to_n_times(1)
        } else {
            mock
        };
        mock.mount(server).await;
    }
}

/// The default chain on an environment that names only the container
/// endpoint (no shared files, so the profile links find nothing).
fn container_chain(server: &MockServer) -> Arc<dyn AwsCredentialProvider> {
    Arc::new(from_node_provider_chain(NodeProviderChainOptions {
        env: Environment::new([
            (
                "AWS_CONTAINER_CREDENTIALS_FULL_URI".to_owned(),
                format!("{}/credentials", server.uri()),
            ),
            (
                "AWS_CONFIG_FILE".to_owned(),
                "/nonexistent/anthropic-sdk-rs/config".to_owned(),
            ),
            (
                "AWS_SHARED_CREDENTIALS_FILE".to_owned(),
                "/nonexistent/anthropic-sdk-rs/credentials".to_owned(),
            ),
        ]),
        http_client: None,
        profile: None,
    }))
}

async fn credential_lookups(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == "/credentials")
        .count()
}

async fn mount_invoke(server: &MockServer, times: u64) {
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(times)
        .mount(server)
        .await;
}

fn hello_message() -> MessageCreateParams {
    MessageCreateParams {
        model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
        max_tokens: 16,
        messages: vec![MessageParam {
            role: "user".to_owned(),
            content: MessageContent::Text("hello".to_owned()),
        }],
        ..Default::default()
    }
}

/// A request to a credential source: the container endpoint or the instance
/// metadata service.
fn is_credential_request(request: &wiremock::Request) -> bool {
    let path = request.url.path();
    path == "/credentials" || path.starts_with("/latest/")
}

fn signing_key_ids(requests: &[wiremock::Request]) -> Vec<String> {
    requests
        .iter()
        .filter(|request| !is_credential_request(request))
        .map(|request| {
            let authorization = request
                .headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap();
            let credential = authorization.split("Credential=").nth(1).unwrap();
            credential.split('/').next().unwrap().to_owned()
        })
        .collect()
}

/// npm's `memoizeChain`: credentials that do not expire within five minutes
/// are looked up once and signed with until then.
#[tokio::test]
async fn default_chain_credentials_are_looked_up_once_while_they_last() {
    let server = MockServer::start().await;
    mount_invoke(&server, 2).await;
    mount_container_credentials(
        &server,
        &["AKIDCHAIN1", "AKIDCHAIN2"],
        std::time::Duration::from_secs(3600),
        std::time::Duration::ZERO,
    )
    .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.credential_provider = Some(container_chain(&server));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    for _ in 0..2 {
        client.messages().create(&hello_message()).await.unwrap();
    }

    assert_eq!(credential_lookups(&server).await, 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDCHAIN1", "AKIDCHAIN1"]);
}

/// `memoizeChain`'s `activeLock`: requests that find no credentials wait for
/// the one lookup in flight.
#[tokio::test]
async fn concurrent_requests_share_one_default_chain_lookup() {
    let server = MockServer::start().await;
    mount_invoke(&server, 4).await;
    mount_container_credentials(
        &server,
        &["AKIDCHAIN1", "AKIDCHAIN2"],
        std::time::Duration::from_secs(3600),
        std::time::Duration::from_millis(100),
    )
    .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.credential_provider = Some(container_chain(&server));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    let message = hello_message();
    let messages = client.messages();
    for result in futures::future::join_all((0..4).map(|_| messages.create(&message))).await {
        result.unwrap();
    }

    assert_eq!(credential_lookups(&server).await, 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDCHAIN1"; 4]);
}

/// A failed lookup's error goes to every request that waited for it, as
/// `await activeLock` rejects for all of them; the next request looks up
/// again. The profile's `credential_process` counts the lookups: `fromIni`
/// and then `fromProcess` each run it, so one lookup runs it twice.
#[cfg(unix)]
#[tokio::test]
async fn a_failed_default_chain_lookup_is_shared_then_tried_again() {
    let server = MockServer::start().await;
    let dir = std::env::temp_dir().join(format!(
        "anthropic-sdk-rs-chain-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let counter = dir.join("runs");
    let config = dir.join("config");
    std::fs::write(
        &config,
        format!(
            "[default]\ncredential_process = sleep 0.1; echo run >> '{}'; exit 3\n",
            counter.display()
        ),
    )
    .unwrap();
    let chain = from_node_provider_chain(NodeProviderChainOptions {
        env: Environment::new([
            ("AWS_CONFIG_FILE".to_owned(), config.display().to_string()),
            (
                "AWS_SHARED_CREDENTIALS_FILE".to_owned(),
                dir.join("credentials").display().to_string(),
            ),
            ("AWS_EC2_METADATA_DISABLED".to_owned(), "true".to_owned()),
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        ]),
        http_client: None,
        profile: None,
    });
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.credential_provider = Some(Arc::new(chain));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();
    let runs = || {
        std::fs::read_to_string(&counter)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    };

    let message = hello_message();
    let messages = client.messages();
    for result in futures::future::join_all((0..4).map(|_| messages.create(&message))).await {
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("Could not load credentials from any providers"),
            "{error}"
        );
    }
    assert_eq!(runs(), 2);

    assert!(client.messages().create(&message).await.is_err());
    assert_eq!(runs(), 4);
    assert!(server.received_requests().await.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn wrapper_and_inherited_resources_share_one_default_chain() {
    let server = MockServer::start().await;
    mount_invoke(&server, 1).await;
    Mock::given(method("GET"))
        .and(path("/v1/models/test-model"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "test-model",
            "created_at": "2025-01-01T00:00:00Z",
            "display_name": "Test model",
            "type": "model"
        })))
        .expect(1)
        .mount(&server)
        .await;

    mount_container_credentials(
        &server,
        &["AKIDCHAIN1", "AKIDCHAIN2"],
        std::time::Duration::from_secs(3600),
        std::time::Duration::ZERO,
    )
    .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.credential_provider = Some(container_chain(&server));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    // The wrapper signs `messages`; the core middleware signs the inherited
    // beta resource. Both resolve through the same chain.
    client.messages().create(&hello_message()).await.unwrap();
    client
        .beta()
        .models()
        .retrieve("test-model", None)
        .await
        .unwrap();

    assert_eq!(credential_lookups(&server).await, 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDCHAIN1", "AKIDCHAIN1"]);
}

/// Given neither keys nor a provider, the client builds the default chain
/// once, on the process environment, for the wrapper and the inherited
/// resources alike. Its metadata requests go out directly while the
/// environment names a proxy, as npm's go through Node's `http`, which reads
/// no proxy variables; the model requests take the caller's client, here one
/// without a proxy. The metadata endpoint comes from the shared config file,
/// which the child writes once its server has a port.
#[tokio::test]
async fn the_clients_own_default_chain_sends_metadata_requests_directly() {
    let config = std::env::temp_dir()
        .join(format!(
            "anthropic-sdk-rs-default-chain-{}",
            std::process::id()
        ))
        .join("config");
    let config = config.display().to_string();
    let credentials = format!("{config}-credentials");
    if child_env::run_in_child_env(
        module_path!(),
        "the_clients_own_default_chain_sends_metadata_requests_directly",
        &[
            ("AWS_CONFIG_FILE", &config),
            ("AWS_SHARED_CREDENTIALS_FILE", &credentials),
            // The discard port: nothing listens there.
            ("HTTP_PROXY", "http://127.0.0.1:9"),
        ],
    ) {
        return;
    }

    let config = std::path::PathBuf::from(
        anthropic_sdk::internal::env::read_env("AWS_CONFIG_FILE").unwrap(),
    );
    let dir = config.parent().unwrap().to_owned();
    std::fs::create_dir_all(&dir).unwrap();
    let server = MockServer::start().await;
    // The proxy variable is in force for a client that reads it.
    assert!(
        reqwest::Client::new()
            .get(server.uri())
            .send()
            .await
            .is_err()
    );
    std::fs::write(
        &config,
        format!(
            "[default]\nec2_metadata_service_endpoint = {}\n",
            server.uri()
        ),
    )
    .unwrap();

    Mock::given(method("PUT"))
        .and(path("/latest/api/token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("TOKEN"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/latest/meta-data/iam/security-credentials/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("test-role"))
        .mount(&server)
        .await;
    let expiration = (time::OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    Mock::given(method("GET"))
        .and(path("/latest/meta-data/iam/security-credentials/test-role"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Code": "Success",
            "Type": "AWS-HMAC",
            "AccessKeyId": "AKIDIMDS",
            "SecretAccessKey": "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "Token": "AKIDIMDS-session",
            "Expiration": expiration,
        })))
        .mount(&server)
        .await;
    mount_invoke(&server, 1).await;
    Mock::given(method("GET"))
        .and(path("/v1/models/test-model"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "test-model",
            "created_at": "2025-01-01T00:00:00Z",
            "display_name": "Test model",
            "type": "model"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new_with_core_options(
        cfg,
        CoreClientOptions {
            http_client: Some(reqwest::Client::builder().no_proxy().build().unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    client.messages().create(&hello_message()).await.unwrap();
    client
        .beta()
        .models()
        .retrieve("test-model", None)
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    // One lookup: the token, the role name, its credentials.
    assert_eq!(
        requests
            .iter()
            .filter(|request| is_credential_request(request))
            .count(),
        3
    );
    assert_eq!(signing_key_ids(&requests), ["AKIDIMDS", "AKIDIMDS"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn messages_create_rewrites_model_path_and_body_like_ts() {
    let server = MockServer::start().await;
    let model = "arn:aws:bedrock:us-east-2:1234:inference-profile/us.anthropic.claude-3-7-sonnet-20250219-v1:0";
    Mock::given(method("POST"))
        .and(path("/model/arn:aws:bedrock:us-east-2:1234:inference-profile%2Fus.anthropic.claude-3-7-sonnet-20250219-v1:0/invoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let _ = client
        .messages()
        .create(&MessageCreateParams {
            model: model.to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
    assert!(body.get("model").is_none());
    assert!(body.get("stream").is_none());
}

#[tokio::test]
async fn messages_create_moves_options_beta_header_into_bedrock_body_like_ts_prepare_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let options = RequestOptions {
        headers: Some(HashMap::from([(
            "Anthropic-Beta".to_owned(),
            Some("from-options,second-beta".to_owned()),
        )])),
        ..Default::default()
    };

    let _ = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("hello".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["anthropic_beta"],
        serde_json::json!(["from-options", "second-beta"])
    );
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "from-options,second-beta"
    );
}

#[tokio::test]
async fn provider_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/model/msg-model/invoke"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_bedrock_message")
                .set_body_json(serde_json::json!({
                    "id": "msg_test",
                    "type": "message",
                    "role": "assistant",
                    "model": "msg-model",
                    "content": [],
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/model/compl-model/invoke"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_bedrock_completion")
                .set_body_json(serde_json::json!({
                    "id": "compl_test",
                    "type": "completion",
                    "model": "compl-model",
                    "completion": " hi",
                    "stop_reason": "stop_sequence"
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/model/beta-model/invoke"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_bedrock_beta_message")
                .set_body_json(serde_json::json!({
                    "id": "msg_beta_test",
                    "type": "message",
                    "role": "assistant",
                    "model": "beta-model",
                    "content": [],
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let message = client
        .messages()
        .create_with_response(&MessageCreateParams {
            model: "msg-model".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(message.data.id, "msg_test");
    assert_eq!(message.request_id.as_deref(), Some("req_bedrock_message"));
    assert_eq!(
        message.response.header("request-id"),
        Some("req_bedrock_message")
    );

    let completion = client
        .completions()
        .create_with_response(&completion_params("compl-model"))
        .await
        .unwrap();
    assert_eq!(completion.data.id, "compl_test");
    assert_eq!(
        completion.request_id.as_deref(),
        Some("req_bedrock_completion")
    );

    let beta_message = client
        .beta()
        .messages()
        .create_with_response(&beta_message_params("beta-model"))
        .await
        .unwrap();
    assert_eq!(beta_message.data.id, "msg_beta_test");
    assert_eq!(
        beta_message.request_id.as_deref(),
        Some("req_bedrock_beta_message")
    );
}

#[tokio::test]
async fn provider_stream_with_response_helpers_return_stream_metadata_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/model/msg-stream/invoke-with-response-stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_bedrock_stream")
                .set_body_bytes(Vec::new()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/model/compl-stream/invoke-with-response-stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_bedrock_completion_stream")
                .set_body_bytes(Vec::new()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/model/beta-stream/invoke-with-response-stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_bedrock_beta_stream")
                .set_body_bytes(Vec::new()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let stream = client
        .messages()
        .create_stream_with_response(&MessageCreateParams {
            model: "msg-stream".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(stream.request_id.as_deref(), Some("req_bedrock_stream"));
    assert_eq!(
        stream.response.header("request-id"),
        Some("req_bedrock_stream")
    );
    assert!(stream.response.body.is_empty());

    let completion_stream = client
        .completions()
        .create_stream_with_response(&completion_params("compl-stream"))
        .await
        .unwrap();
    assert_eq!(
        completion_stream.request_id.as_deref(),
        Some("req_bedrock_completion_stream")
    );
    assert!(completion_stream.response.body.is_empty());

    let beta_stream = client
        .beta()
        .messages()
        .stream_with_response(&beta_message_params("beta-stream"))
        .await
        .unwrap();
    assert_eq!(
        beta_stream.request_id.as_deref(),
        Some("req_bedrock_beta_stream")
    );
    assert!(beta_stream.response.body.is_empty());

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    for request in requests {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert!(body.get("stream").is_none());
    }
}

#[tokio::test]
async fn bedrock_core_stream_from_sse_response_alias_decodes_event_stream_like_ts_class() {
    let server = MockServer::start().await;
    let event_payload = serde_json::json!({
        "type": "message_stop"
    });
    let body = bedrock_event_stream_body(vec![bedrock_chunk_frame(&event_payload)]);

    Mock::given(method("GET"))
        .and(path("/bedrock-stream"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(1)
        .mount(&server)
        .await;

    let response = reqwest::Client::new()
        .get(format!("{}/bedrock-stream", server.uri()))
        .send()
        .await
        .unwrap();
    let mut stream = BedrockCoreStream::<MessageStreamEvent>::fromSSEResponse(response);
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        MessageStreamEvent::MessageStop
    ));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn messages_create_stream_decodes_bedrock_event_stream_chunks() {
    let server = MockServer::start().await;
    let event_payload = serde_json::json!({
        "type": "message_start",
        "message": {
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 0}
        }
    });
    let body = bedrock_event_stream_body(vec![bedrock_chunk_frame(&event_payload)]);

    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke-with-response-stream",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let mut stream = client
        .messages()
        .create_stream(&MessageCreateParams {
            model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            stream: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();

    let first = stream.next().await.unwrap().unwrap();
    match first {
        MessageStreamEvent::MessageStart { message } => {
            assert_eq!(message.id, "msg_test");
        }
        other => panic!("expected message_start event, got {other:?}"),
    }
    assert!(stream.next().await.is_none());

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(body.get("stream").is_none());
}

#[tokio::test]
async fn messages_create_stream_surfaces_bedrock_event_stream_errors() {
    let server = MockServer::start().await;
    let body = bedrock_event_stream_body(vec![bedrock_exception_frame(
        "modelStreamErrorException",
        serde_json::json!({"message": "boom"}),
    )]);

    Mock::given(method("POST"))
        .and(path(
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke-with-response-stream",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    let client = AnthropicBedrock::new(cfg).unwrap();

    let mut stream = client
        .messages()
        .create_stream(&MessageCreateParams {
            model: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            stream: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();

    let err = stream.next().await.unwrap().unwrap_err();
    assert!(err.to_string().contains("ModelStreamErrorException"));
}

fn bedrock_event_stream_body(frames: Vec<Vec<u8>>) -> Vec<u8> {
    frames.into_iter().flatten().collect()
}

fn bedrock_chunk_frame(json_payload: &serde_json::Value) -> Vec<u8> {
    let wrapper = serde_json::json!({
        "bytes": BASE64_STANDARD.encode(serde_json::to_vec(json_payload).unwrap()),
    });
    bedrock_event_stream_frame("chunk", &serde_json::to_vec(&wrapper).unwrap())
}

fn bedrock_exception_frame(event_type: &str, json_payload: serde_json::Value) -> Vec<u8> {
    bedrock_event_stream_frame(event_type, &serde_json::to_vec(&json_payload).unwrap())
}

fn bedrock_event_stream_frame(event_type: &str, payload: &[u8]) -> Vec<u8> {
    let headers = bedrock_event_stream_headers(event_type);
    let total_len = (12 + headers.len() + payload.len() + 4) as u32;
    let mut frame = Vec::new();
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    let prelude_crc = crc32fast::hash(&frame);
    frame.extend_from_slice(&prelude_crc.to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(payload);
    let message_crc = crc32fast::hash(&frame);
    frame.extend_from_slice(&message_crc.to_be_bytes());
    frame
}

fn bedrock_event_stream_headers(event_type: &str) -> Vec<u8> {
    let mut out = Vec::new();
    push_bedrock_string_header(&mut out, ":message-type", "event");
    push_bedrock_string_header(&mut out, ":event-type", event_type);
    push_bedrock_string_header(&mut out, ":content-type", "application/json");
    out
}

fn push_bedrock_string_header(out: &mut Vec<u8>, name: &str, value: &str) {
    out.push(name.len() as u8);
    out.extend_from_slice(name.as_bytes());
    out.push(7);
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}
