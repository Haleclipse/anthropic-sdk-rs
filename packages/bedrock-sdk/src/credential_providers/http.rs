//! Port of `@aws-sdk/credential-provider-http` 3.972.x `fromHttp`: the
//! container credentials endpoint (ECS, EKS Pod Identity).
//!
//! npm sends with a `NodeHttpHandler` of its own, which bounds the connection
//! to one second and only warns when a response is slow; this port sends with
//! the chain's client (whose connection timeout stands in for the first) and
//! sets no response timeout. Error texts that embed a Node error's `String(e)`
//! carry the reqwest error's text instead.

use std::time::Duration;

use serde_json::Value;

use super::{Credentials, CredentialsProviderError, Init};

const AWS_CONTAINER_CREDENTIALS_RELATIVE_URI: &str = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";
const DEFAULT_LINK_LOCAL_HOST: &str = "http://169.254.170.2";
const AWS_CONTAINER_CREDENTIALS_FULL_URI: &str = "AWS_CONTAINER_CREDENTIALS_FULL_URI";
const AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE: &str = "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE";
const AWS_CONTAINER_AUTHORIZATION_TOKEN: &str = "AWS_CONTAINER_AUTHORIZATION_TOKEN";
/// `options.timeout ?? 1000`: the delay between attempts.
const RETRY_DELAY: Duration = Duration::from_millis(1000);
/// `options.maxRetries ?? 3`.
const MAX_RETRIES: usize = 3;

/// What `fromHttp(init)` settles before it returns its provider: the URL,
/// checked, and the token sources.
pub(crate) struct ContainerEndpoint<'a> {
    url: url::Url,
    token: Option<&'a str>,
    token_file: Option<&'a str>,
}

/// `fromHttp(init)`, up to the provider it returns. npm runs it as the remote
/// provider builds its chain, so its errors are that link's.
pub(crate) fn container_endpoint(
    init: &Init,
) -> Result<ContainerEndpoint<'_>, CredentialsProviderError> {
    let env = &init.env;
    let relative = env.truthy(AWS_CONTAINER_CREDENTIALS_RELATIVE_URI);
    let full = env.truthy(AWS_CONTAINER_CREDENTIALS_FULL_URI);
    let token = env.truthy(AWS_CONTAINER_AUTHORIZATION_TOKEN);
    let token_file = env.truthy(AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE);
    if relative.is_some() && full.is_some() {
        tracing::warn!(
            "@aws-sdk/credential-provider-http: you have set both awsContainerCredentialsRelativeUri and awsContainerCredentialsFullUri."
        );
        tracing::warn!("awsContainerCredentialsFullUri will take precedence.");
    }
    if token.is_some() && token_file.is_some() {
        tracing::warn!(
            "@aws-sdk/credential-provider-http: you have set both awsContainerAuthorizationToken and awsContainerAuthorizationTokenFile."
        );
        tracing::warn!("awsContainerAuthorizationToken will take precedence.");
    }
    let host = match (full, relative) {
        (Some(full), _) => full.to_owned(),
        (None, Some(relative)) => format!("{DEFAULT_LINK_LOCAL_HOST}{relative}"),
        (None, None) => {
            return Err(CredentialsProviderError::new(
                "No HTTP credential provider host provided.\nSet AWS_CONTAINER_CREDENTIALS_FULL_URI or AWS_CONTAINER_CREDENTIALS_RELATIVE_URI.",
            ));
        }
    };
    // `new URL(host)` throws a TypeError, which is no provider error.
    let url = url::Url::parse(&host).map_err(|_| CredentialsProviderError::stop("Invalid URL"))?;
    check_url(&url)?;
    Ok(ContainerEndpoint {
        url,
        token,
        token_file,
    })
}

/// The provider `fromHttp(init)` returns. `retryWrapper`: the first
/// `MAX_RETRIES` failures are retried after a delay; the last attempt's error
/// is the result.
pub(crate) async fn from_http(
    init: &Init,
    endpoint: &ContainerEndpoint<'_>,
) -> Result<Credentials, CredentialsProviderError> {
    for _ in 0..MAX_RETRIES {
        if let Ok(credentials) = attempt(init, endpoint).await {
            return Ok(credentials);
        }
        tokio::time::sleep(RETRY_DELAY).await;
    }
    attempt(init, endpoint).await
}

async fn attempt(
    init: &Init,
    endpoint: &ContainerEndpoint<'_>,
) -> Result<Credentials, CredentialsProviderError> {
    let mut request = init.http_client.get(endpoint.url.clone());
    if let Some(token) = endpoint.token {
        request = request.header(reqwest::header::AUTHORIZATION, token);
    } else if let Some(token_file) = endpoint.token_file {
        // Read outside npm's `try`, so a read error is no provider error.
        let token = tokio::fs::read(token_file).await.map_err(|error| {
            CredentialsProviderError::stop(format!("{error}, open '{token_file}'"))
        })?;
        request = request.header(
            reqwest::header::AUTHORIZATION,
            String::from_utf8_lossy(&token).into_owned(),
        );
    }
    // npm's `try` covers the request only: its error becomes
    // `new CredentialsProviderError(String(e))`. `getCredentials`' promise is
    // returned, not awaited, inside it, so its errors pass through as they are.
    let response = request.send().await.map_err(|error| {
        CredentialsProviderError::new(if error.is_timeout() {
            format!("TimeoutError: {error}")
        } else {
            format!("Error: {error}")
        })
    })?;
    get_credentials(response).await
}

