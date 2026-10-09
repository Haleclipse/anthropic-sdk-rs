//! The credential against local stand-ins for IMDS and App Service. IMDS is
//! reached through `AZURE_POD_IDENTITY_AUTHORITY_HOST`, or, where the probe
//! must run (that variable skips it), through the test IMDS host.
//!
//! The detected source and the token cache are process-wide, as in MSAL, so
//! the tests take [`isolated`] first: it serializes them and forgets that
//! state.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::managed_identity_application::save_access_token;
use super::*;
use crate::azure_identity::msal::access_token_entity::AccessTokenEntity;
use crate::azure_identity::now_ms;

const SCOPE: &str = "https://cognitiveservices.azure.com/.default";
const RESOURCE: &str = "https://cognitiveservices.azure.com";
const ENCODED_RESOURCE: &str = "https%3A%2F%2Fcognitiveservices.azure.com";
const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded;charset=utf-8";

static PROCESS_STATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Holds the process-wide state for one test, starting from nothing.
async fn isolated() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = PROCESS_STATE.lock().await;
    reset_process_state();
    guard
}

fn env(variables: &[(&str, &str)]) -> Environment {
    Environment::new(variables.iter().copied())
}

/// A client that ignores the host's proxy settings, so the requests reach
/// the local stand-ins.
fn direct_client() -> Option<reqwest::Client> {
    Some(reqwest::Client::builder().no_proxy().build().unwrap())
}

#[derive(Clone, Debug)]
struct Recorded {
    target: String,
    headers: Vec<(String, String)>,
}

impl Recorded {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    fn query(&self) -> Option<&str> {
        self.target.split_once('?').map(|(_, query)| query)
    }
}

struct FakeEndpoint {
    url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl FakeEndpoint {
    fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

/// A local HTTP endpoint that records each request and answers it with
/// `respond(index, request)`, one connection per request.
async fn serve(
    respond: impl Fn(usize, &Recorded) -> (u16, String) + Send + 'static,
) -> FakeEndpoint {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut head = Vec::new();
            let mut chunk = [0u8; 1024];
            while !head.windows(4).any(|window| window == b"\r\n\r\n") {
                match stream.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(count) => head.extend_from_slice(&chunk[..count]),
                }
            }
            let text = String::from_utf8_lossy(&head).into_owned();
            let mut lines = text.split("\r\n");
            let target = lines
                .next()
                .and_then(|line| line.split(' ').nth(1))
                .unwrap_or_default()
                .to_owned();
            let headers = lines
                .filter_map(|line| line.split_once(':'))
                .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
                .collect();
            let request = Recorded { target, headers };
            let index = {
                let mut all = recorded.lock().unwrap();
                all.push(request.clone());
                all.len() - 1
            };
            let (status, body) = respond(index, &request);
            let response = format!(
                "HTTP/1.1 {status} Fake\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    FakeEndpoint { url, requests }
}

/// An address nothing listens on: connecting is refused at once.
fn closed_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{address}")
}

/// An IMDS-style token response, `expires_on` in seconds as a string.
fn token_body(token: &str, expires_in_seconds: u64) -> String {
    let expires_on = now_ms() / 1000 + expires_in_seconds;
    format!(
        r#"{{"access_token":"{token}","expires_on":"{expires_on}","resource":"{RESOURCE}","token_type":"Bearer"}}"#
    )
}

fn credential(
    env: &Environment,
    client_id: Option<&str>,
    send_probe_request: bool,
    max_retries: u32,
) -> ManagedIdentityCredential {
    ManagedIdentityCredential::new(ManagedIdentityOptions {
        env: env.clone(),
        http_client: direct_client(),
        client_id: client_id.map(str::to_owned),
        tenant_id: None,
        send_probe_request,
        retry_options: RetryOptions {
            max_retries,
            retry_delay_ms: 1,
            max_retry_delay_ms: 1,
        },
    })
    .unwrap()
}

fn scopes() -> Vec<String> {
    vec![SCOPE.to_owned()]
}

fn unavailable(message: &str) -> CredentialError {
    CredentialError::Unavailable(message.to_owned())
}

/// `AppService.ts:162-193` and the static `nodeStorage`: the secret header and
/// API version, no probe, and the token kept for every credential in the
/// process.
#[tokio::test]
async fn app_service_sends_the_secret_header_and_the_token_is_cached_for_the_process() {
    let _state = isolated().await;
    let endpoint = serve(|_, _| (200, token_body("app-token", 3600))).await;
    let env = env(&[
        ("IDENTITY_ENDPOINT", &format!("{}/msi/token", endpoint.url)),
        ("IDENTITY_HEADER", "secret"),
    ]);

    let before = now_ms();
    let token = credential(&env, None, true, 0)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(token.token, "app-token");
    assert!(
        (before + 3_598_000..=now_ms() + 3_601_000).contains(&token.expires_on_timestamp),
        "{}",
        token.expires_on_timestamp
    );
    assert_eq!(token.refresh_after_timestamp, None);

    let requests = endpoint.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path(), "/msi/token");
    assert_eq!(
        requests[0].query(),
        Some(format!("api-version=2019-08-01&resource={ENCODED_RESOURCE}").as_str())
    );
    assert_eq!(requests[0].header("X-IDENTITY-HEADER"), Some("secret"));
    assert_eq!(requests[0].header("Content-Type"), Some(FORM_CONTENT_TYPE));
    assert_eq!(requests[0].header("Metadata"), None);

