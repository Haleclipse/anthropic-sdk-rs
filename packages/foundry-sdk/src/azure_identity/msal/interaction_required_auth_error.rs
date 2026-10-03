//! Maps to: `@azure/msal-common` 16.4.0
//! `src/error/InteractionRequiredAuthError.ts`, for the check
//! `ResponseHandler.validateTokenResponse` runs. The error itself is built by
//! [`MsalError::server`].

use serde_json::Value;

use super::auth_error::MsalError;
use crate::azure_identity::js::js_truthy;

/// `:13-20` `InteractionRequiredServerErrorMessage`.
const INTERACTION_REQUIRED_SERVER_ERROR_MESSAGE: [&str; 6] = [
    "interaction_required",
    "consent_required",
    "login_required",
    "bad_token",
    "ux_not_allowed",
    "interrupted_user",
];

/// `:22-31` `InteractionRequiredAuthSubErrorMessage`.
const INTERACTION_REQUIRED_AUTH_SUB_ERROR_MESSAGE: [&str; 8] = [
    "message_only",
    "additional_action",
    "basic_action",
    "user_password_expired",
    "consent_required",
    "bad_token",
    "ux_not_allowed",
    "interrupted_user",
];

/// Maps to: `:91-113` `isInteractionRequiredError`. The code and sub-error
/// are looked up with `indexOf` (`===`), so only strings match. All three
/// checks run, so a description without `indexOf` throws whatever the code
/// says; an array description matches by element.
pub(crate) fn is_interaction_required_error(
    error_code: Option<&Value>,
    error_string: Option<&Value>,
    sub_error: Option<&Value>,
) -> Result<bool, MsalError> {
    let is_interaction_required_error_code = matches!(
        error_code,
        Some(Value::String(code)) if INTERACTION_REQUIRED_SERVER_ERROR_MESSAGE.contains(&code.as_str())
    );
    let is_interaction_required_sub_error = matches!(
        sub_error,
        Some(Value::String(sub)) if INTERACTION_REQUIRED_AUTH_SUB_ERROR_MESSAGE.contains(&sub.as_str())
    );
    let is_interaction_required_error_desc = match error_string.filter(|value| js_truthy(value)) {
        None => false,
        Some(Value::String(text)) => INTERACTION_REQUIRED_SERVER_ERROR_MESSAGE
            .iter()
            .any(|code| text.contains(code)),
        // `Array.prototype.indexOf`: an element equal to a code.
        Some(Value::Array(items)) => items.iter().any(|item| {
            matches!(item, Value::String(item) if INTERACTION_REQUIRED_SERVER_ERROR_MESSAGE.contains(&item.as_str()))
        }),
        Some(_) => {
            return Err(MsalError::other("errorString.indexOf is not a function".to_owned()));
        }
    };
    Ok(is_interaction_required_error_code
        || is_interaction_required_error_desc
        || is_interaction_required_sub_error)
}
