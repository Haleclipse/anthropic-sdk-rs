use std::time::{Duration, SystemTime};

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::shared_ini_file_loader::{get_profile_name, parse_ini, parse_known_files};
use super::*;

fn env(pairs: &[(&str, &str)]) -> Environment {
    Environment::new(pairs.iter().map(|(key, value)| (*key, *value)))
}

/// An environment with no shared files, plus `pairs`.
fn bare_env(pairs: &[(&str, &str)]) -> Environment {
    let mut all = vec![
        ("AWS_CONFIG_FILE", "/nonexistent/anthropic-sdk-rs/config"),
        (
            "AWS_SHARED_CREDENTIALS_FILE",
            "/nonexistent/anthropic-sdk-rs/credentials",
        ),
    ];
    all.extend_from_slice(pairs);
    env(&all)
}

fn init(env: Environment) -> Init {
    Init {
        env,
        http_client: reqwest::Client::new(),
        profile: None,
    }
}

/// A directory with the given shared files, removed on drop.
struct Files {
    dir: std::path::PathBuf,
}

impl Files {
    fn new(name: &str, config: &str, credentials: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "anthropic-sdk-rs-ini-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config"), config).unwrap();
        std::fs::write(dir.join("credentials"), credentials).unwrap();
        Self { dir }
    }

    fn env(&self, pairs: &[(&str, &str)]) -> Environment {
        let config = self.dir.join("config").display().to_string();
        let credentials = self.dir.join("credentials").display().to_string();
        let mut all: Vec<(String, String)> = vec![
            ("AWS_CONFIG_FILE".to_owned(), config),
            ("AWS_SHARED_CREDENTIALS_FILE".to_owned(), credentials),
            ("AWS_EC2_METADATA_DISABLED".to_owned(), "true".to_owned()),
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        ];
        all.extend(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
        );
        Environment::new(all)
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn rfc3339(time: SystemTime) -> String {
    time::OffsetDateTime::from(time)
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

// ── shared-ini-file-loader ──────────────────────────────────────────────────

#[test]
fn parse_ini_reads_sections_comments_and_sub_sections_like_npm() {
    let parsed = parse_ini(
        "# a comment\r\n\
         [default]\r\n\
         region = us-west-2 ; trailing comment\r\n\
         note = a#b;c\n\
         [profile dev]\n\
         aws_access_key_id=AKIDDEV\n\
         s3 =\n  max_concurrent_requests = 10\n\
         region = eu-west-1\n\
         [sso-session 'corp']\n\
         sso_region = us-east-1\n\
         [unknown thing]\n\
         ignored = yes\n\
         [profile \"two words\"]\n\
         kept = whole name\n",
    )
    .unwrap();
    assert_eq!(parsed["default"]["region"], "us-west-2");
    // A `;` or `#` starts a comment only at the start or after whitespace.
    assert_eq!(parsed["default"]["note"], "a#b;c");
    assert_eq!(parsed["profile.dev"]["aws_access_key_id"], "AKIDDEV");
    assert_eq!(parsed["profile.dev"]["s3.max_concurrent_requests"], "10");
    // An unindented key ends the sub-section.
    assert_eq!(parsed["profile.dev"]["region"], "eu-west-1");
    assert_eq!(parsed["sso-session.corp"]["sso_region"], "us-east-1");
    // A prefix that is no section type drops the section.
    assert!(!parsed.contains_key("unknown thing"));
    assert!(!parsed.contains_key("unknown.thing"));
    // A name the prefix pattern rejects keeps the whole section name.
    assert_eq!(parsed["profile \"two words\""]["kept"], "whole name");
}

#[test]
fn parse_ini_rejects_a_proto_section_like_npm() {
    assert_eq!(
        parse_ini("[__proto__]\nkey = value\n").unwrap_err(),
        "Found invalid profile name \"__proto__\""
    );
    assert!(parse_ini("[profile __proto__]\n").is_err());
}

#[tokio::test]
async fn parse_known_files_overlays_credentials_on_config_like_npm() {
    let files = Files::new(
        "merge",
        "[profile dev]\nregion = eu-west-1\naws_access_key_id = FROMCONFIG\n[dev]\nignored = yes\n",
        "[dev]\naws_access_key_id = FROMCREDENTIALS\naws_secret_access_key = secret\n",
    );
    let profiles = parse_known_files(&files.env(&[])).await;
    // The config file's `[dev]` lacks the `profile ` prefix, so it is dropped.
    assert_eq!(profiles["dev"].get("ignored"), None);
    assert_eq!(profiles["dev"]["region"], "eu-west-1");
    assert_eq!(profiles["dev"]["aws_access_key_id"], "FROMCREDENTIALS");
}

#[test]
fn profile_name_prefers_init_then_aws_profile_then_default() {
    let environment = env(&[("AWS_PROFILE", "from-env")]);
    assert_eq!(
        get_profile_name(Some("from-init"), &environment),
        "from-init"
    );
    assert_eq!(get_profile_name(None, &environment), "from-env");
    // `||`: empty values fall through.
    assert_eq!(
        get_profile_name(Some(""), &env(&[("AWS_PROFILE", "")])),
        "default"
    );
}

// ── credential-provider-env ─────────────────────────────────────────────────

#[test]
fn from_env_needs_both_keys_and_reads_the_optional_ones() {
    let credentials = env::from_env(&env(&[
        ("AWS_ACCESS_KEY_ID", "AKIDENV"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
        ("AWS_SESSION_TOKEN", "session"),
        ("AWS_CREDENTIAL_EXPIRATION", "2099-01-01T00:00:00Z"),
    ]))
    .unwrap();
    assert_eq!(credentials.access_key_id, "AKIDENV");
    assert_eq!(credentials.session_token.as_deref(), Some("session"));
    assert!(credentials.expiration.is_some());

    let error = env::from_env(&env(&[
        ("AWS_ACCESS_KEY_ID", "AKIDENV"),
        ("AWS_SECRET_ACCESS_KEY", ""),
    ]))
    .unwrap_err();
    assert_eq!(
        error.message(),
        "Unable to find environment variable credentials."
    );
    assert!(error.try_next_link());
}

// ── the default chain ───────────────────────────────────────────────────────

#[tokio::test]
async fn the_chain_takes_environment_keys_first() {
    let files = Files::new(
        "env-first",
        "",
        "[default]\naws_access_key_id = AKIDFILE\naws_secret_access_key = secret\n",
    );
    let credentials = default_chain(&init(files.env(&[
        ("AWS_ACCESS_KEY_ID", "AKIDENV"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
    ])))
    .await
    .unwrap();
    assert_eq!(credentials.access_key_id, "AKIDENV");
}

#[tokio::test]
async fn aws_profile_skips_the_environment_keys_for_the_profile() {
    let files = Files::new(
        "profile-skips-env",
        "",
        "[dev]\naws_access_key_id = AKIDDEV\naws_secret_access_key = secret\naws_session_token = token\n",
    );
    let credentials = default_chain(&init(files.env(&[
        ("AWS_PROFILE", "dev"),
        ("AWS_ACCESS_KEY_ID", "AKIDENV"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
    ])))
    .await
    .unwrap();
    assert_eq!(credentials.access_key_id, "AKIDDEV");
    assert_eq!(credentials.session_token.as_deref(), Some("token"));
}

#[tokio::test]
async fn a_profile_that_assumes_a_role_stops_the_chain() {
    let files = Files::new(
        "assume-role",
        "[default]\nrole_arn = arn:aws:iam::123456789012:role/test\nsource_profile = base\n",
        "[base]\naws_access_key_id = AKIDBASE\naws_secret_access_key = secret\n",
    );
    let error = default_chain(&init(files.env(&[]))).await.unwrap_err();
    assert_eq!(
        error.message(),
        "Profile default uses role assumption (role_arn), which is not resolved yet."
    );
    assert!(!error.try_next_link());
}

#[tokio::test]
async fn nothing_configured_ends_with_npms_last_error() {
    let files = Files::new("nothing", "", "");
    let error = default_chain(&init(files.env(&[]))).await.unwrap_err();
    assert_eq!(
        error.message(),
        "Could not load credentials from any providers"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn credential_process_output_is_validated_like_npm() {
    let ok = r#"printf '%s' '{"Version": 1, "AccessKeyId": "AKIDPROC", "SecretAccessKey": "secret", "SessionToken": "token", "Expiration": "2099-01-01T00:00:00Z"}'"#;
    let files = Files::new(
        "process",
        &format!(
            "[default]\ncredential_process = {ok}\n\
             [profile v2]\ncredential_process = printf '%s' '{{\"Version\": 2}}'\n\
             [profile bad]\ncredential_process = printf 'not json'\n\
             [profile expired]\ncredential_process = printf '%s' '{{\"Version\": 1, \"AccessKeyId\": \"A\", \"SecretAccessKey\": \"S\", \"Expiration\": \"2000-01-01T00:00:00Z\"}}'\n\
             [profile fails]\ncredential_process = echo oops >&2; exit 3\n"
        ),
        "",
    );
    let environment = files.env(&[]);
    let profiles = parse_known_files(&environment).await;
    let resolve =
        |name: &'static str| process::resolve_process_credentials(name, &profiles, &environment);

    let credentials = resolve("default").await.unwrap();
    assert_eq!(credentials.access_key_id, "AKIDPROC");
    assert_eq!(credentials.session_token.as_deref(), Some("token"));
    assert!(credentials.expiration.is_some());

    assert_eq!(
        resolve("v2").await.unwrap_err().message(),
        "Profile v2 credential_process did not return Version 1."
    );
    assert_eq!(
        resolve("bad").await.unwrap_err().message(),
        "Profile bad credential_process returned invalid JSON."
    );
    assert_eq!(
        resolve("expired").await.unwrap_err().message(),
        "Profile expired credential_process returned expired credentials."
    );
    let failed = resolve("fails").await.unwrap_err();
    assert!(failed.try_next_link());
    assert_eq!(
        failed.message(),
        "Command failed: echo oops >&2; exit 3\noops\n"
    );
    assert_eq!(
        resolve("missing").await.unwrap_err().message(),
        "Profile missing could not be found in shared credentials file."
    );
}

#[cfg(unix)]
#[tokio::test]
async fn credential_process_sees_only_the_chains_environment() {
    let files = Files::new(
        "process-env",
        r#"[default]
credential_process = printf '{"Version": 1, "AccessKeyId": "%s", "SecretAccessKey": "S"}' "$CHAIN_ONLY"
"#,
        "",
    );
    let credentials = default_chain(&init(files.env(&[("CHAIN_ONLY", "AKIDFROMCHAINENV")])))
        .await
        .unwrap();
    assert_eq!(credentials.access_key_id, "AKIDFROMCHAINENV");
}

/// `JSON.parse(stdout.trim())`: `trim` strips a byte order mark too.
#[cfg(unix)]
#[tokio::test]
async fn credential_process_output_is_trimmed_as_js_trims() {
    let files = Files::new(
        "process-bom",
        r#"[default]
credential_process = printf '\357\273\277 {"Version": 1, "AccessKeyId": "AKIDBOM", "SecretAccessKey": "S"}\n'
"#,
        "",
    );
    let credentials = default_chain(&init(files.env(&[]))).await.unwrap();
    assert_eq!(credentials.access_key_id, "AKIDBOM");
}

// ── credential-provider-http ────────────────────────────────────────────────

fn container_response(key: &str, expiration: SystemTime) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "AccessKeyId": key,
        "SecretAccessKey": "secret",
        "Token": format!("{key}-token"),
        "Expiration": rfc3339(expiration),
    }))
}

#[tokio::test]
async fn container_credentials_come_from_the_full_uri_with_its_token() {
    let server = MockServer::start().await;
    let expiration = SystemTime::now() + Duration::from_secs(3600);
    Mock::given(method("GET"))
        .and(path("/creds"))
        .and(header("authorization", "Bearer container"))
        .respond_with(container_response("AKIDECS", expiration))
        .expect(1)
        .mount(&server)
        .await;
    let full_uri = format!("{}/creds", server.uri());
    let init = init(bare_env(&[
        ("AWS_CONTAINER_CREDENTIALS_FULL_URI", &full_uri),
        ("AWS_CONTAINER_AUTHORIZATION_TOKEN", "Bearer container"),
    ]));
    let endpoint = http::container_endpoint(&init).unwrap();
    let credentials = http::from_http(&init, &endpoint).await.unwrap();
    assert_eq!(credentials.access_key_id, "AKIDECS");
    assert_eq!(credentials.session_token.as_deref(), Some("AKIDECS-token"));
}

/// `fromHttp(init)` checks the URL as the remote provider builds its chain,
/// so a rejected URL is that link's error: the container metadata provider
/// never runs, and the chain moves on to its last link.
#[tokio::test]
async fn a_rejected_container_url_is_the_remote_links_own_error() {
    let init = init(bare_env(&[(
        "AWS_CONTAINER_CREDENTIALS_FULL_URI",
        "http://example.com/creds",
    )]));
    let error = remote_provider(&init).await.unwrap_err();
    assert!(error.message().starts_with("URL not accepted."), "{error}");
    assert!(error.try_next_link());
    assert_eq!(
        default_chain(&init).await.unwrap_err().message(),
        "Could not load credentials from any providers"
    );
}

/// `getCredentials` runs outside `fromHttp`'s `try` (its promise is returned,
/// not awaited): its own checks throw provider errors without a prefix, and
/// `JSON.parse`, a `null` body and the date parser throw errors that stop the
/// chain.
#[tokio::test]
async fn container_response_errors_are_npms() {
    let respond = |status: u16, body: &str| {
        reqwest::Response::from(
            ::http::Response::builder()
                .status(status)
                .body(body.to_owned())
                .unwrap(),
        )
    };
    let error = http::get_credentials(respond(500, "")).await.unwrap_err();
    assert_eq!(error.message(), "Server responded with status: 500");
    assert!(error.try_next_link());

    let error = http::get_credentials(respond(200, r#"{"AccessKeyId": 1}"#))
        .await
        .unwrap_err();
    assert!(
        error
            .message()
            .starts_with("HTTP credential provider response not of the required format"),
        "{error}"
    );
    assert!(error.try_next_link());

    for body in [
        "not json",
        "null",
        // A multi-byte character where the date parser's byte checks pass.
        r#"{"AccessKeyId":"A","SecretAccessKey":"S","Token":"T","Expiration":"2024-01-01T00:00:0éZ"}"#,
        r#"{"AccessKeyId":"A","SecretAccessKey":"S","Token":"T","Expiration":"2030-01-01T00:00:00+00:00"}"#,
    ] {
        let error = http::get_credentials(respond(200, body)).await.unwrap_err();
        assert!(!error.try_next_link(), "{body}: {error}");
    }
}

#[test]
fn container_expirations_are_strict_rfc3339_utc() {
    use super::http::parse_rfc3339_date_time;
    assert!(parse_rfc3339_date_time("2026-10-08T12:34:56Z").is_some());
    assert!(parse_rfc3339_date_time("2026-10-08t12:34:56.123456789z").is_some());
    assert!(parse_rfc3339_date_time("2026-10-08T12:34:56+01:00").is_none());
    assert!(parse_rfc3339_date_time("2026-10-08").is_none());
    assert!(parse_rfc3339_date_time("2024-01-01T00:00:0éZ").is_none());
    // A leap second rolls into the next minute, as `Date.UTC` does.
    assert_eq!(
        parse_rfc3339_date_time("2026-10-08T12:34:60Z"),
        parse_rfc3339_date_time("2026-10-08T12:35:00Z")
    );
}

// ── credential-provider-imds ────────────────────────────────────────────────

const X_TOKEN: &str = "x-aws-ec2-metadata-token";

fn imds_credentials_body(key: &str, expiration: SystemTime) -> serde_json::Value {
    serde_json::json!({
        "Code": "Success",
        "Type": "AWS-HMAC",
        "AccessKeyId": key,
        "SecretAccessKey": "secret",
        "Token": format!("{key}-token"),
        "Expiration": rfc3339(expiration),
    })
}

async fn mount_imds_profile(server: &MockServer, key: &str, expiration: SystemTime) {
    Mock::given(method("GET"))
        .and(path("/latest/meta-data/iam/security-credentials/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("test-role\n"))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/latest/meta-data/iam/security-credentials/test-role"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(imds_credentials_body(key, expiration)),
        )
        .mount(server)
        .await;
}

fn imds_env(server: &MockServer, extra: &[(&str, &str)]) -> Environment {
    let endpoint = format!("{}/ignored-path", server.uri());
    let mut pairs = vec![("AWS_EC2_METADATA_SERVICE_ENDPOINT", endpoint.as_str())];
    pairs.extend_from_slice(extra);
    bare_env(&pairs)
}

#[tokio::test]
async fn instance_metadata_v2_sends_the_token_it_fetched() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/latest/api/token"))
        .and(header("x-aws-ec2-metadata-token-ttl-seconds", "21600"))
        .respond_with(ResponseTemplate::new(200).set_body_string("TOKEN"))
        .mount(&server)
        .await;
    mount_imds_profile(
        &server,
        "AKIDIMDS",
        SystemTime::now() + Duration::from_secs(3600),
    )
    .await;
    let credentials = imds::from_instance_metadata(&init(imds_env(&server, &[])))
        .await
        .unwrap();
    assert_eq!(credentials.access_key_id, "AKIDIMDS");
    let requests = server.received_requests().await.unwrap();
    // The endpoint's own path is dropped, as `{ ...endpoint, path }` does.
    assert_eq!(
        requests[1].url.path(),
        "/latest/meta-data/iam/security-credentials/"
    );
    for request in &requests[1..] {
        assert_eq!(request.headers.get(X_TOKEN).unwrap(), "TOKEN");
    }
}

#[tokio::test]
async fn instance_metadata_falls_back_to_v1_without_a_token() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    mount_imds_profile(
        &server,
        "AKIDV1",
        SystemTime::now() + Duration::from_secs(3600),
    )
    .await;
    let credentials = imds::from_instance_metadata(&init(imds_env(&server, &[])))
        .await
        .unwrap();
    assert_eq!(credentials.access_key_id, "AKIDV1");
    for request in &server.received_requests().await.unwrap()[1..] {
        assert!(request.headers.get(X_TOKEN).is_none());
    }
}

#[tokio::test]
async fn instance_metadata_v1_fallback_can_be_blocked() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let error = imds::from_instance_metadata(&init(imds_env(
        &server,
        &[("AWS_EC2_METADATA_V1_DISABLED", "true")],
    )))
    .await
    .unwrap_err();
    assert_eq!(
        error.message(),
        "AWS EC2 Metadata v1 fallback has been blocked by AWS SDK configuration in the following: [process environment variable (AWS_EC2_METADATA_V1_DISABLED)]."
    );
}

#[tokio::test]
async fn a_token_request_rejected_with_400_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;
    let error = imds::from_instance_metadata(&init(imds_env(&server, &[])))
        .await
        .unwrap_err();
    assert_eq!(error.message(), "EC2 Metadata token request returned error");
}

#[tokio::test]
async fn expired_instance_metadata_credentials_are_extended() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_string("TOKEN"))
        .mount(&server)
        .await;
    mount_imds_profile(
        &server,
        "AKIDOLD",
        SystemTime::now() - Duration::from_secs(60),
    )
    .await;
    let credentials = imds::from_instance_metadata(&init(imds_env(&server, &[])))
        .await
        .unwrap();
    let expiration = credentials.expiration.unwrap();
    assert!(expiration >= SystemTime::now() + Duration::from_secs(299));
    assert!(expiration <= SystemTime::now() + Duration::from_secs(600));
}

