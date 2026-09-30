use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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
use anthropic_sdk_bedrock::{
    create_client, create_client_with_core_options, get_auth_headers, rewrite_url,
    AnthropicBedrock, AwsCredentialProvider, AwsCredentials, BedrockConfig,
    ClientOptions as BedrockClientOptions, ANTHROPIC_VERSION,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
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
        sdk_config: None,
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
        parsed.path(),
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
        sdk_config: None,
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
    assert!(authorization
        .contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token"));
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
    assert!(requests[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("AWS4-HMAC-SHA256 "));
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
async fn messages_create_uses_aws_sdk_default_provider_chain_env_credentials() {
    // Beyond `from_env`, the AWS SDK's own credential chain reads the process
    // environment here; no SDK option can stand in for it.
    if child_env::run_in_child_env(
        module_path!(),
        "messages_create_uses_aws_sdk_default_provider_chain_env_credentials",
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
    assert!(headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("AWS4-HMAC-SHA256 "));
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
    assert!(requests[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("Credential=AKIDEXAMPLE1/"));
    assert_eq!(
        requests[0].headers.get("x-amz-security-token").unwrap(),
        "provider-session-1"
    );
    assert!(requests[1]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("Credential=AKIDEXAMPLE2/"));
    assert_eq!(
        requests[1].headers.get("x-amz-security-token").unwrap(),
        "provider-session-2"
    );
}

/// An AWS SDK credentials provider that counts its lookups. `lifetime` sets
/// each credential's expiry; `None` means it never expires.
#[derive(Debug)]
struct CountingSdkProvider {
    counter: Arc<AtomicUsize>,
    lifetime: Option<std::time::Duration>,
}

impl aws_credential_types::provider::ProvideCredentials for CountingSdkProvider {
    fn provide_credentials<'a>(
        &'a self,
    ) -> aws_credential_types::provider::future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        let next = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        aws_credential_types::provider::future::ProvideCredentials::ready(Ok(
            aws_credential_types::Credentials::new(
                format!("AKIDSDK{next}"),
                "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
                Some(format!("sdk-session-{next}")),
                self.lifetime
                    .map(|lifetime| std::time::SystemTime::now() + lifetime),
                "counting",
            ),
        ))
    }
}

fn sdk_config_with(provider: CountingSdkProvider) -> aws_config::SdkConfig {
    aws_config::SdkConfig::builder()
        .credentials_provider(
            aws_credential_types::provider::SharedCredentialsProvider::new(provider),
        )
        .build()
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

fn signing_key_ids(requests: &[wiremock::Request]) -> Vec<String> {
    requests
        .iter()
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

#[tokio::test]
async fn sdk_config_credentials_are_loaded_once_and_reused_until_expiry() {
    let server = MockServer::start().await;
    mount_invoke(&server, 2).await;

    let counter = Arc::new(AtomicUsize::new(0));
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.sdk_config = Some(sdk_config_with(CountingSdkProvider {
        counter: Arc::clone(&counter),
        lifetime: None,
    }));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    for _ in 0..2 {
        client.messages().create(&hello_message()).await.unwrap();
    }

    // Go `WithConfig` + `aws.CredentialsCache`: credentials without an
    // expiry are fetched once.
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDSDK1", "AKIDSDK1"]);
    assert_eq!(
        requests[1].headers.get("x-amz-security-token").unwrap(),
        "sdk-session-1"
    );
}

#[tokio::test]
async fn expiring_credentials_are_reused_within_their_lifetime() {
    let server = MockServer::start().await;
    mount_invoke(&server, 2).await;

    let counter = Arc::new(AtomicUsize::new(0));
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.sdk_config = Some(sdk_config_with(CountingSdkProvider {
        counter: Arc::clone(&counter),
        // IMDS, SSO and STS credentials carry an expiry like this one.
        lifetime: Some(std::time::Duration::from_secs(3600)),
    }));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    for _ in 0..2 {
        client.messages().create(&hello_message()).await.unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDSDK1", "AKIDSDK1"]);
}

/// An AWS SDK credentials provider whose every lookup fails after a delay.
#[derive(Debug)]
struct FailingSdkProvider {
    counter: Arc<AtomicUsize>,
}

impl aws_credential_types::provider::ProvideCredentials for FailingSdkProvider {
    fn provide_credentials<'a>(
        &'a self,
    ) -> aws_credential_types::provider::future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        let counter = Arc::clone(&self.counter);
        aws_credential_types::provider::future::ProvideCredentials::new(async move {
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            Err(
                aws_credential_types::provider::error::CredentialsError::provider_error(
                    "sso session expired",
                ),
            )
        })
    }
}

#[tokio::test]
async fn concurrent_requests_share_one_failed_lookup() {
    let server = MockServer::start().await;
    let counter = Arc::new(AtomicUsize::new(0));
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.sdk_config = Some(
        aws_config::SdkConfig::builder()
            .credentials_provider(
                aws_credential_types::provider::SharedCredentialsProvider::new(
                    FailingSdkProvider {
                        counter: Arc::clone(&counter),
                    },
                ),
            )
            .build(),
    );
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    let message = hello_message();
    let messages = client.messages();
    let results = futures::future::join_all((0..4).map(|_| messages.create(&message))).await;

    // Go's singleflight: requests that waited for the lookup take its error
    // instead of each running their own.
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    for result in results {
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("from sdk_config") && error.contains("sso session expired"),
            "{error}"
        );
    }

    // A request that starts after the failure looks up again.
    assert!(client.messages().create(&message).await.is_err());
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn credentials_inside_the_refresh_buffer_are_fetched_again() {
    let server = MockServer::start().await;
    mount_invoke(&server, 2).await;

    let counter = Arc::new(AtomicUsize::new(0));
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.sdk_config = Some(sdk_config_with(CountingSdkProvider {
        counter: Arc::clone(&counter),
        // Expires before the refresh buffer elapses.
        lifetime: Some(anthropic_sdk_bedrock::CREDENTIALS_REFRESH_BUFFER / 2),
    }));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    for _ in 0..2 {
        client.messages().create(&hello_message()).await.unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), 2);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDSDK1", "AKIDSDK2"]);
}

#[tokio::test]
async fn custom_credential_provider_takes_precedence_over_sdk_config() {
    let server = MockServer::start().await;
    mount_invoke(&server, 1).await;

    let custom = Arc::new(AtomicUsize::new(0));
    let sdk = Arc::new(AtomicUsize::new(0));
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.credential_provider = Some(Arc::new(CountingAwsProvider {
        counter: Arc::clone(&custom),
    }));
    cfg.sdk_config = Some(sdk_config_with(CountingSdkProvider {
        counter: Arc::clone(&sdk),
        lifetime: None,
    }));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    client.messages().create(&hello_message()).await.unwrap();

    assert_eq!(custom.load(Ordering::SeqCst), 1);
    assert_eq!(sdk.load(Ordering::SeqCst), 0);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDEXAMPLE1"]);
}

#[tokio::test]
async fn wrapper_and_inherited_resources_share_one_credentials_cache() {
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

    let counter = Arc::new(AtomicUsize::new(0));
    let mut cfg = bedrock_config("us-east-1");
    cfg.base_url = Some(server.uri());
    cfg.sdk_config = Some(sdk_config_with(CountingSdkProvider {
        counter: Arc::clone(&counter),
        lifetime: None,
    }));
    cfg.skip_auth = false;
    let client = AnthropicBedrock::new(cfg).unwrap();

    // The wrapper signs `messages`; the core middleware signs the inherited
    // beta resource. Both read the same cache.
    client.messages().create(&hello_message()).await.unwrap();
    client
        .beta()
        .models()
        .retrieve("test-model", None)
        .await
        .unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(signing_key_ids(&requests), ["AKIDSDK1", "AKIDSDK1"]);
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
