//! Maps to: `@azure/identity` `client/identityClient.js`, the network client
//! the credentials send their token requests through, and the retry policy its
//! pipeline runs (`@typespec/ts-http-runtime` 0.3.4 `policies/retryPolicy.js`,
//! `defaultRetryPolicy.js`, `retryStrategies/throttlingRetryStrategy.js`,
//! `retryStrategies/exponentialRetryStrategy.js`, `util/delay.js`).
//!
//! The transport is the client the caller passed in
//! ([`super::DefaultAzureCredentialOptions::http_client`]), else a default
//! `reqwest::Client`. The npm pipeline has its own proxy policy, which picks
//! `HTTPS_PROXY`, `ALL_PROXY`, then `HTTP_PROXY` for every scheme, matches
//! NO_PROXY without `*`, and sends no client certificate; it is not ported.
//!
//! The authority host checks of its constructor are here too: every
//! credential that builds an identity client runs them.

use std::time::Duration;

use crate::azure_identity::Environment;
use crate::azure_identity::constants::DEFAULT_AUTHORITY_HOST;

/// `identityClient.js:47`.
pub(crate) const INSECURE_AUTHORITY_HOST: &str =
    "The authorityHost address must use the 'https' protocol.";

/// Maps to: `identityClient.js:16-25` `getIdentityClientAuthorityHost`.
/// `authority_host` is `options?.authorityHost`; without it the variable is
/// read with `??`, so an empty one is kept (and fails the `https:` check).
pub(crate) fn get_identity_client_authority_host(
    env: &Environment,
    authority_host: Option<&str>,
) -> String {
    authority_host
        .or_else(|| env.var("AZURE_AUTHORITY_HOST"))
        .unwrap_or(DEFAULT_AUTHORITY_HOST)
        .to_owned()
}

/// Maps to: `identityClient.js:45-48`: the `baseUri` of
/// `new IdentityClient(options)`, which throws unless it starts with
/// `https:`, before anything else is built. `Err` is the thrown `Error`'s
/// message.
pub(crate) fn base_uri(env: &Environment, authority_host: Option<&str>) -> Result<String, String> {
    let base_uri = get_identity_client_authority_host(env, authority_host);
    if base_uri.starts_with("https:") {
        Ok(base_uri)
    } else {
        Err(INSECURE_AUTHORITY_HOST.to_owned())
    }
}

/// The retry options a pipeline is built with.
#[derive(Clone, Copy, Debug)]
pub struct RetryOptions {
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub max_retry_delay_ms: u64,
}

impl RetryOptions {
    /// `IdentityClient`'s `retryOptions: { maxRetries: 3 }`
    /// (`identityClient.js:49-53`), with the exponential strategy's defaults
    /// (`exponentialRetryStrategy.js:6-7`).
    pub(crate) const IDENTITY_CLIENT: Self = Self {
        max_retries: 3,
        retry_delay_ms: 1000,
        max_retry_delay_ms: 64_000,
    };
}

pub(crate) struct Request {
    pub method: reqwest::Method,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    /// A form body, sent as is.
    pub body: Option<String>,
    /// `request.timeout`; none is no timeout (`pipelineRequest.js:30`).
    pub timeout: Option<Duration>,
}

#[derive(Debug)]
pub(crate) struct Response {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub body: String,
}

