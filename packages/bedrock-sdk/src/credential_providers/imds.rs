//! Port of `@smithy/credential-provider-imds` 4.x: `fromContainerMetadata`
//! and `fromInstanceMetadata`, with the `loadConfig` lookups they make
//! (`@smithy/node-config-provider`).
//!
//! npm sends with Node's `http` module; this port sends with the chain's
//! client. `remoteProvider` builds these providers afresh for each lookup, so
//! the state npm keeps in them (`disableFetchToken`, the static stability
//! provider's last credentials) never outlives one lookup and is not kept.

use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::js::{date_from_string, js_trim};
use super::shared_ini_file_loader::{
    config_preferred_profile, get_profile_name, load_shared_config_files,
};
use super::{Credentials, CredentialsProviderError, Environment, Init};

pub(crate) const ENV_CMDS_FULL_URI: &str = "AWS_CONTAINER_CREDENTIALS_FULL_URI";
pub(crate) const ENV_CMDS_RELATIVE_URI: &str = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";
const ENV_CMDS_AUTH_TOKEN: &str = "AWS_CONTAINER_AUTHORIZATION_TOKEN";
pub(crate) const ENV_IMDS_DISABLED: &str = "AWS_EC2_METADATA_DISABLED";
const CMDS_IP: &str = "169.254.170.2";
/// `DEFAULT_TIMEOUT`; `DEFAULT_MAX_RETRIES` is 0, so nothing is retried.
const TIMEOUT: Duration = Duration::from_millis(1000);

const ENV_ENDPOINT_NAME: &str = "AWS_EC2_METADATA_SERVICE_ENDPOINT";
const CONFIG_ENDPOINT_NAME: &str = "ec2_metadata_service_endpoint";
const ENV_ENDPOINT_MODE_NAME: &str = "AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE";
const CONFIG_ENDPOINT_MODE_NAME: &str = "ec2_metadata_service_endpoint_mode";
const ENDPOINT_IPV4: &str = "http://169.254.169.254";
const ENDPOINT_IPV6: &str = "http://[fd00:ec2::254]";

const IMDS_PATH: &str = "/latest/meta-data/iam/security-credentials/";
const IMDS_TOKEN_PATH: &str = "/latest/api/token";
const AWS_EC2_METADATA_V1_DISABLED: &str = "AWS_EC2_METADATA_V1_DISABLED";
const PROFILE_AWS_EC2_METADATA_V1_DISABLED: &str = "ec2_metadata_v1_disabled";
const X_AWS_EC2_METADATA_TOKEN: &str = "x-aws-ec2-metadata-token";

const STATIC_STABILITY_REFRESH_INTERVAL_SECONDS: u64 = 5 * 60;
const STATIC_STABILITY_REFRESH_INTERVAL_JITTER_WINDOW_SECONDS: u64 = 5 * 60;
const STATIC_STABILITY_DOC_URL: &str =
    "https://docs.aws.amazon.com/sdkref/latest/guide/feature-static-credentials.html";

/// The `ProviderError` `httpRequest` rejects with, and the status it carries.
#[derive(Debug)]
struct RequestError {
    error: CredentialsProviderError,
    status: Option<u16>,
}

/// `httpRequest`: the body of a 2xx response.
async fn http_request(
    init: &Init,
    method: reqwest::Method,
    url: url::Url,
    headers: &[(&str, &str)],
) -> Result<Vec<u8>, RequestError> {
    let mut request = init.http_client.request(method, url).timeout(TIMEOUT);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await.map_err(|error| RequestError {
        error: CredentialsProviderError::new(if error.is_timeout() {
            "TimeoutError from instance metadata service"
        } else {
            "Unable to connect to instance metadata service"
        }),
        status: None,
    })?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(RequestError {
            error: CredentialsProviderError::new(
                "Error response received from instance metadata service",
            ),
            status: Some(status),
        });
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|_| RequestError {
            error: CredentialsProviderError::new("Unable to connect to instance metadata service"),
            status: None,
        })
}

/// `isImdsCredentials` / `fromImdsCredentials`; `JSON.parse` throws a
/// `SyntaxError`, which is no provider error.
fn imds_credentials(body: &[u8]) -> Result<Credentials, CredentialsProviderError> {
    let parsed: Value = serde_json::from_slice(body)
        .map_err(|error| CredentialsProviderError::stop(error.to_string()))?;
    let field = |name: &str| parsed.get(name).and_then(Value::as_str);
    match (
        field("AccessKeyId"),
        field("SecretAccessKey"),
        field("Token"),
        field("Expiration"),
    ) {
        (Some(access_key_id), Some(secret_access_key), Some(token), Some(expiration)) => {
            Ok(Credentials {
                access_key_id: access_key_id.to_owned(),
                secret_access_key: secret_access_key.to_owned(),
                session_token: Some(token.to_owned()),
                expiration: date_from_string(expiration),
            })
        }
        _ => Err(CredentialsProviderError::new(
            "Invalid response received from instance metadata service.",
        )),
    }
}

