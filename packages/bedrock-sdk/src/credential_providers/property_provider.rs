//! Port of `@smithy/property-provider`'s errors.
//!
//! `chain` and `memoize` live where the chain uses them (`mod.rs`); this
//! file keeps the error every link reports.

/// `ProviderError` / `CredentialsProviderError`: a lookup that failed, and
/// whether the chain may move on to its next link (`tryNextLink`).
///
/// The npm classes default `tryNextLink` to `true`. Errors that are not
/// provider errors (a JSON syntax error, a file read error) have no
/// `tryNextLink` and stop the chain; here they are errors whose
/// [`try_next_link`](Self::try_next_link) is `false`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialsProviderError {
    message: String,
    try_next_link: bool,
}

impl CredentialsProviderError {
    /// `new CredentialsProviderError(message)`: the chain moves on.
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            try_next_link: true,
        }
    }

    /// An error that ends the chain (`tryNextLink: false`, or not a provider
    /// error at all).
    pub(crate) fn stop(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            try_next_link: false,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn try_next_link(&self) -> bool {
        self.try_next_link
    }
}

impl std::fmt::Display for CredentialsProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CredentialsProviderError {}
