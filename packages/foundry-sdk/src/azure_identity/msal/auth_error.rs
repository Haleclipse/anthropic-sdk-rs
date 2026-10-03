//! Maps to: `@azure/msal-common` 16.4.0 `src/error/AuthError.ts`, the base of
//! the errors MSAL throws, with the constructors of the subclasses the two
//! flows raise (`ClientAuthError.ts`, `ClientConfigurationError.ts`,
//! `ServerError.ts`, `InteractionRequiredAuthError.ts`): each only names
//! itself and hands its code and message to `AuthError`.

use serde_json::Value;

use crate::azure_identity::js::{js_string, js_truthy, template_string};

/// `:10-12` `getDefaultErrorMessage`.
pub(crate) fn get_default_error_message(code: &str) -> String {
    format!("See https://aka.ms/msal.js.errors#{code} for details")
}

/// The message of `new AuthError(errorCode, errorMessage)` (`:43-55`):
/// `${errorCode}: ${message}` with `message = errorMessage ||
/// getDefaultErrorMessage(errorCode)`, or `errorCode` alone when both are
/// falsy. A missing field renders as `undefined`, as in a template literal.
pub(crate) fn auth_error_message(
    error_code: Option<&Value>,
    error_message: Option<&Value>,
) -> String {
    let message = match error_message.filter(|message| js_truthy(message)) {
        Some(message) => Some(js_string(message)),
        None => error_code
            .filter(|code| js_truthy(code))
            .map(|code| get_default_error_message(&js_string(code))),
    };
    match message {
        Some(message) => format!("{}: {message}", template_string(error_code)),
        // `super(errorCode)`: `new Error(undefined)` has an empty message.
        None => error_code.map_or_else(String::new, js_string),
    }
}

/// What an MSAL call throws, by the parts the identity layer reads: the
/// class (`name`), `errorCode` when it is a string, and `message`.
#[derive(Debug)]
pub(crate) struct MsalError {
    pub kind: MsalErrorKind,
    pub error_code: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MsalErrorKind {
    /// `ClientAuthError`.
    ClientAuth,
    /// `ClientConfigurationError`.
    ClientConfiguration,
    /// `ServerError` and `InteractionRequiredAuthError`.
    Server,
    /// Anything that is no `AuthError`: the `TypeError`s a malformed response
    /// raises, and the identity layer's own `AuthenticationRequiredError`.
    Other,
}

impl MsalError {
    /// Maps to: `ClientAuthError.ts:26-31` `createClientAuthError(code)`.
    pub(crate) fn client_auth(code: &str) -> Self {
        Self {
            kind: MsalErrorKind::ClientAuth,
            error_code: Some(code.to_owned()),
            message: format!("{code}: {}", get_default_error_message(code)),
        }
    }

    /// Maps to: `ClientConfigurationError.ts:21-25`
    /// `createClientConfigurationError(code)`.
    pub(crate) fn client_configuration(code: &str) -> Self {
        Self {
            kind: MsalErrorKind::ClientConfiguration,
            error_code: Some(code.to_owned()),
            message: format!("{code}: {}", get_default_error_message(code)),
        }
    }

    /// Maps to: `new ServerError(errorCode, errorMessage, …)`
    /// (`ServerError.ts:22-35`) and `new InteractionRequiredAuthError(errorCode,
    /// errorMessage, …)` (`InteractionRequiredAuthError.ts:63-82`), which pass
    /// both to `AuthError`.
    pub(crate) fn server(error_code: Option<&Value>, error_message: Option<&Value>) -> Self {
        Self {
            kind: MsalErrorKind::Server,
            error_code: match error_code {
                Some(Value::String(code)) => Some(code.clone()),
                _ => None,
            },
            message: auth_error_message(error_code, error_message),
        }
    }

    /// An error that is no `AuthError`, by its message.
    pub(crate) fn other(message: String) -> Self {
        Self {
            kind: MsalErrorKind::Other,
            error_code: None,
            message,
        }
    }
}
