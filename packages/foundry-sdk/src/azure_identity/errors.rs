//! Maps to: `@azure/identity` `dist/esm/errors.js`, the error classes the
//! chain tells apart by `err.name` (`chainedTokenCredential.js:69-76`).

use crate::azure_identity::js::template_string;

/// The errors a credential fails with. [`std::fmt::Display`] is the JS
/// `err.message`, which is what the Foundry client puts after its prefix;
/// [`CredentialError::to_js_string`] is `Error.prototype.toString`, which is
/// how the aggregate lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialError {
    /// `CredentialUnavailableError`: the chain records it and tries the next
    /// credential.
    Unavailable(String),
    /// `AuthenticationRequiredError`: recorded like an unavailable one.
    AuthenticationRequired(String),
    /// `AuthenticationError(statusCode, errorBody)`: halts the chain.
    Authentication {
        status: u16,
        error: String,
        error_description: String,
    },
    /// `AggregateAuthenticationError(errors, errorMessage)`.
    Aggregate {
        message: String,
        errors: Vec<CredentialError>,
    },
    /// Any other `Error`: halts the chain.
    Other(String),
}

/// The body `AuthenticationError`'s constructor receives.
pub enum AuthenticationErrorBody {
    /// An object with string `error` and `error_description` (`isErrorResponse`).
    Response {
        error: String,
        error_description: String,
    },
    /// A response body, which is tried as JSON first.
    Text(String),
    /// Anything else.
    Missing,
}

impl CredentialError {
    /// Maps to: `errors.js:41-79` `new AuthenticationError(statusCode, errorBody)`.
    pub fn authentication(status: u16, body: AuthenticationErrorBody) -> Self {
        let unknown = || {
            (
                "unknown_error".to_owned(),
                "An unknown error occurred and no additional details are available.".to_owned(),
            )
        };
        let (error, error_description) = match body {
            AuthenticationErrorBody::Response {
                error,
                error_description,
            } => (error, error_description),
            AuthenticationErrorBody::Text(text) => {
                match serde_json::from_str::<serde_json::Value>(&text) {
                    // `convertOAuthErrorResponseToErrorResponse` copies the two
                    // fields whatever they hold; a missing one is `undefined`.
                    Ok(parsed) => (
                        template_string(parsed.get("error")),
                        template_string(parsed.get("error_description")),
                    ),
                    Err(_) if status == 400 => (
                        "invalid_request".to_owned(),
                        format!("The service indicated that the request was invalid.\n\n{text}"),
                    ),
                    Err(_) => (
                        "unknown_error".to_owned(),
                        format!("An unknown error has occurred. Response body:\n\n{text}"),
                    ),
                }
            }
            AuthenticationErrorBody::Missing => unknown(),
        };
        Self::Authentication {
            status,
            error,
            error_description,
        }
    }

    /// The JS `err.name`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "CredentialUnavailableError",
            Self::AuthenticationRequired(_) => "AuthenticationRequiredError",
            Self::Authentication { .. } => "AuthenticationError",
            Self::Aggregate { .. } => "AggregateAuthenticationError",
            Self::Other(_) => "Error",
        }
    }

    /// `Error.prototype.toString`: `${name}: ${message}`, or the bare name
    /// for an empty message.
    pub fn to_js_string(&self) -> String {
        let message = self.to_string();
        if message.is_empty() {
            self.name().to_owned()
        } else {
            format!("{}: {message}", self.name())
        }
    }

    /// Whether `ChainedTokenCredential` records this error and goes on
    /// (`chainedTokenCredential.js:69-72`) rather than throwing it.
    pub(crate) fn continues_chain(&self) -> bool {
        matches!(self, Self::Unavailable(_) | Self::AuthenticationRequired(_))
    }
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message)
            | Self::AuthenticationRequired(message)
            | Self::Other(message) => f.write_str(message),
            // `errors.js:77`, trailing comma included.
            Self::Authentication {
                status,
                error,
                error_description,
            } => write!(
                f,
                "{error} Status code: {status}\nMore details:\n{error_description},"
            ),
            // `errors.js:97-99`: `${errorMessage}\n${errors.join("\n")}`.
            Self::Aggregate { message, errors } => {
                let detail = errors
                    .iter()
                    .map(CredentialError::to_js_string)
                    .collect::<Vec<_>>()
                    .join("\n");
                write!(f, "{message}\n{detail}")
            }
        }
    }
}

impl std::error::Error for CredentialError {}
