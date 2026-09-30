//! Application Default Credentials (`GoogleAuth`), against mock token
//! endpoints. Each test runs in a child process with an exact environment: the
//! parent fixes `GOOGLE_APPLICATION_CREDENTIALS`, and the child writes the file
//! once its mock server knows its address.

use std::collections::HashMap;
use std::sync::Arc;

use anthropic_sdk::{
    ClientOptions as CoreClientOptions, MessageContent, MessageCreateParams, MessageParam,
};
use anthropic_sdk_vertex::google_auth::NO_ADC_FOUND;
use anthropic_sdk_vertex::{AnthropicVertex, TokenProvider, VertexConfig};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::Value;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "../../../tests/support/child_env.rs"]
mod child_env;

const MODEL_PATH: &str =
    "/projects/adc-project/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict";

fn credentials_path(test: &str) -> String {
    std::env::temp_dir()
        .join(format!("anthropic-sdk-vertex-adc-{test}.json"))
        .to_string_lossy()
        .into_owned()
}

fn write_credentials(test: &str, json: Value) {
    std::fs::write(credentials_path(test), json.to_string()).unwrap();
}

fn config(project_id: &str, server: &MockServer) -> VertexConfig {
    VertexConfig {
        project_id: project_id.to_owned(),
        region: "us-east5".to_owned(),
        access_token: None,
        token_provider: None,
        base_url: Some(server.uri()),
    }
}

fn hello() -> MessageCreateParams {
    MessageCreateParams {
        model: "claude-sonnet-4-20250514".to_owned(),
        max_tokens: 16,
        messages: vec![MessageParam {
            role: "user".to_owned(),
            content: MessageContent::Text("hello".to_owned()),
        }],
        ..Default::default()
    }
}

async fn mount_model(server: &MockServer, times: u64) {
    Mock::given(method("POST"))
        .and(path(MODEL_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(times)
        .mount(server)
        .await;
}

async fn mount_token(server: &MockServer, route: &str, token: &str, times: u64) {
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "access_token": token, "expires_in": 3600 })),
        )
        .expect(times)
        .mount(server)
        .await;
}

async fn requests_to(server: &MockServer, route: &str) -> Vec<wiremock::Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.url.path() == route)
        .collect()
}

fn form_fields(request: &wiremock::Request) -> HashMap<String, String> {
    url::form_urlencoded::parse(&request.body)
        .into_owned()
        .collect()
}

#[tokio::test]
async fn authorized_user_refreshes_once_and_sends_the_quota_project() {
    let test = "authorized_user_refreshes_once_and_sends_the_quota_project";
    let path_var = credentials_path(test);
    if child_env::run_in_child_env(
        module_path!(),
        test,
        &[("GOOGLE_APPLICATION_CREDENTIALS", &path_var)],
    ) {
        return;
    }
    let server = MockServer::start().await;
    mount_token(&server, "/token", "user-token", 1).await;
    mount_model(&server, 2).await;
    write_credentials(
        test,
        serde_json::json!({
            "type": "authorized_user",
            "client_id": "client-id",
            "client_secret": "client-secret",
            "refresh_token": "refresh-token",
            "quota_project_id": "adc-project",
            "token_uri": format!("{}/token", server.uri()),
        }),
    );

    // No project id: TS falls back to `x-goog-user-project`.
    let client = AnthropicVertex::new(&config("", &server)).unwrap();
    for _ in 0..2 {
        client.messages().create(&hello()).await.unwrap();
    }

    let token_requests = requests_to(&server, "/token").await;
    let fields = form_fields(&token_requests[0]);
    assert_eq!(fields["grant_type"], "refresh_token");
    assert_eq!(fields["refresh_token"], "refresh-token");
    for request in requests_to(&server, MODEL_PATH).await {
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            "Bearer user-token"
        );
        assert_eq!(
            request.headers.get("x-goog-user-project").unwrap(),
            "adc-project"
        );
    }
}

