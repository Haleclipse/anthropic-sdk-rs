//! Maps to: `@azure/identity` `credentials/chainedTokenCredential.js`.

use futures::future::BoxFuture;

use crate::azure_identity::{AccessToken, CredentialError, TokenCredential};

/// Maps to: `ChainedTokenCredential`.
pub struct ChainedTokenCredential {
    sources: Vec<Box<dyn TokenCredential>>,
}

impl ChainedTokenCredential {
    pub fn new(sources: Vec<Box<dyn TokenCredential>>) -> Self {
        Self { sources }
    }
}

impl TokenCredential for ChainedTokenCredential {
    /// `getTokenInternal` (`:58-90`): the first token wins. An unavailable or
    /// authentication-required error is recorded and the next credential
    /// tried; any other error is thrown as it is. Nothing is remembered
    /// between calls, so every call starts from the first credential.
    fn get_token<'a>(
        &'a self,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        Box::pin(async move {
            let mut errors = Vec::new();
            for source in &self.sources {
                match source.get_token(scopes).await {
                    Ok(Some(token)) => return Ok(Some(token)),
                    Ok(None) => {}
                    Err(error) if error.continues_chain() => errors.push(error),
                    Err(error) => return Err(error),
                }
            }
            if !errors.is_empty() {
                return Err(CredentialError::Aggregate {
                    message: "ChainedTokenCredential authentication failed.".to_owned(),
                    errors,
                });
            }
            Err(CredentialError::Unavailable(
                "Failed to retrieve a valid token".to_owned(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Result<Option<AccessToken>, CredentialError>);

    impl TokenCredential for Fixed {
        fn get_token<'a>(
            &'a self,
            _scopes: &'a [String],
        ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
            let result = self.0.clone();
            Box::pin(async move { result })
        }
    }

    fn chain(results: Vec<Result<Option<AccessToken>, CredentialError>>) -> ChainedTokenCredential {
        ChainedTokenCredential::new(
            results
                .into_iter()
                .map(|result| Box::new(Fixed(result)) as Box<dyn TokenCredential>)
                .collect(),
        )
    }

    fn token() -> AccessToken {
        AccessToken {
            token: "t".to_owned(),
            expires_on_timestamp: 1,
            refresh_after_timestamp: None,
        }
    }

    /// `chainedTokenCredential.js:58-90` and `errors.js:97-99`.
    #[tokio::test]
    async fn chain_matches_official_continue_halt_and_aggregate_rules() {
        let scopes = ["s".to_owned()];
        let unavailable = || {
            Err(CredentialError::Unavailable(
                "first is unavailable".to_owned(),
            ))
        };
        assert_eq!(
            chain(vec![unavailable(), Ok(None), Ok(Some(token()))])
                .get_token(&scopes)
                .await
                .unwrap(),
            Some(token())
        );
        let halted = chain(vec![
            Err(CredentialError::Other("broken".to_owned())),
            Ok(Some(token())),
        ]);
        assert_eq!(
            halted.get_token(&scopes).await.unwrap_err(),
            CredentialError::Other("broken".to_owned())
        );
        let aggregate = chain(vec![
            unavailable(),
            Err(CredentialError::AuthenticationRequired(
                "sign in".to_owned(),
            )),
        ])
        .get_token(&scopes)
        .await
        .unwrap_err();
        assert_eq!(
            aggregate.to_string(),
            "ChainedTokenCredential authentication failed.\n\
             CredentialUnavailableError: first is unavailable\n\
             AuthenticationRequiredError: sign in"
        );
        assert_eq!(
            chain(vec![Ok(None)]).get_token(&scopes).await.unwrap_err(),
            CredentialError::Unavailable("Failed to retrieve a valid token".to_owned())
        );
    }
}
