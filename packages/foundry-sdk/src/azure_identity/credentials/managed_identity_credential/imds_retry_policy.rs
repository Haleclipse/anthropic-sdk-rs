//! Maps to: `@azure/identity`
//! `credentials/managedIdentityCredential/imdsRetryPolicy.js`, the extra retry
//! policy `ManagedIdentityCredential` adds to its identity client
//! (`index.js:68-71`).
//!
//! It is added `perCall`, which runs before the pipeline's Retry phase
//! (`@azure/core-client` `serviceClient.js:50-58`,
//! `@typespec/ts-http-runtime` `pipeline.js:68-80`), so it wraps the default
//! retry policy: each of its attempts is a whole [`identity_client::send`]. It
//! applies to every MSAL request, App Service as well as IMDS.

use std::time::Duration;

use crate::azure_identity::identity_client::{
    self, calculate_retry_delay, Request, Response, RetryOptions, SendError,
};

/// `:6`, the exponential strategy's cap.
const DEFAULT_CLIENT_MAX_RETRY_INTERVAL: u64 = 1000 * 64;
/// `:7-11`: at least 70 s in all for a 410.
const MIN_DELAY_FOR_410_MS: u64 = 3000;

/// `index.js:29-33` `msiRetryConfig`, without the `intervalIncrement` no
/// policy reads.
#[derive(Clone, Copy, Debug)]
pub(super) struct MsiRetryConfig {
    pub max_retries: u32,
    pub start_delay_ms: u64,
}

/// Sends `request` through `imdsRetryPolicy` (`:21-42`) as the generic
/// `retryPolicy.js:15-101` runs it: a 404 or 410 is retried with exponential
/// backoff; any other response, and any error, is passed on.
pub(super) async fn send(
    client: &reqwest::Client,
    request: &Request,
    retry_options: RetryOptions,
    config: MsiRetryConfig,
) -> Result<Response, SendError> {
    let mut retry_count: u32 = 0;
    loop {
        let outcome = identity_client::send(client, request, retry_options).await;
        if retry_count >= config.max_retries {
            return outcome;
        }
        let status = match &outcome {
            Ok(response) if matches!(response.status, 404 | 410) => response.status,
            _ => return outcome,
        };
        let initial_delay_ms = if status == 410 {
            MIN_DELAY_FOR_410_MS.max(config.start_delay_ms)
        } else {
            config.start_delay_ms
        };
        // `:34-37`: `calculateRetryDelay(retryCount, { retryDelayInMs,
        // maxRetryDelayInMs })` (`@azure/core-util` `delay.js:29-38`).
        let delay_ms = calculate_retry_delay(
            retry_count,
            RetryOptions {
                retry_delay_ms: initial_delay_ms,
                max_retry_delay_ms: DEFAULT_CLIENT_MAX_RETRY_INTERVAL,
                ..retry_options
            },
        );
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        retry_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_starts_from_the_configured_delay_and_caps_at_64_seconds() {
        for (count, initial, full) in [
            (0, 800, 800),
            (2, 800, 3200),
            (0, 3000, 3000),
            (10, 3000, 64_000),
        ] {
            let delay = calculate_retry_delay(
                count,
                RetryOptions {
                    retry_delay_ms: initial,
                    max_retry_delay_ms: DEFAULT_CLIENT_MAX_RETRY_INTERVAL,
                    ..RetryOptions::IDENTITY_CLIENT
                },
            );
            assert!(
                (full / 2..=full).contains(&delay),
                "{count}/{initial}: {delay}"
            );
        }
    }
}