#[tokio::test]
async fn service_account_exchanges_a_signed_jwt_and_backfills_the_project() {
    let test = "service_account_exchanges_a_signed_jwt_and_backfills_the_project";
    let path_var = credentials_path(test);
    if child_env::run_in_child_env(
        module_path!(),
        test,
        &[("GOOGLE_APPLICATION_CREDENTIALS", &path_var)],
    ) {
        return;
    }
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let server = MockServer::start().await;
    mount_token(&server, "/token", "sa-token", 1).await;
    mount_model(&server, 1).await;
    let token_uri = format!("{}/token", server.uri());
    write_credentials(
        test,
        serde_json::json!({
            "type": "service_account",
            "project_id": "adc-project",
            "client_email": "vertex@adc-project.iam.gserviceaccount.com",
            "private_key": include_str!("fixtures/test-service-account-key.pem"),
            "private_key_id": "key-1",
            "token_uri": token_uri,
        }),
    );

    let client = AnthropicVertex::new(&config("", &server)).unwrap();
    client.messages().create(&hello()).await.unwrap();

    let fields = form_fields(&requests_to(&server, "/token").await[0]);
    assert_eq!(
        fields["grant_type"],
        "urn:ietf:params:oauth:grant-type:jwt-bearer"
    );
    let parts: Vec<&str> = fields["assertion"].split('.').collect();
    assert_eq!(parts.len(), 3);
    let jwt_header: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    assert_eq!(jwt_header["alg"], "RS256");
    assert_eq!(jwt_header["kid"], "key-1");
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(claims["iss"], "vertex@adc-project.iam.gserviceaccount.com");
    assert_eq!(claims["aud"], token_uri);
    assert_eq!(
        claims["scope"],
        "https://www.googleapis.com/auth/cloud-platform"
    );
    assert_eq!(URL_SAFE_NO_PAD.decode(parts[2]).unwrap().len(), 256);
    let model = &requests_to(&server, MODEL_PATH).await[0];
    assert_eq!(
        model.headers.get("authorization").unwrap(),
        "Bearer sa-token"
    );
}

#[tokio::test]
async fn impersonated_service_account_uses_the_source_token() {
    let test = "impersonated_service_account_uses_the_source_token";
    let path_var = credentials_path(test);
    if child_env::run_in_child_env(
        module_path!(),
        test,
        &[("GOOGLE_APPLICATION_CREDENTIALS", &path_var)],
    ) {
        return;
    }
    let server = MockServer::start().await;
    mount_token(&server, "/token", "source-token", 1).await;
    let impersonate = "/v1/projects/-/serviceAccounts/target@adc-project.iam.gserviceaccount.com:generateAccessToken";
    Mock::given(method("POST"))
        .and(path(impersonate))
        .and(header("authorization", "Bearer source-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": "impersonated-token",
            "expireTime": "2099-01-01T00:00:00Z",
        })))
        .expect(1)
        .mount(&server)
        .await;
    mount_model(&server, 1).await;
    write_credentials(
        test,
        serde_json::json!({
            "type": "impersonated_service_account",
            "service_account_impersonation_url": format!("{}{impersonate}", server.uri()),
            "delegates": ["delegate@adc-project.iam.gserviceaccount.com"],
            "source_credentials": {
                "type": "authorized_user",
                "client_id": "client-id",
                "client_secret": "client-secret",
                "refresh_token": "refresh-token",
                "token_uri": format!("{}/token", server.uri()),
            },
        }),
    );

    let client = AnthropicVertex::new(&config("adc-project", &server)).unwrap();
    client.messages().create(&hello()).await.unwrap();

    let body: Value =
        serde_json::from_slice(&requests_to(&server, impersonate).await[0].body).unwrap();
    assert_eq!(
        body["delegates"][0],
        "delegate@adc-project.iam.gserviceaccount.com"
    );
    assert_eq!(body["lifetime"], "3600s");
    let model = &requests_to(&server, MODEL_PATH).await[0];
    assert_eq!(
        model.headers.get("authorization").unwrap(),
        "Bearer impersonated-token"
    );
}

