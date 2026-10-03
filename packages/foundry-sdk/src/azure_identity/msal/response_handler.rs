//! Maps to: `@azure/msal-common` 16.4.0 `src/response/ResponseHandler.ts`:
//! `validateTokenResponse`, and the access-token part of
//! `handleServerTokenResponse` (`generateCacheRecord`,
//! `generateAuthenticationResult`), which both flows run their token
//! responses through.

use serde_json::Value;

use super::access_token_entity::AccessTokenEntity;
use super::auth_error::MsalError;
use super::authentication_result::AuthenticationResult;
use super::constants::NOT_AVAILABLE;
use super::interaction_required_auth_error::is_interaction_required_error;
use super::scope_set::{scope_set, scope_set_from_string};
use super::server_authorization_token_response::ServerAuthorizationTokenResponse;
use super::time_utils::to_date_from_seconds;
use crate::azure_identity::js::{js_string, js_truthy, parse_int};

/// Maps to: `ResponseHandler.ts:94-180` `validateTokenResponse`. While
/// refreshing proactively, an error with a 4xx or 5xx status is only logged,
/// and the cached token stands.
pub(crate) fn validate_token_response(
    server_response: &ServerAuthorizationTokenResponse,
    refresh_access_token: bool,
) -> Result<(), MsalError> {
    let is_truthy = |value: &Option<Value>| value.as_ref().is_some_and(js_truthy);
    if !(is_truthy(&server_response.error)
        || is_truthy(&server_response.error_description)
        || is_truthy(&server_response.suberror))
    {
        return Ok(());
    }
    let or_not_available = |value: &Option<Value>| {
        value
            .as_ref()
            .filter(|value| js_truthy(value))
            .map_or_else(|| NOT_AVAILABLE.to_owned(), js_string)
    };
    let err_string = format!(
        "Error(s): {} - Timestamp: {} - Description: {} - Correlation ID: {} - Trace ID: {}",
        or_not_available(&server_response.error_codes),
        or_not_available(&server_response.timestamp),
        or_not_available(&server_response.error_description),
        or_not_available(&server_response.correlation_id),
        or_not_available(&server_response.trace_id),
    );
    if refresh_access_token && (400..=599).contains(&server_response.status) {
        return Ok(());
    }
    if is_interaction_required_error(
        server_response.error.as_ref(),
        server_response.error_description.as_ref(),
        server_response.suberror.as_ref(),
    )? {
        // `new InteractionRequiredAuthError(error, error_description, …)`.
        return Err(MsalError::server(
            server_response.error.as_ref(),
            server_response.error_description.as_ref(),
        ));
    }
    // `new ServerError(error, errString, …)`.
    Err(MsalError::server(
        server_response.error.as_ref(),
        Some(&Value::String(err_string)),
    ))
}

/// Maps to: `ResponseHandler.ts:187-351` `handleServerTokenResponse`, for a
/// response without an id token or refresh token: the access-token part of
/// `generateCacheRecord` (`:405-458`), saved through the handler's cache
/// storage (`saveCacheRecord`, `:318-324`), and `generateAuthenticationResult`
/// (`:520-631`). Times count from `req_timestamp`, taken before the request.
/// `client_id` is the handler's (the application's, or the managed
/// identity's id) and `save_access_token` its storage's `saveAccessToken`.
pub(crate) fn handle_server_token_response(
    server_token_response: &ServerAuthorizationTokenResponse,
    client_id: &str,
    req_timestamp: f64,
    request_scopes: &[String],
    save_access_token: impl FnOnce(AccessTokenEntity),
) -> Result<AuthenticationResult, MsalError> {
    let Some(access_token) = server_token_response
        .access_token
        .as_ref()
        .filter(|value| js_truthy(value))
    else {
        // A cache record without an access token (`:532-534`).
        return Ok(AuthenticationResult {
            access_token: String::new(),
            expires_on: None,
            refresh_on: None,
        });
    };
    // `ScopeSet.fromString(scope)`, which splits it (only a string can be
    // split), else the request's scopes.
    let response_scopes = match server_token_response
        .scope
        .as_ref()
        .filter(|value| js_truthy(value))
    {
        Some(Value::String(scope)) => scope_set_from_string(scope)?,
        Some(_) => {
            return Err(MsalError::other(
                "scopeString.split is not a function".to_owned(),
            ));
        }
        None => scope_set(request_scopes)?,
    };
    // `expires_in || 0` and `refresh_in || undefined`: 0 and `NaN` drop.
    let expires_in = server_token_response
        .expires_in
        .filter(|seconds| *seconds != 0.0 && !seconds.is_nan())
        .unwrap_or(0.0);
    let refresh_in = server_token_response
        .refresh_in
        .filter(|seconds| *seconds != 0.0 && !seconds.is_nan());
    let cached_access_token = AccessTokenEntity {
        client_id: client_id.to_owned(),
        target: response_scopes.join(" "),
        secret: js_string(access_token),
        expires_on: req_timestamp + expires_in,
        refresh_on: refresh_in
            .filter(|seconds| *seconds > 0.0)
            .map(|seconds| req_timestamp + seconds),
    };
    // `CacheHelpers.ts:103-108`: any other token type is stored as
    // `AccessToken_With_AuthScheme`, which the client-credential cache
    // lookup never matches.
    let token_type = server_token_response
        .token_type
        .as_ref()
        .filter(|value| js_truthy(value))
        .map(js_string);
    if token_type.is_none_or(|token_type| token_type.to_lowercase() == "bearer") {
        save_access_token(cached_access_token.clone());
    }
    Ok(generate_authentication_result(&cached_access_token))
}

/// Maps to: `ResponseHandler.ts:520-631` `generateAuthenticationResult`, for
/// an access token (`:539-582`): the times in seconds become dates,
/// `refreshOn` only when set.
pub(crate) fn generate_authentication_result(
    access_token: &AccessTokenEntity,
) -> AuthenticationResult {
    AuthenticationResult {
        access_token: access_token.secret.clone(),
        expires_on: Some(to_date_from_seconds(access_token.expires_on)),
        refresh_on: access_token.refresh_on.map(to_date_from_seconds),
    }
}

/// `typeof v === "string" ? parseInt(v, 10) : v` (`ResponseHandler.ts:417-428`)
/// for a JSON value, as the number the arithmetic after it sees: `NaN` for a
/// value that is no number (a boolean counts as 0 or 1).
pub(crate) fn js_seconds(value: &Value) -> f64 {
    match value {
        Value::String(text) => parse_int(text),
        Value::Number(number) => number.as_f64().unwrap_or(f64::NAN),
        Value::Bool(flag) => f64::from(u8::from(*flag)),
        _ => f64::NAN,
    }
}