    // Another credential finds the token in the process-wide cache.
    let again = credential(&env, None, true, 0)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.token, "app-token");
    assert_eq!(endpoint.requests().len(), 1);
}

/// `index.js:165-179` and `imdsMsi.js:68-90`: a refused connection, or Docker
/// Desktop's 403, makes IMDS unavailable before any token request.
#[tokio::test]
async fn failed_imds_probe_makes_the_credential_unavailable() {
    let _state = isolated().await;
    let none = Environment::default();
    let expected = unavailable(
        "ManagedIdentityCredential: Authentication failed. Message Attempted to use the IMDS endpoint, but it is not available.",
    );

    *TEST_IMDS_HOST.lock().unwrap() = Some(closed_url());
    let error = credential(&none, None, true, 0)
        .get_token(&scopes())
        .await
        .unwrap_err();
    assert_eq!(error, expected);

    let endpoint = serve(|_, _| {
        (
            403,
            "A socket operation was attempted to an unreachable network.".to_owned(),
        )
    })
    .await;
    *TEST_IMDS_HOST.lock().unwrap() = Some(endpoint.url.clone());
    let error = credential(&none, None, true, 0)
        .get_token(&scopes())
        .await
        .unwrap_err();
    assert_eq!(error, expected);
    assert_eq!(endpoint.requests().len(), 1, "the probe only");
}

/// `imdsMsi.js:16-34` and `Imds.ts:159-192`: the probe goes bare, the token
/// request with `Metadata: true`; the probe runs on every call, the token
/// comes from the cache the second time.
#[tokio::test]
async fn answered_imds_probe_is_followed_by_the_token_request() {
    let _state = isolated().await;
    let none = Environment::default();
    let endpoint = serve(|_, request| match request.query() {
        None => (
            400,
            r#"{"error":"invalid_request","error_description":"Required metadata header not specified"}"#
                .to_owned(),
        ),
        Some(_) => (200, token_body("imds-token", 3600)),
    })
    .await;
    *TEST_IMDS_HOST.lock().unwrap() = Some(endpoint.url.clone());

    let token = credential(&none, None, true, 0)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(token.token, "imds-token");
    let requests = endpoint.requests();
    assert_eq!(requests.len(), 2);
    let (probe, request) = (&requests[0], &requests[1]);
    assert_eq!(probe.target, "/metadata/identity/oauth2/token");
    assert_eq!(probe.header("Accept"), Some("application/json"));
    assert_eq!(probe.header("Metadata"), None);
    assert_eq!(request.path(), "/metadata/identity/oauth2/token");
    assert_eq!(
        request.query(),
        Some(format!("api-version=2018-02-01&resource={ENCODED_RESOURCE}").as_str())
    );
    assert_eq!(request.header("Metadata"), Some("true"));
    assert_eq!(request.header("Content-Type"), Some(FORM_CONTENT_TYPE));

    let again = credential(&none, None, true, 0)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.token, "imds-token");
    let requests = endpoint.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[2].query(),
        None,
        "a second probe, no second token request"
    );
}