#[tokio::test]
async fn external_account_file_source_exchanges_at_sts() {
    let test = "external_account_file_source_exchanges_at_sts";
    let path_var = credentials_path(test);
    if child_env::run_in_child_env(
        module_path!(),
        test,
        &[("GOOGLE_APPLICATION_CREDENTIALS", &path_var)],
    ) {
        return;
    }
    let server = MockServer::start().await;
    mount_token(&server, "/sts", "sts-token", 1).await;
    mount_model(&server, 1).await;
    let subject = std::env::temp_dir().join(format!("anthropic-sdk-vertex-adc-{test}.subject"));
    std::fs::write(&subject, r#"{"id_token":"subject-jwt"}"#).unwrap();
    write_credentials(
        test,
        serde_json::json!({
            "type": "external_account",
            "audience": "//iam.googleapis.com/projects/1/locations/global/workloadIdentityPools/p/providers/github",
            "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
            "token_url": format!("{}/sts", server.uri()),
            "credential_source": {
                "file": subject.to_string_lossy(),
                "format": { "type": "json", "subject_token_field_name": "id_token" },
            },
        }),
    );

    let client = AnthropicVertex::new(&config("adc-project", &server)).unwrap();
    client.messages().create(&hello()).await.unwrap();

    let fields = form_fields(&requests_to(&server, "/sts").await[0]);
    assert_eq!(
        fields["grant_type"],
        "urn:ietf:params:oauth:grant-type:token-exchange"
    );
    assert_eq!(fields["subject_token"], "subject-jwt");
    assert_eq!(
        fields["subject_token_type"],
        "urn:ietf:params:oauth:token-type:jwt"
    );
    assert_eq!(
        fields["requested_token_type"],
        "urn:ietf:params:oauth:token-type:access_token"
    );
    let model = &requests_to(&server, MODEL_PATH).await[0];
    assert_eq!(
        model.headers.get("authorization").unwrap(),
        "Bearer sts-token"
    );
}

#[tokio::test]
async fn metadata_server_supplies_the_token_without_a_credential_file() {
    let test = "metadata_server_supplies_the_token_without_a_credential_file";
    // The parent picks a free port and passes it down; the child, which runs
    // this function again, reads it back and serves the mock there.
    let host = anthropic_sdk::internal::env::read_env("GCE_METADATA_HOST").unwrap_or_else(|| {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().to_string()
    });
    if child_env::run_in_child_env(module_path!(), test, &[("GCE_METADATA_HOST", &host)]) {
        return;
    }
    let listener = std::net::TcpListener::bind(&host).unwrap();
    let server = MockServer::builder().listener(listener).start().await;
    Mock::given(method("GET"))
        .and(path("/computeMetadata/v1/instance"))
        .and(header("metadata-flavor", "Google"))
        .respond_with(ResponseTemplate::new(200).insert_header("Metadata-Flavor", "Google"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/computeMetadata/v1/instance/service-accounts/default/token",
        ))
        .and(header("metadata-flavor", "Google"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "access_token": "mds-token", "expires_in": 3600 }),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    mount_model(&server, 1).await;

    let client = AnthropicVertex::new(&config("adc-project", &server)).unwrap();
    client.messages().create(&hello()).await.unwrap();

    let model = &requests_to(&server, MODEL_PATH).await[0];
    assert_eq!(
        model.headers.get("authorization").unwrap(),
        "Bearer mds-token"
    );
}

#[tokio::test]
async fn no_credentials_report_node_no_adc_found() {
    let test = "no_credentials_report_node_no_adc_found";
    if child_env::run_in_child_env(
        module_path!(),
        test,
        &[("METADATA_SERVER_DETECTION", "none")],
    ) {
        return;
    }
    let server = MockServer::start().await;
    let client = AnthropicVertex::new(&config("adc-project", &server)).unwrap();
    let error = client
        .messages()
        .create(&hello())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(NO_ADC_FOUND), "{error}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn executable_sourced_external_accounts_say_they_are_unsupported() {
    let test = "executable_sourced_external_accounts_say_they_are_unsupported";
    let path_var = credentials_path(test);
    if child_env::run_in_child_env(
        module_path!(),
        test,
        &[("GOOGLE_APPLICATION_CREDENTIALS", &path_var)],
    ) {
        return;
    }
    write_credentials(
        test,
        serde_json::json!({
            "type": "external_account",
            "audience": "aud",
            "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
            "credential_source": { "executable": { "command": "get-token" } },
        }),
    );
    let server = MockServer::start().await;
    let client = AnthropicVertex::new(&config("adc-project", &server)).unwrap();
    let error = client
        .messages()
        .create(&hello())
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Executable-sourced external account credentials are not supported yet"),
        "{error}"
    );
}

struct StaticProvider;

impl TokenProvider for StaticProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, anthropic_sdk::ApiError>> {
        Box::pin(async { Ok("gcp-token".to_owned()) })
    }
}

/// TS `prepareOptions` merges the GCP headers after `defaultHeaders`, so a
/// host's `Authorization` default (Claude Code sets one for third-party
/// providers) never replaces Google's.
#[tokio::test]
async fn gcp_auth_headers_win_over_default_headers_like_ts_prepare_options() {
    let server = MockServer::start().await;
    mount_model(&server, 1).await;
    let mut cfg = config("adc-project", &server);
    cfg.token_provider = Some(Arc::new(StaticProvider));
    let client = AnthropicVertex::new_with_core_options(
        &cfg,
        CoreClientOptions {
            default_headers: Some(HashMap::from([(
                "Authorization".to_owned(),
                Some("Bearer anthropic-token".to_owned()),
            )])),
            ..Default::default()
        },
    )
    .unwrap();

    client.messages().create(&hello()).await.unwrap();

    let model = &requests_to(&server, MODEL_PATH).await[0];
    assert_eq!(
        model.headers.get("authorization").unwrap(),
        "Bearer gcp-token"
    );
}