/// `fromContainerMetadata(init)()`.
pub(crate) async fn from_container_metadata(
    init: &Init,
) -> Result<Credentials, CredentialsProviderError> {
    let url = get_cmds_uri(&init.env)?;
    let token = init.env.truthy(ENV_CMDS_AUTH_TOKEN);
    let headers: Vec<(&str, &str)> = token
        .map(|token| ("authorization", token))
        .into_iter()
        .collect();
    let body = http_request(init, reqwest::Method::GET, url, &headers)
        .await
        .map_err(|error| error.error)?;
    imds_credentials(&body)
}

/// `getCmdsUri`.
fn get_cmds_uri(env: &Environment) -> Result<url::Url, CredentialsProviderError> {
    if let Some(relative) = env.truthy(ENV_CMDS_RELATIVE_URI) {
        return url::Url::parse(&format!("http://{CMDS_IP}{relative}"))
            .map_err(|_| CredentialsProviderError::stop("Invalid URL"));
    }
    if let Some(full) = env.truthy(ENV_CMDS_FULL_URI) {
        let parsed = url::Url::parse(full).ok();
        let hostname = parsed.as_ref().and_then(url::Url::host_str);
        if !matches!(hostname, Some("localhost" | "127.0.0.1")) {
            return Err(CredentialsProviderError::stop(format!(
                "{} is not a valid container metadata service hostname",
                hostname.unwrap_or("null")
            )));
        }
        let parsed = parsed.expect("a hostname was found");
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(CredentialsProviderError::stop(format!(
                "{}: is not a valid container metadata service protocol",
                parsed.scheme()
            )));
        }
        return Ok(parsed);
    }
    Err(CredentialsProviderError::stop(format!(
        "The container metadata credential provider cannot be used unless the {ENV_CMDS_RELATIVE_URI} or {ENV_CMDS_FULL_URI} environment variable is set"
    )))
}

/// `loadConfig` for one key: the environment variable when it is set (even
/// empty), else the profile's key, else nothing. The profile is the chain's
/// `profile`, else `AWS_PROFILE`, else `default`.
async fn load_config(
    env: &Environment,
    profile: Option<&str>,
    env_name: &str,
    config_name: &str,
) -> Option<String> {
    if let Some(value) = env.var(env_name) {
        return Some(value.to_owned());
    }
    let files = load_shared_config_files(env).await;
    config_preferred_profile(&files, &get_profile_name(profile, env))
        .get(config_name)
        .cloned()
}

/// `getInstanceMetadataEndpoint`: the configured endpoint, else the one
/// the endpoint mode names. `loadConfig` gets no profile here, so only
/// `AWS_PROFILE` selects it.
async fn get_instance_metadata_endpoint(
    env: &Environment,
) -> Result<url::Url, CredentialsProviderError> {
    let configured = load_config(env, None, ENV_ENDPOINT_NAME, CONFIG_ENDPOINT_NAME)
        .await
        .filter(|endpoint| !endpoint.is_empty());
    let endpoint = match configured {
        Some(endpoint) => endpoint,
        None => {
            let mode = load_config(env, None, ENV_ENDPOINT_MODE_NAME, CONFIG_ENDPOINT_MODE_NAME)
                .await
                .unwrap_or_else(|| "IPv4".to_owned());
            match mode.as_str() {
                "IPv4" => ENDPOINT_IPV4.to_owned(),
                "IPv6" => ENDPOINT_IPV6.to_owned(),
                _ => {
                    return Err(CredentialsProviderError::stop(format!(
                        "Unsupported endpoint mode: {mode}. Select from IPv4,IPv6"
                    )));
                }
            }
        }
    };
    url::Url::parse(&endpoint).map_err(|_| CredentialsProviderError::stop("Invalid URL"))
}

/// The endpoint with `path`, as `httpRequest({ ...endpoint, path })` sends
/// it: the endpoint's own path and query are dropped.
fn endpoint_url(endpoint: &url::Url, path: &str) -> url::Url {
    let mut url = endpoint.clone();
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    url
}

