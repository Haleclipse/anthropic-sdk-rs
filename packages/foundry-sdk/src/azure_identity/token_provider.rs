//! Maps to: `@azure/identity` `tokenProvider.js` `getBearerTokenProvider`, and
//! the token cycler its bearer policy keeps (`@azure/core-rest-pipeline`
//! 1.23.0 `util/tokenCycler.js`, used by
//! `policies/bearerTokenAuthenticationPolicy.js` with `{ enableCae: true }`).
//!
//! The cycler's token lives as long as the provider. Claude Code builds one
//! provider per `getAnthropicClient()` (`services/api/client.ts:203-210`).

use std::sync::{Arc, Mutex};

use futures::FutureExt;
use futures::future::{BoxFuture, Shared};

use super::{AccessToken, CredentialError, TokenCredential, now_ms};

/// `DEFAULT_CYCLER_OPTIONS` (`tokenCycler.js:6-10`).
const FORCED_REFRESH_WINDOW_MS: u64 = 1000;
const RETRY_INTERVAL_MS: u64 = 3000;
const REFRESH_WINDOW_MS: u64 = 2 * 60 * 1000;

type RefreshWorker = Shared<BoxFuture<'static, Result<AccessToken, CredentialError>>>;

#[derive(Default)]
struct CyclerState {
    token: Option<AccessToken>,
    refresh_worker: Option<RefreshWorker>,
}

/// Maps to: `getBearerTokenProvider(credential, scopes)`. Each
/// [`BearerTokenProvider::get_token`] is one call of the returned function.
pub struct BearerTokenProvider {
    credential: Arc<dyn TokenCredential>,
    scopes: Arc<[String]>,
    state: Arc<Mutex<CyclerState>>,
}

/// Maps to: `tokenProvider.js:26-51` `getBearerTokenProvider`.
pub fn get_bearer_token_provider(
    credential: Arc<dyn TokenCredential>,
    scope: &str,
) -> BearerTokenProvider {
    BearerTokenProvider {
        credential,
        scopes: Arc::from([scope.to_owned()]),
        state: Arc::default(),
    }
}

impl BearerTokenProvider {
    /// `getRefreshedToken()`: the token the bearer policy would put in its
    /// `Authorization` header, or its error.
    pub async fn get_token(&self) -> Result<String, CredentialError> {
        let token = self.cycled_token().await?;
        // `tokenProvider.js:42-46`: `authorization.split(" ")[1]`, falsy throws.
        let token = token.token.split(' ').next().unwrap_or_default().to_owned();
        if token.is_empty() {
            return Err(CredentialError::Other(
                "Failed to get access token".to_owned(),
            ));
        }
        Ok(token)
    }

    /// The function `createTokenCycler` returns (`tokenCycler.js:136-159`),
    /// without the tenant and claim-challenge inputs CC never passes.
    async fn cycled_token(&self) -> Result<AccessToken, CredentialError> {
        let worker = {
            let mut state = self.state.lock().unwrap();
            let now = now_ms();
            match state.token.clone() {
                // Not `mustRefresh` (a token with more than a second left):
                // the cached token is returned. `shouldRefresh` (not already
                // refreshing, and past `refreshAfterTimestamp` or inside the
                // 2-minute window) starts a refresh nobody waits for.
                Some(current)
                    if current
                        .expires_on_timestamp
                        .saturating_sub(FORCED_REFRESH_WINDOW_MS)
                        >= now =>
                {
                    let should_refresh = state.refresh_worker.is_none()
                        && (current
                            .refresh_after_timestamp
                            .is_some_and(|refresh_after| refresh_after < now)
                            || current
                                .expires_on_timestamp
                                .saturating_sub(REFRESH_WINDOW_MS)
                                < now);
                    if should_refresh {
                        drop(self.refresh(&mut state));
                    }
                    return Ok(current);
                }
                _ => self.refresh(&mut state),
            }
        };
        worker.await
    }