#[tokio::test]
async fn an_unknown_endpoint_mode_stops_the_chain() {
    let error = imds::from_instance_metadata(&init(bare_env(&[(
        "AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE",
        "IPv5",
    )])))
    .await
    .unwrap_err();
    assert_eq!(
        error.message(),
        "Unsupported endpoint mode: IPv5. Select from IPv4,IPv6"
    );
    assert!(!error.try_next_link());
}

// ── memoizeChain ────────────────────────────────────────────────────────────

/// Credentials in their last five minutes are answered with while a lookup
/// runs in the background; its result answers from then on.
#[tokio::test]
async fn expiring_credentials_are_refreshed_in_the_background() {
    let server = MockServer::start().await;
    let soon = SystemTime::now() + Duration::from_secs(120);
    Mock::given(method("GET"))
        .and(path("/creds"))
        .respond_with(container_response("AKIDFIRST", soon))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/creds"))
        .respond_with(container_response("AKIDSECOND", soon))
        .mount(&server)
        .await;
    let full_uri = format!("{}/creds", server.uri());
    let chain = from_node_provider_chain(NodeProviderChainOptions {
        env: bare_env(&[("AWS_CONTAINER_CREDENTIALS_FULL_URI", &full_uri)]),
        http_client: None,
        profile: None,
    });
    assert_eq!(chain.provide().await.unwrap().access_key_id, "AKIDFIRST");
    // Starts the background lookup and answers with what it has.
    assert_eq!(chain.provide().await.unwrap().access_key_id, "AKIDFIRST");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if chain.provide().await.unwrap().access_key_id == "AKIDSECOND" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the refresh never landed"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn container_chain(server: &MockServer) -> NodeProviderChain {
    let full_uri = format!("{}/creds", server.uri());
    from_node_provider_chain(NodeProviderChainOptions {
        env: bare_env(&[("AWS_CONTAINER_CREDENTIALS_FULL_URI", &full_uri)]),
        http_client: None,
        profile: None,
    })
}