/// A request that produced no response: the `RestError` without one that the
/// retry strategies see. `code` is the Node system error code for the failures
/// the exponential strategy retries; reqwest does not expose those codes, so a
/// connection failure counts as `ECONNREFUSED` and a timeout as `ETIMEDOUT`.
#[derive(Debug)]
pub(crate) struct SendError {
    pub code: Option<&'static str>,
    pub message: String,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// The client the token requests go through: the one passed in, or a default
/// one.
pub(crate) fn http_client(
    injected: Option<&reqwest::Client>,
) -> Result<reqwest::Client, SendError> {
    match injected {
        Some(client) => Ok(client.clone()),
        None => reqwest::Client::builder()
            .build()
            .map_err(|error| SendError {
                code: None,
                message: error.to_string(),
            }),
    }
}

/// Sends `request` through the retry policy: throttling first, then the
/// exponential strategy (`defaultRetryPolicy.js:9-14`).
pub(crate) async fn send(
    client: &reqwest::Client,
    request: &Request,
    retry: RetryOptions,
) -> Result<Response, SendError> {
    // `retryPolicy.js:16-104`.
    let mut retry_count: u32 = 0;
    loop {
        let outcome = send_once(client, request).await;
        if retry_count >= retry.max_retries {
            return outcome;
        }
        let delay_ms = match &outcome {
            Ok(response) => throttling_retry_after_ms(response).or_else(|| {
                is_exponential_retry_response(response.status)
                    .then(|| calculate_retry_delay(retry_count, retry))
            }),
            // `exponentialRetryStrategy.js:20-22`: a non-system error is
            // thrown as it is.
            Err(error) => error
                .code
                .map(|_| calculate_retry_delay(retry_count, retry)),
        };
        let Some(delay_ms) = delay_ms else {
            return outcome;
        };
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        retry_count += 1;
    }
}

async fn send_once(client: &reqwest::Client, request: &Request) -> Result<Response, SendError> {
    let mut builder = client.request(request.method.clone(), &request.url);
    for (name, value) in &request.headers {
        builder = builder.header(*name, value);
    }
    if let Some(body) = &request.body {
        builder = builder.body(body.clone());
    }
    if let Some(timeout) = request.timeout {
        builder = builder.timeout(timeout);
    }
    let response = builder.send().await.map_err(send_error)?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response.text().await.map_err(send_error)?;
    Ok(Response {
        status,
        headers,
        body,
    })
}

fn send_error(error: reqwest::Error) -> SendError {
    let code = if error.is_timeout() {
        Some("ETIMEDOUT")
    } else if error.is_connect() {
        Some("ECONNREFUSED")
    } else {
        None
    };
    SendError {
        code,
        message: error.to_string(),
    }
}

/// `exponentialRetryStrategy.js:39-45` `isExponentialRetryResponse`.
fn is_exponential_retry_response(status: u16) -> bool {
    (status >= 500 || status == 408) && status != 501 && status != 505
}

/// `throttlingRetryStrategy.js:8-37` `getRetryAfterInMs`, for 429 and 503
/// only. `Number(value)` is read as a decimal float.
fn throttling_retry_after_ms(response: &Response) -> Option<u64> {
    if !matches!(response.status, 429 | 503) {
        return None;
    }
    let header = |name: &str| {
        response
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
    };
    for (name, factor) in [
        ("retry-after-ms", 1.0),
        ("x-ms-retry-after-ms", 1.0),
        ("retry-after", 1000.0),
    ] {
        if let Some(value) = header(name).and_then(|value| value.trim().parse::<f64>().ok()) {
            let delay = value * factor;
            return delay.is_finite().then(|| delay.max(0.0) as u64);
        }
    }
    let date = chrono::DateTime::parse_from_rfc2822(header("retry-after")?).ok()?;
    let diff = date.timestamp_millis() - super::now_ms() as i64;
    Some(diff.max(0) as u64)
}

/// `delay.js:10-19` `calculateRetryDelay`: `d * 2^n`, capped, then a uniform
/// draw from `[d/2, d]`. `@azure/core-util` 1.13.1 `delay.js:29-38` has the
/// same function, which `imdsRetryPolicy` calls. Of `retry`, only the delay
/// and its cap are read (the JS `config` holds just those two).
pub(crate) fn calculate_retry_delay(retry_count: u32, retry: RetryOptions) -> u64 {
    let exponential = retry
        .retry_delay_ms
        .saturating_mul(2u64.saturating_pow(retry_count));
    let clamped = exponential.min(retry.max_retry_delay_ms);
    let half = clamped / 2;
    let jitter = getrandom::u64().unwrap_or(0) % (half + 1);
    half + jitter
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, headers: &[(&str, &str)]) -> Response {
        let mut map = reqwest::header::HeaderMap::new();
        for (name, value) in headers {
            map.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        Response {
            status,
            headers: map,
            body: String::new(),
        }
    }

    #[test]
    fn retry_decisions_match_official_strategies() {
        // 408 and 5xx except 501/505 back off exponentially.
        for status in [408, 500, 502, 503, 504] {
            assert!(is_exponential_retry_response(status), "{status}");
        }
        for status in [200, 400, 401, 404, 429, 501, 505] {
            assert!(!is_exponential_retry_response(status), "{status}");
        }
        // Retry-After applies to 429 and 503 only, in milliseconds for the
        // `-ms` headers and seconds for `Retry-After`.
        assert_eq!(
            throttling_retry_after_ms(&response(429, &[("retry-after", "2")])),
            Some(2000)
        );
        assert_eq!(
            throttling_retry_after_ms(&response(503, &[("retry-after-ms", "150")])),
            Some(150)
        );
        assert_eq!(
            throttling_retry_after_ms(&response(500, &[("retry-after", "2")])),
            None
        );
        assert_eq!(throttling_retry_after_ms(&response(429, &[])), None);
    }

    #[test]
    fn retry_delay_doubles_and_keeps_jitter_in_the_upper_half() {
        let options = RetryOptions::IDENTITY_CLIENT;
        for (count, full) in [(0, 1000), (1, 2000), (2, 4000), (10, 64_000)] {
            let delay = calculate_retry_delay(count, options);
            assert!((full / 2..=full).contains(&delay), "{count}: {delay}");
        }
    }
}