/// `Imds.ts:94-121` and `BaseManagedIdentitySource.ts:339-355`: the pod
/// identity host skips the probe and is used in its canonical form; a
/// user-assigned identity is the `client_id` parameter.
#[tokio::test]
async fn user_assigned_client_id_goes_in_the_query_of_the_pod_identity_endpoint() {
    let _state = isolated().await;
    let endpoint = serve(|_, _| (200, token_body("pod-token", 3600))).await;
    let env = env(&[("AZURE_POD_IDENTITY_AUTHORITY_HOST", &endpoint.url)]);

    let token = credential(&env, Some("my-client"), true, 0)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(token.token, "pod-token");
    let requests = endpoint.requests();
    assert_eq!(requests.len(), 1, "no probe");
    assert_eq!(requests[0].path(), "/metadata/identity/oauth2/token/");
    assert_eq!(
        requests[0].query(),
        Some(
            format!("api-version=2018-02-01&resource={ENCODED_RESOURCE}&client_id=my-client")
                .as_str()
        )
    );
}

/// `ManagedIdentityClient.ts:128-188` and `index.js:141-164`: a source this
/// port leaves out is reported, never replaced by IMDS.
#[tokio::test]
async fn unported_sources_are_unavailable_without_falling_back_to_imds() {
    let _state = isolated().await;
    let imds = serve(|_, _| (200, token_body("node-identity", 3600))).await;
    let cases: [(&[&'static str], &str); 5] = [
        (
            &[
                "IDENTITY_ENDPOINT",
                "IDENTITY_HEADER",
                "IDENTITY_SERVER_THUMBPRINT",
            ],
            "ManagedIdentityCredential: ServiceFabric managed identity is not supported by anthropic-sdk-foundry.",
        ),
        (
            &["MSI_ENDPOINT", "MSI_SECRET"],
            "ManagedIdentityCredential: MachineLearning managed identity is not supported by anthropic-sdk-foundry.",
        ),
        (
            &["MSI_ENDPOINT"],
            "ManagedIdentityCredential: CloudShell managed identity is not supported by anthropic-sdk-foundry.",
        ),
        (
            &["IDENTITY_ENDPOINT", "IMDS_ENDPOINT"],
            "ManagedIdentityCredential: AzureArc managed identity is not supported by anthropic-sdk-foundry.",
        ),
        (
            &[
                "AZURE_CLIENT_ID",
                "AZURE_TENANT_ID",
                "AZURE_FEDERATED_TOKEN_FILE",
            ],
            "ManagedIdentityCredential: The token exchange managed identity (AKS workload identity) is not supported by anthropic-sdk-foundry.",
        ),
    ];
    let value = format!("{}/source", imds.url);
    for (variables, message) in cases {
        reset_process_state();
        let mut set: Vec<(&str, &str)> = variables
            .iter()
            .map(|&name| (name, value.as_str()))
            .collect();
        set.push(("AZURE_POD_IDENTITY_AUTHORITY_HOST", &imds.url));
        let error = credential(&env(&set), None, true, 0)
            .get_token(&scopes())
            .await
            .unwrap_err();
        assert_eq!(error, unavailable(message), "{variables:?}");
    }
    assert!(imds.requests().is_empty());
}

/// `index.js:68-116` and `identityClient.js:45-48`: what the official
/// constructor throws for, which the chain then skips. The detected source is
/// kept for the process.
#[test]
fn constructor_refuses_what_the_official_constructor_throws_for() {
    let _state = PROCESS_STATE.blocking_lock();
    reset_process_state();
    let build = |env: &Environment, client_id: Option<&str>| {
        ManagedIdentityCredential::new(ManagedIdentityOptions {
            env: env.clone(),
            http_client: None,
            client_id: client_id.map(str::to_owned),
            tenant_id: None,
            send_probe_request: false,
            retry_options: RetryOptions::IDENTITY_CLIENT,
        })
        .err()
    };

    for host in ["http://login.example", ""] {
        assert_eq!(
            build(&env(&[("AZURE_AUTHORITY_HOST", host)]), None).as_deref(),
            Some("The authorityHost address must use the 'https' protocol.")
        );
    }

    let cloud_shell = env(&[("MSI_ENDPOINT", "http://localhost:50342/oauth2/token")]);
    assert_eq!(
        build(&cloud_shell, None),
        None,
        "a system-assigned identity is fine"
    );
    assert!(
        build(&cloud_shell, Some("my-client"))
            .unwrap()
            .starts_with("ManagedIdentityCredential: Specifying a user-assigned managed identity is not supported for CloudShell")
    );
    // Still Cloud Shell: the name was detected once for the process.
    assert!(build(&Environment::default(), Some("my-client")).is_some());

    reset_process_state();
    let fabric = env(&[
        ("IDENTITY_ENDPOINT", "fabric"),
        ("IDENTITY_HEADER", "fabric"),
        ("IDENTITY_SERVER_THUMBPRINT", "fabric"),
    ]);
    assert_eq!(
        build(&fabric, Some("my-client")),
        Some(format!(
            "ManagedIdentityCredential: {SERVICE_FABRIC_ERROR_MESSAGE}"
        ))
    );
}

/// `index.js:135-138`, `:198-251` and `BaseManagedIdentitySource.ts:284-290`:
/// a failed request or unparsable body is MSAL's `network_error`, an error
/// response a `ServerError`, both rethrown as unavailable; a response without
/// a token is the `AuthenticationRequiredError` that keeps its name.
#[tokio::test]
async fn errors_are_rethrown_as_the_official_get_token_rethrows_them() {
    let _state = isolated().await;
    let network_unreachable = unavailable(
        "ManagedIdentityCredential: Network unreachable. Message: network_error: See https://aka.ms/msal.js.errors#network_error for details",
    );
    let app_service = |endpoint: &str| {
        env(&[
            ("IDENTITY_ENDPOINT", endpoint),
            ("IDENTITY_HEADER", "secret"),
        ])
    };

    let two_scopes = ["a".to_owned(), "b".to_owned()];
    assert_eq!(
        credential(&Environment::default(), None, false, 0)
            .get_token(&two_scopes)
            .await
            .unwrap_err(),
        unavailable(
            r#"ManagedIdentityCredential: Multiple scopes are not supported. Scopes: ["a","b"]"#
        )
    );

    assert_eq!(
        credential(&app_service(&closed_url()), None, false, 0)
            .get_token(&scopes())
            .await
            .unwrap_err(),
        network_unreachable
    );

    for (status, body, expected) in [
        (
            400,
            r#"{"error":"invalid_resource","error_description":"bad resource"}"#,
            unavailable(
                "ManagedIdentityCredential: Authentication failed. Message invalid_resource: Error(s): Not Available - Timestamp: Not Available - Description: bad resource - Correlation ID: Not Available - Trace ID: Not Available",
            ),
        ),
        (200, "<html>not json</html>", network_unreachable.clone()),
        (
            200,
            "{}",
            CredentialError::AuthenticationRequired(
                "Response had no \"expiresOn\" property.".to_owned(),
            ),
        ),
    ] {
        reset_process_state();
        let endpoint = serve(move |_, _| (status, body.to_owned())).await;
        assert_eq!(
            credential(&app_service(&endpoint.url), None, false, 0)
                .get_token(&scopes())
                .await
                .unwrap_err(),
            expected,
            "{status} {body}"
        );
    }
}

/// `imdsRetryPolicy.js:21-42`: a 404 is retried, up to the configured count.
#[tokio::test]
async fn imds_retry_policy_retries_a_404() {
    let _state = isolated().await;
    let endpoint = serve(|index, _| match index {
        0 => (404, "{}".to_owned()),
        _ => (200, token_body("after-404", 3600)),
    })
    .await;
    let env = env(&[("AZURE_POD_IDENTITY_AUTHORITY_HOST", &endpoint.url)]);

    let token = credential(&env, None, false, 1)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(token.token, "after-404");
    assert_eq!(endpoint.requests().len(), 2);
}

/// `ManagedIdentityApplication.ts:204-225`: past its refresh time a cached
/// token is refreshed, but the call returns the cached one; the next call gets
/// the new one from the cache.
#[tokio::test]
async fn token_past_its_refresh_time_is_refreshed_and_the_cached_one_returned() {
    let _state = isolated().await;
    let endpoint = serve(|_, _| (200, token_body("fresh", 3600))).await;
    let env = env(&[
        ("IDENTITY_ENDPOINT", &endpoint.url),
        ("IDENTITY_HEADER", "secret"),
    ]);
    let now = (now_ms() / 1000) as f64;
    save_access_token(AccessTokenEntity {
        client_id: "system_assigned_managed_identity".to_owned(),
        target: RESOURCE.to_owned(),
        secret: "stale".to_owned(),
        expires_on: now + 3000.0,
        refresh_on: Some(now - 10.0),
    });

    let token = credential(&env, None, false, 0)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(token.token, "stale");
    assert_eq!(
        token.refresh_after_timestamp,
        Some(((now - 10.0) * 1000.0) as u64)
    );
    assert_eq!(endpoint.requests().len(), 1);

    let token = credential(&env, None, false, 0)
        .get_token(&scopes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(token.token, "fresh");
    assert_eq!(endpoint.requests().len(), 1);
}