async fn received(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

/// Waits for `server` to have received `count` requests.
async fn until_received(server: &MockServer, count: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while received(server).await < count {
        assert!(
            std::time::Instant::now() < deadline,
            "request {count} never came"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn one_background_refresh_runs_at_a_time() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/creds"))
        .respond_with(container_response(
            "AKIDFIRST",
            SystemTime::now() + Duration::from_secs(120),
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/creds"))
        .respond_with(
            container_response("AKIDSECOND", SystemTime::now() + Duration::from_secs(3600))
                .set_delay(Duration::from_millis(300)),
        )
        .mount(&server)
        .await;
    let chain = container_chain(&server);
    assert_eq!(chain.provide().await.unwrap().access_key_id, "AKIDFIRST");
    for _ in 0..5 {
        assert_eq!(chain.provide().await.unwrap().access_key_id, "AKIDFIRST");
    }
    until_received(&server, 2).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while chain.provide().await.unwrap().access_key_id != "AKIDSECOND" {
        assert!(
            std::time::Instant::now() < deadline,
            "the refresh never landed"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(received(&server).await, 2);
}

/// npm's lookup promise runs to its end whoever awaits it. Here the endpoint
/// fails once, so the lookup's retry after a second shows that it went on
/// with no caller waiting; the next caller then gets its result.
#[tokio::test]
async fn a_lookup_goes_on_when_its_caller_stops_waiting() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/creds"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/creds"))
        .respond_with(container_response(
            "AKIDRETRIED",
            SystemTime::now() + Duration::from_secs(3600),
        ))
        .mount(&server)
        .await;
    let chain = container_chain(&server);
    tokio::time::timeout(Duration::from_millis(100), chain.provide())
        .await
        .unwrap_err();
    until_received(&server, 2).await;
    assert_eq!(chain.provide().await.unwrap().access_key_id, "AKIDRETRIED");
    assert_eq!(received(&server).await, 2);
}

/// A lookup started on a runtime that then shut down cannot finish: its
/// waiters get an error that stops the chain, and the next call looks up
/// again on its own runtime.
#[tokio::test]
async fn a_lookup_whose_runtime_shut_down_is_an_error_then_tried_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/creds"))
        .respond_with(
            container_response("AKIDAGAIN", SystemTime::now() + Duration::from_secs(3600))
                .set_delay(Duration::from_millis(300)),
        )
        .mount(&server)
        .await;
    let chain = container_chain(&server);
    let other = chain.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(50), other.provide())
                .await
                .unwrap_err();
        });
    })
    .join()
    .unwrap();
    let error = chain.provide().await.unwrap_err();
    assert!(
        error
            .message()
            .starts_with("The AWS credential lookup did not finish"),
        "{error}"
    );
    assert!(!error.try_next_link());
    assert_eq!(chain.provide().await.unwrap().access_key_id, "AKIDAGAIN");
}

