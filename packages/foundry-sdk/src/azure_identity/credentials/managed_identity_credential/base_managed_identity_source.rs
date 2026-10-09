//! Maps to: `@azure/msal-node`
//! `src/client/ManagedIdentitySources/BaseManagedIdentitySource.ts`. The
//! response goes through `@azure/msal-common` 16.4.0's
//! `ResponseHandler.validateTokenResponse` and `handleServerTokenResponse`,
//! shared with the client-credential flow in
//! [`crate::azure_identity::msal`].
//!
//! Response fields are read with JS semantics (truthiness, `String()`,
//! `Number()`), since the body is whatever JSON the endpoint sent. Where a
//! malformed body makes MSAL throw a `TypeError`, the message is V8's (Node):
//! Bun's quotes the minified expression and cannot be reproduced.

use serde_json::Value;

use super::managed_identity_application::{ManagedIdentityId, save_access_token};
use super::{IdentityClient, MsiError};
use crate::azure_identity::js::{js_string, js_truthy, string_to_number};
use crate::azure_identity::msal::auth_error::MsalError;
use crate::azure_identity::msal::authentication_result::AuthenticationResult;
use crate::azure_identity::msal::constants::URL_FORM_CONTENT_TYPE;
use crate::azure_identity::msal::network_response::NetworkResponse;
use crate::azure_identity::msal::response_handler::{
    handle_server_token_response, validate_token_response,
};
use crate::azure_identity::msal::server_authorization_token_response::ServerAuthorizationTokenResponse;
use crate::azure_identity::msal::time_utils::now_seconds;
use crate::azure_identity::msal::url_string::{append_query_string, url_string};
use crate::azure_identity::msal::url_utils::map_to_query_string;

/// Maps to: the abstract `BaseManagedIdentitySource`: what a source adds is
/// its request.
pub(super) trait BaseManagedIdentitySource: Send + Sync {
    /// `:103-106` `createRequest`.
    fn create_request(
        &self,
        resource: &str,
        managed_identity_id: &ManagedIdentityId,
    ) -> ManagedIdentityRequestParameters;
}

/// Maps to: `src/request/ManagedIdentityRequest.ts`, for the field read here.
pub(super) struct ManagedIdentityRequest {
    pub resource: String,
}