/// `getCredentials` (`requestHelpers.js`). Its own checks throw provider
/// errors; `JSON.parse`, a `null` body and `parseRfc3339DateTime` throw
/// errors that are none, which stop the chain.
pub(super) async fn get_credentials(
    response: reqwest::Response,
) -> Result<Credentials, CredentialsProviderError> {
    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|error| CredentialsProviderError::stop(error.to_string()))?;
    if status == 200 {
        let parsed: Value = serde_json::from_str(&text)
            .map_err(|error| CredentialsProviderError::stop(format!("SyntaxError: {error}")))?;
        if parsed.is_null() {
            return Err(CredentialsProviderError::stop(
                "TypeError: Cannot read properties of null (reading 'AccessKeyId')",
            ));
        }
        let field = |name: &str| parsed.get(name).and_then(Value::as_str);
        let (Some(access_key_id), Some(secret_access_key), Some(token), Some(expiration)) = (
            field("AccessKeyId"),
            field("SecretAccessKey"),
            field("Token"),
            field("Expiration"),
        ) else {
            return Err(CredentialsProviderError::new(
                "HTTP credential provider response not of the required format, an object matching: { AccessKeyId: string, SecretAccessKey: string, Token: string, Expiration: string(rfc3339) }",
            ));
        };
        let expiration = parse_rfc3339_date_time(expiration).ok_or_else(|| {
            CredentialsProviderError::stop("TypeError: Invalid RFC-3339 date-time value")
        })?;
        return Ok(Credentials {
            access_key_id: access_key_id.to_owned(),
            secret_access_key: secret_access_key.to_owned(),
            session_token: Some(token.to_owned()),
            expiration: Some(expiration),
        });
    }
    Err(CredentialsProviderError::new(format!(
        "Server responded with status: {status}"
    )))
}

/// `@smithy/core`'s `parseRfc3339DateTime`:
/// `^\d{4}-\d{2}-\d{2}[tT]\d{2}:\d{2}:\d{2}(\.\d+)?[zZ]$`, UTC only. A
/// seconds field of 60 is allowed, and rolls into the next minute as
/// `Date.UTC` does.
pub(super) fn parse_rfc3339_date_time(text: &str) -> Option<std::time::SystemTime> {
    if !text.is_ascii() {
        return None;
    }
    let bytes = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = text.get(range)?;
        part.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
        || !matches!(bytes[bytes.len() - 1], b'Z' | b'z')
    {
        return None;
    }
    let fraction = text.get(19..text.len() - 1)?;
    let nanos = match fraction.strip_prefix('.') {
        None if fraction.is_empty() => 0,
        Some(digits) if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) => {
            let padded = format!("{digits:0<9}");
            padded.get(..9)?.parse().ok()?
        }
        _ => return None,
    };
    let month = time::Month::try_from(u8::try_from(digits(5..7)?).ok()?).ok()?;
    let date = time::Date::from_calendar_date(
        i32::try_from(digits(0..4)?).ok()?,
        month,
        u8::try_from(digits(8..10)?).ok()?,
    )
    .ok()?;
    let seconds = u8::try_from(digits(17..19)?).ok()?;
    let time = time::Time::from_hms_nano(
        u8::try_from(digits(11..13)?).ok()?,
        u8::try_from(digits(14..16)?).ok()?,
        seconds.min(59),
        nanos,
    )
    .ok()?;
    let at: std::time::SystemTime = date.with_time(time).assume_utc().into();
    match seconds {
        0..=59 => Some(at),
        60 => at.checked_add(Duration::from_secs(1)),
        _ => None,
    }
}

/// `checkUrl`: HTTPS, or a loopback or container host over HTTP.
fn check_url(url: &url::Url) -> Result<(), CredentialsProviderError> {
    if url.scheme() == "https" {
        return Ok(());
    }
    let hostname = url.host_str().unwrap_or_default();
    if matches!(
        hostname,
        "169.254.170.2" | "169.254.170.23" | "[fd00:ec2::23]"
    ) {
        return Ok(());
    }
    if hostname.contains('[') {
        if hostname == "[::1]" || hostname == "[0000:0000:0000:0000:0000:0000:0000:0001]" {
            return Ok(());
        }
    } else {
        if hostname == "localhost" {
            return Ok(());
        }
        let components: Vec<&str> = hostname.split('.').collect();
        if components.len() == 4
            && components[0] == "127"
            && components[1..].iter().all(|component| {
                parse_int(component).is_some_and(|number| (0..=255).contains(&number))
            })
        {
            return Ok(());
        }
    }
    Err(CredentialsProviderError::new(
        "URL not accepted. It must either be HTTPS or match one of the following:\n  - loopback CIDR 127.0.0.0/8 or [::1/128]\n  - ECS container host 169.254.170.2\n  - EKS container host 169.254.170.23 or [fd00:ec2::23]",
    ))
}

/// `parseInt(text, 10)`: leading whitespace, an optional sign, then the
/// leading digits; `None` for `NaN`.
fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim_start_matches(super::js::is_js_whitespace);
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    let value: i64 = digits.parse().ok()?;
    Some(if negative { -value } else { value })
}