    /// `refresh` (`tokenCycler.js:100-131`): starts one refresh, or joins the
    /// one running. The work is spawned so that a refresh nobody awaits still
    /// runs, as a promise does.
    fn refresh(&self, state: &mut CyclerState) -> RefreshWorker {
        if let Some(worker) = &state.refresh_worker {
            return worker.clone();
        }
        let credential = Arc::clone(&self.credential);
        let scopes = Arc::clone(&self.scopes);
        let shared_state = Arc::clone(&self.state);
        // "If we don't have a token, then we should timeout immediately."
        let refresh_timeout = state
            .token
            .as_ref()
            .map_or_else(now_ms, |token| token.expires_on_timestamp);
        let worker = async move {
            let result = begin_refresh(credential.as_ref(), &scopes, refresh_timeout).await;
            let mut state = shared_state.lock().unwrap();
            state.refresh_worker = None;
            // A failure resets the token, so the next call starts over.
            state.token = result.as_ref().ok().cloned();
            result
        }
        .boxed()
        .shared();
        state.refresh_worker = Some(worker.clone());
        tokio::spawn(worker.clone());
        worker
    }
}

/// `beginRefresh` (`tokenCycler.js:15-41`): before `refresh_timeout` a failure
/// or `null` is retried every 3 s; at or after it, one last attempt decides.
async fn begin_refresh(
    credential: &dyn TokenCredential,
    scopes: &[String],
    refresh_timeout: u64,
) -> Result<AccessToken, CredentialError> {
    loop {
        if now_ms() < refresh_timeout {
            if let Ok(Some(token)) = credential.get_token(scopes).await {
                return Ok(token);
            }
        } else {
            return credential.get_token(scopes).await?.ok_or_else(|| {
                CredentialError::Other("Failed to refresh access token.".to_owned())
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(RETRY_INTERVAL_MS)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Scripted {
        calls: AtomicUsize,
        results: Mutex<Vec<Result<Option<AccessToken>, CredentialError>>>,
    }

    impl TokenCredential for Scripted {
        fn get_token<'a>(
            &'a self,
            _scopes: &'a [String],
        ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let next = self.results.lock().unwrap().remove(0);
            Box::pin(async move { next })
        }
    }

    fn token(value: &str, expires_in_ms: u64) -> AccessToken {
        AccessToken {
            token: value.to_owned(),
            expires_on_timestamp: now_ms() + expires_in_ms,
            refresh_after_timestamp: None,
        }
    }

    fn scripted_provider(
        results: Vec<Result<Option<AccessToken>, CredentialError>>,
    ) -> (BearerTokenProvider, Arc<Scripted>) {
        let credential = Arc::new(Scripted {
            calls: AtomicUsize::new(0),
            results: Mutex::new(results),
        });
        let provider = get_bearer_token_provider(
            Arc::clone(&credential) as Arc<dyn TokenCredential>,
            "https://cognitiveservices.azure.com/.default",
        );
        (provider, credential)
    }

    /// `tokenCycler.js`: a fresh token is reused, and a first failure throws
    /// at once instead of being retried.
    #[tokio::test]
    async fn cycler_reuses_a_fresh_token_and_fails_a_first_attempt_at_once() {
        let (provider, credential) =
            scripted_provider(vec![Ok(Some(token("first", 60 * 60 * 1000)))]);
        assert_eq!(provider.get_token().await.unwrap(), "first");
        assert_eq!(provider.get_token().await.unwrap(), "first");
        assert_eq!(credential.calls.load(Ordering::SeqCst), 1);

        let (provider, credential) = scripted_provider(vec![Err(CredentialError::Unavailable(
            "nothing".to_owned(),
        ))]);
        assert_eq!(
            provider.get_token().await.unwrap_err(),
            CredentialError::Unavailable("nothing".to_owned())
        );
        assert_eq!(credential.calls.load(Ordering::SeqCst), 1);
    }

    /// Inside the 2-minute window the old token is returned while a refresh
    /// runs; an empty token is TS's "Failed to get access token".
    #[tokio::test]
    async fn cycler_refreshes_in_the_background_inside_the_window() {
        let (provider, credential) = scripted_provider(vec![
            Ok(Some(token("old", 60 * 1000))),
            Ok(Some(token("new", 60 * 60 * 1000))),
        ]);
        assert_eq!(provider.get_token().await.unwrap(), "old");
        assert_eq!(provider.get_token().await.unwrap(), "old");
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(provider.get_token().await.unwrap(), "new");
        assert_eq!(credential.calls.load(Ordering::SeqCst), 2);

        let (provider, _) = scripted_provider(vec![Ok(Some(token("", 60 * 60 * 1000)))]);
        assert_eq!(
            provider.get_token().await.unwrap_err().to_string(),
            "Failed to get access token"
        );
    }
}