/// Maps to: `src/config/ManagedIdentityRequestParameters.ts`, for the GET
/// requests of the ported sources: no body parameters, and MSAL's own retry
/// policy unused.
pub(super) struct ManagedIdentityRequestParameters {
    base_endpoint: String,
    pub headers: Vec<(&'static str, String)>,
    pub query_parameters: Vec<(&'static str, String)>,
}

impl ManagedIdentityRequestParameters {
    pub(super) fn new(endpoint: String) -> Self {
        Self {
            base_endpoint: endpoint,
            headers: Vec::new(),
            query_parameters: Vec::new(),
        }
    }

    /// Maps to: `:37-53` `computeUri`: the non-empty parameters in order
    /// (`RequestParameterBuilder.ts:468-477`), each value through
    /// `encodeURIComponent` (`UrlUtils.ts:94-102`), appended with `?` or `&`
    /// (`UrlString.ts:99-107`).
    pub(super) fn compute_uri(&self) -> String {
        let parameters = self
            .query_parameters
            .iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        append_query_string(&self.base_endpoint, &map_to_query_string(&parameters))
    }
}

/// Maps to: `:205-324` `acquireTokenWithManagedIdentity`, without the
/// revoked-token hash and client capabilities CC never sets. Internal retries
/// are disabled (`index.js:79`), so the identity client sends the request
/// itself. Whatever it throws is not an `AuthError`, so it becomes
/// `network_error` (`:284-290`).
pub(super) async fn acquire_token_with_managed_identity(
    source: &dyn BaseManagedIdentitySource,
    managed_identity_request: &ManagedIdentityRequest,
    managed_identity_id: &ManagedIdentityId,
    refresh_access_token: bool,
    network_client: &IdentityClient<'_>,
) -> Result<AuthenticationResult, MsiError> {
    let mut network_request =
        source.create_request(&managed_identity_request.resource, managed_identity_id);
    network_request
        .headers
        .push(("Content-Type", URL_FORM_CONTENT_TYPE.to_owned()));
    let req_timestamp = now_seconds();
    let response = network_client
        .send_get_request_async(network_request.compute_uri(), network_request.headers)
        .await
        .map_err(|_| MsalError::client_auth("network_error"))?;
    let server_token_response = get_server_token_response(&response)?;
    validate_token_response(&server_token_response, refresh_access_token)?;
    // `:292-323`: the handler's client id is the identity's id, its storage
    // the static `nodeStorage`; the request's scopes are `[resource]`
    // (`ManagedIdentityApplication.ts:149-151`).
    handle_server_token_response(
        &server_token_response,
        &managed_identity_id.id,
        req_timestamp,
        std::slice::from_ref(&managed_identity_request.resource),
        save_access_token,
    )
    .map_err(MsiError::from)
}

/// Maps to: `:141-188` `getServerTokenResponse`. `expires_on` (seconds, or an
/// exact ISO 8601 string) becomes `expires_in` from now; past two hours the
/// token is refreshed halfway. A body that is not an object reads as all
/// fields missing, except `undefined` and `null`, whose first read throws.
fn get_server_token_response(
    response: &NetworkResponse<Option<Value>>,
) -> Result<ServerAuthorizationTokenResponse, MsiError> {
    let body = match &response.body {
        None => return Err(cannot_read_expires_on("undefined")),
        Some(Value::Null) => return Err(cannot_read_expires_on("null")),
        Some(body) => body,
    };
    let field = |key: &str| body.get(key).cloned();
    let (mut expires_in, mut refresh_in) = (None, None);
    if let Some(expires_on) = field("expires_on").filter(js_truthy) {
        let expires_on = match &expires_on {
            Value::String(text) => iso8601_milliseconds(text).map_or_else(
                || string_to_number(text),
                |milliseconds| milliseconds / 1000.0,
            ),
            value => js_to_number(value),
        };
        let seconds = expires_on - now_seconds();
        expires_in = Some(seconds);
        if seconds > 2.0 * 3600.0 {
            refresh_in = Some(seconds / 2.0);
        }
    }
    // `typeof error === "string" ? … : error?.…`.
    let error = field("error");
    let error_is_string = matches!(error, Some(Value::String(_)));
    let error_member = |key: &str| match &error {
        Some(Value::Object(members)) => members.get(key).cloned(),
        _ => None,
    };
    Ok(ServerAuthorizationTokenResponse {
        status: response.status,
        access_token: field("access_token"),
        expires_in,
        scope: field("resource"),
        token_type: field("token_type"),
        refresh_in,
        correlation_id: js_or(field("correlation_id"), field("correlationId")),
        error: if error_is_string {
            error.clone()
        } else {
            error_member("code")
        },
        error_description: js_or(
            field("message"),
            if error_is_string {
                field("error_description")
            } else {
                error_member("message")
            },
        ),
        error_codes: field("error_codes"),
        suberror: None,
        timestamp: field("timestamp"),
        trace_id: field("trace_id"),
    })
}

/// The `TypeError` of reading `expires_on` off `undefined` or `null`.
fn cannot_read_expires_on(value: &str) -> MsiError {
    MsiError::Other(format!(
        "Cannot read properties of {value} (reading 'expires_on')"
    ))
}

/// Maps to: `:339-377` `getManagedIdentityUserAssignedIdQueryParameterKey`,
/// for a client id on the 2019+ API versions both ported sources use.
pub(super) fn get_managed_identity_user_assigned_id_query_parameter_key() -> &'static str {
    "client_id"
}

/// Maps to: `:393-414` `getValidatedEnvVariableUrlString`, for a non-empty
/// value, which `new UrlString` cannot reject: its canonical form
/// (`UrlString.ts:24-56`), lowercase and ending in `/`, unless it has a `#`.
pub(super) fn get_validated_env_variable_url_string(env_variable: &str) -> String {
    url_string(env_variable)
}

/// msal-node `src/utils/TimeUtils.ts:13-20` `isIso8601`, as the date it
/// parses to: only the exact `toISOString()` form counts.
fn iso8601_milliseconds(text: &str) -> Option<f64> {
    const ISO_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";
    let date = chrono::NaiveDateTime::parse_from_str(text, ISO_FORMAT).ok()?;
    (date.format(ISO_FORMAT).to_string() == text).then(|| date.and_utc().timestamp_millis() as f64)
}

fn is_truthy(value: &Option<Value>) -> bool {
    value.as_ref().is_some_and(js_truthy)
}

/// `a || b`.
fn js_or(left: Option<Value>, right: Option<Value>) -> Option<Value> {
    if is_truthy(&left) { left } else { right }
}

/// `Number(value)` of a JSON value.
fn js_to_number(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(value) => f64::from(u8::from(*value)),
        Value::Number(number) => number.as_f64().unwrap_or(f64::NAN),
        Value::String(text) => string_to_number(text),
        Value::Array(_) => string_to_number(&js_string(value)),
        Value::Object(_) => f64::NAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn response(body: Value) -> NetworkResponse<Option<Value>> {
        NetworkResponse {
            status: 200,
            body: Some(body),
        }
    }

    /// `BaseManagedIdentitySource.ts:144-158` and msal-node `isIso8601`.
    #[test]
    fn expiry_reads_seconds_and_exact_iso_strings_and_refreshes_long_tokens_halfway() {
        // The clock may tick between the two reads of it.
        let about = |value: Option<f64>, expected: f64| {
            value.is_some_and(|value| (expected - 1.0..=expected).contains(&value))
        };
        let now = now_seconds();
        let parsed = get_server_token_response(&response(
            json!({ "expires_on": format!("{}", now + 3600.0) }),
        ))
        .unwrap();
        assert!(about(parsed.expires_in, 3600.0), "{:?}", parsed.expires_in);
        assert_eq!(parsed.refresh_in, None);

        let parsed =
            get_server_token_response(&response(json!({ "expires_on": now + 3.0 * 3600.0 })))
                .unwrap();
        assert!(
            about(parsed.refresh_in, 1.5 * 3600.0),
            "{:?}",
            parsed.refresh_in
        );

        let iso = chrono::DateTime::from_timestamp((now + 3600.0) as i64, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let parsed = get_server_token_response(&response(json!({ "expires_on": iso }))).unwrap();
        assert!(about(parsed.expires_in, 3600.0), "{:?}", parsed.expires_in);
        // Not the exact `toISOString()` form: `Number("…")` is NaN.
        let parsed =
            get_server_token_response(&response(json!({ "expires_on": "2030-01-01T00:00:00Z" })))
                .unwrap();
        assert!(parsed.expires_in.unwrap().is_nan());

        let Err(MsiError::Other(message)) = get_server_token_response(&NetworkResponse {
            status: 404,
            body: None,
        }) else {
            panic!("an undefined body throws");
        };
        assert_eq!(
            message,
            "Cannot read properties of undefined (reading 'expires_on')"
        );
    }

    /// `ResponseHandler.ts:100-179` and `AuthError.ts:43-55`.
    #[test]
    fn error_responses_become_server_and_interaction_required_errors() {
        let parsed = get_server_token_response(&NetworkResponse {
            status: 400,
            body: Some(
                json!({ "error": { "code": "bad_request", "message": "no such identity" } }),
            ),
        })
        .unwrap();
        let Err(MsiError::Msal {
            error_code,
            message,
        }) = validate_token_response(&parsed, false).map_err(MsiError::from)
        else {
            panic!("an error response throws");
        };
        assert_eq!(error_code.as_deref(), Some("bad_request"));
        assert_eq!(
            message,
            "bad_request: Error(s): Not Available - Timestamp: Not Available - Description: no such identity - Correlation ID: Not Available - Trace ID: Not Available"
        );
        // On a refresh a 4xx is only logged.
        assert!(validate_token_response(&parsed, true).is_ok());

        let parsed = get_server_token_response(&response(
            json!({ "error": "login_required", "error_description": "sign in" }),
        ))
        .unwrap();
        let Err(MsiError::Msal { message, .. }) =
            validate_token_response(&parsed, false).map_err(MsiError::from)
        else {
            panic!("an interaction-required response throws");
        };
        assert_eq!(message, "login_required: sign in");

        let parsed =
            get_server_token_response(&response(json!({ "message": "only a message" }))).unwrap();
        let Err(MsiError::Msal {
            error_code,
            message,
        }) = validate_token_response(&parsed, false).map_err(MsiError::from)
        else {
            panic!("a description alone throws");
        };
        assert_eq!(error_code, None);
        assert!(message.starts_with("undefined: Error(s): "), "{message}");

        assert_eq!(
            match MsiError::from(MsalError::server(Some(&json!("network_error")), None)) {
                MsiError::Msal { message, .. } => message,
                other => panic!("{other:?}"),
            },
            "network_error: See https://aka.ms/msal.js.errors#network_error for details"
        );
    }

    /// `ManagedIdentityRequestParameters.computeUri` and `UrlString`.
    #[test]
    fn request_uri_encodes_values_and_canonical_urls_end_in_a_slash() {
        let mut request = ManagedIdentityRequestParameters::new("http://host/path".to_owned());
        request
            .query_parameters
            .push(("api-version", "2019-08-01".to_owned()));
        request
            .query_parameters
            .push(("resource", "https://cognitiveservices.azure.com".to_owned()));
        request.query_parameters.push(("client_id", String::new()));
        assert_eq!(
            request.compute_uri(),
            "http://host/path?api-version=2019-08-01&resource=https%3A%2F%2Fcognitiveservices.azure.com"
        );
        assert_eq!(
            get_validated_env_variable_url_string("HTTP://Pod:8080/metadata/identity/oauth2/token"),
            "http://pod:8080/metadata/identity/oauth2/token/"
        );
        assert_eq!(
            get_validated_env_variable_url_string("http://a/b?"),
            "http://a/b/"
        );
        assert_eq!(
            get_validated_env_variable_url_string("http://A/#x"),
            "http://A/#x"
        );
    }
}