/// `fromInstanceMetadata(init)()`.
pub(crate) async fn from_instance_metadata(
    init: &Init,
) -> Result<Credentials, CredentialsProviderError> {
    let endpoint = get_instance_metadata_endpoint(&init.env).await?;
    let token = http_request(
        init,
        reqwest::Method::PUT,
        endpoint_url(&endpoint, IMDS_TOKEN_PATH),
        &[("x-aws-ec2-metadata-token-ttl-seconds", "21600")],
    )
    .await;
    let credentials = match token {
        Ok(token) => {
            let token = String::from_utf8_lossy(&token).into_owned();
            get_credentials(init, &endpoint, Some(&token)).await?
        }
        // The `ProviderError` itself, its message replaced.
        Err(RequestError {
            status: Some(400), ..
        }) => {
            return Err(CredentialsProviderError::new(
                "EC2 Metadata token request returned error",
            ));
        }
        // npm compares `error.message === "TimeoutError"`, which its own
        // timeout message never equals: every other failure falls back to v1.
        Err(_) => get_credentials(init, &endpoint, None).await?,
    };
    Ok(static_stability(credentials))
}

/// `getCredentials` in `getInstanceMetadataProvider`.
async fn get_credentials(
    init: &Init,
    endpoint: &url::Url,
    token: Option<&str>,
) -> Result<Credentials, CredentialsProviderError> {
    if token.is_none() {
        check_v1_fallback(init).await?;
    }
    let headers: Vec<(&str, &str)> = token
        .map(|token| (X_AWS_EC2_METADATA_TOKEN, token))
        .into_iter()
        .collect();
    let profile = http_request(
        init,
        reqwest::Method::GET,
        endpoint_url(endpoint, IMDS_PATH),
        &headers,
    )
    .await
    .map_err(|error| error.error)?;
    let profile = js_trim(&String::from_utf8_lossy(&profile)).to_owned();
    let body = http_request(
        init,
        reqwest::Method::GET,
        endpoint_url(endpoint, &format!("{IMDS_PATH}{profile}")),
        &headers,
    )
    .await
    .map_err(|error| error.error)?;
    imds_credentials(&body)
}

/// The v1 fallback check: `AWS_EC2_METADATA_V1_DISABLED`, else the profile's
/// `ec2_metadata_v1_disabled`, blocks it when set to anything but `false`.
async fn check_v1_fallback(init: &Init) -> Result<(), CredentialsProviderError> {
    let blocked = |value: &str| !value.is_empty() && value != "false";
    let mut causes = Vec::new();
    if let Some(value) = init.env.var(AWS_EC2_METADATA_V1_DISABLED) {
        if blocked(value) {
            causes.push(format!(
                "process environment variable ({AWS_EC2_METADATA_V1_DISABLED})"
            ));
        }
    } else {
        let files = load_shared_config_files(&init.env).await;
        let profile = config_preferred_profile(
            &files,
            &get_profile_name(init.profile.as_deref(), &init.env),
        );
        if profile
            .get(PROFILE_AWS_EC2_METADATA_V1_DISABLED)
            .is_some_and(|value| blocked(value))
        {
            causes.push(format!(
                "config file profile ({PROFILE_AWS_EC2_METADATA_V1_DISABLED})"
            ));
        }
    }
    if causes.is_empty() {
        return Ok(());
    }
    Err(CredentialsProviderError::new(format!(
        "AWS EC2 Metadata v1 fallback has been blocked by AWS SDK configuration in the following: [{}].",
        causes.join(", ")
    )))
}

/// `staticStabilityProvider`: expired credentials are extended by five to ten
/// minutes, so a lookup during an IMDS outage keeps signing.
fn static_stability(mut credentials: Credentials) -> Credentials {
    if credentials
        .expiration
        .is_some_and(|expiration| expiration < SystemTime::now())
    {
        // `Math.random()`; the jitter needs no quality.
        let jitter = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos()))
            % STATIC_STABILITY_REFRESH_INTERVAL_JITTER_WINDOW_SECONDS;
        let new_expiration = SystemTime::now()
            + Duration::from_secs(STATIC_STABILITY_REFRESH_INTERVAL_SECONDS + jitter);
        let at = time::OffsetDateTime::from(new_expiration)
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        tracing::warn!(
            "Attempting credential expiration extension due to a credential service availability issue. A refresh of these credentials will be attempted after {at}.\nFor more information, please visit: {STATIC_STABILITY_DOC_URL}"
        );
        credentials.expiration = Some(new_expiration);
    }
    credentials
}