#[test]
fn debug_output_shows_no_variable_or_credential() {
    let options = NodeProviderChainOptions {
        env: env(&[("AWS_SECRET_ACCESS_KEY", "secret-in-env")]),
        http_client: Some(reqwest::Client::new()),
        profile: Some("dev".to_owned()),
    };
    let chain = from_node_provider_chain(options.clone());
    chain.inner.state.lock().unwrap().credentials = Some(Credentials {
        access_key_id: "AKIDCACHED".to_owned(),
        secret_access_key: "secret-cached".to_owned(),
        session_token: Some("token-cached".to_owned()),
        expiration: None,
    });
    for printed in [format!("{options:?}"), format!("{chain:?}")] {
        for secret in [
            "AWS_SECRET_ACCESS_KEY",
            "secret-in-env",
            "AKIDCACHED",
            "secret-cached",
            "token-cached",
        ] {
            assert!(!printed.contains(secret), "{printed}");
        }
    }
}

// ── JS built-ins ────────────────────────────────────────────────────────────

/// The results are node's for the same strings.
#[test]
fn dates_are_the_es_date_time_string_format() {
    use super::js::date_from_string;
    let utc = |text: &str| {
        Some(SystemTime::from(
            time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
                .unwrap(),
        ))
    };
    assert_eq!(
        date_from_string("1970-01-02"),
        Some(std::time::UNIX_EPOCH + Duration::from_secs(86_400))
    );
    assert_eq!(
        date_from_string("2026-10-08T00:00:00.5+02:00"),
        utc("2026-10-07T22:00:00.500Z")
    );
    assert_eq!(
        date_from_string("+002026-10-08T00:00:00Z"),
        utc("2026-10-08T00:00:00Z")
    );
    assert_eq!(
        date_from_string("2026-10-08T24:00:00Z"),
        utc("2026-10-09T00:00:00Z")
    );
    assert_eq!(date_from_string("2026-02-30"), utc("2026-03-02T00:00:00Z"));
    assert_eq!(
        date_from_string("2026-10-08T12:34Z"),
        utc("2026-10-08T12:34:00Z")
    );
    for invalid in [
        "-000000-01-01T00:00:00Z",
        "2026-10-08T12:34:60Z",
        // Accepted by V8's fallback parser, outside the format.
        "Thu, 08 Oct 2026 00:00:00 GMT",
        "not a date",
    ] {
        assert_eq!(date_from_string(invalid), None, "{invalid}");
    }
}

#[test]
fn a_date_time_without_an_offset_is_local_time() {
    use chrono::TimeZone;
    let local = chrono::Local
        .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
        .earliest()
        .unwrap();
    assert_eq!(
        super::js::date_from_string("2026-01-02T03:04:05"),
        Some(SystemTime::from(local))
    );
}

/// `new Date(value)` on what `JSON.parse` returns.
#[test]
fn dates_from_json_values_are_new_dates() {
    use super::js::date_from_value;
    use serde_json::json;
    let epoch = std::time::UNIX_EPOCH;
    assert_eq!(
        date_from_value(&json!(-1000)),
        Some(epoch - Duration::from_secs(1))
    );
    assert_eq!(
        date_from_value(&json!(1.9)),
        Some(epoch + Duration::from_millis(1))
    );
    assert_eq!(
        date_from_value(&json!(true)),
        Some(epoch + Duration::from_millis(1))
    );
    assert_eq!(date_from_value(&json!(null)), Some(epoch));
    assert_eq!(date_from_value(&json!(8.64e15 + 1.0)), None);
    assert_eq!(date_from_value(&json!([])), None);
    assert_eq!(
        date_from_value(&json!("1970-01-02")),
        Some(epoch + Duration::from_secs(86_400))
    );
}
