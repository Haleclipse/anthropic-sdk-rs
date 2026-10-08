//! Port of `@aws-sdk/credential-provider-env` 3.972.x `fromEnv`.

use super::js::date_from_string;
use super::{Credentials, CredentialsProviderError, Environment};

pub(crate) const ENV_KEY: &str = "AWS_ACCESS_KEY_ID";
pub(crate) const ENV_SECRET: &str = "AWS_SECRET_ACCESS_KEY";
const ENV_SESSION: &str = "AWS_SESSION_TOKEN";
const ENV_EXPIRATION: &str = "AWS_CREDENTIAL_EXPIRATION";

/// `fromEnv`: the key pair, with the session token and expiry when set.
pub(crate) fn from_env(env: &Environment) -> Result<Credentials, CredentialsProviderError> {
    match (env.truthy(ENV_KEY), env.truthy(ENV_SECRET)) {
        (Some(access_key_id), Some(secret_access_key)) => Ok(Credentials {
            access_key_id: access_key_id.to_owned(),
            secret_access_key: secret_access_key.to_owned(),
            session_token: env.truthy(ENV_SESSION).map(str::to_owned),
            expiration: env.truthy(ENV_EXPIRATION).and_then(date_from_string),
        }),
        _ => Err(CredentialsProviderError::new(
            "Unable to find environment variable credentials.",
        )),
    }
}
