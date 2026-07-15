// Maps to: TS internal/errors.ts
//
//! Error classification and conversion utilities for the Anthropic Rust SDK.
//!
//! Provides helpers to detect abort/cancellation errors and to convert
//! arbitrary error-like values into `Box<dyn Error>`.

use std::error::Error;
use std::fmt;

// ---------------------------------------------------------------------------
// is_abort_error
// ---------------------------------------------------------------------------

/// Maps to: TS `isAbortError()` in internal/errors.ts
///
/// Returns `true` if `e` represents a request that was intentionally
/// cancelled / aborted.  In Rust this checks:
///
/// - The error's `Display` message contains "canceled" or "aborted"
///   (case-insensitive) -- this catches `reqwest::Error` wrapping
///   `hyper::Error("request canceled")` as well as `tokio::JoinError`.
/// - The error chain (via `source()`) is walked so that wrapped errors are
///   also detected.
pub fn is_abort_error(e: &dyn Error) -> bool {
    // Walk the error chain.
    let mut current: Option<&dyn Error> = Some(e);
    while let Some(err) = current {
        let msg = err.to_string().to_lowercase();
        if msg.contains("canceled") || msg.contains("aborted") || msg.contains("cancelled") {
            return true;
        }
        current = err.source();
    }
    false
}

// ---------------------------------------------------------------------------
// cast_to_error
// ---------------------------------------------------------------------------

/// Maps to: TS `castToError()` in internal/errors.ts
///
/// Wraps an arbitrary value that implements `Into<Box<dyn Error>>` into a
/// boxed trait object.  This is the Rust equivalent of the TS function that
/// ensures any thrown value becomes a proper `Error` instance.
pub fn cast_to_error<E>(e: E) -> Box<dyn Error + Send + Sync + 'static>
where
    E: Into<Box<dyn Error + Send + Sync + 'static>>,
{
    e.into()
}

// ---------------------------------------------------------------------------
// StringError (internal helper)
// ---------------------------------------------------------------------------

/// A simple error type wrapping a `String`, used by [`cast_to_error_from_string`].
#[derive(Debug)]
struct StringError(String);

impl fmt::Display for StringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StringError {}

/// Converts a plain string message into a boxed error.
///
/// This is a convenience overload for cases where the caller has a raw
/// message string rather than a concrete error type.
pub fn cast_to_error_from_string(msg: String) -> Box<dyn Error + Send + Sync + 'static> {
    Box::new(StringError(msg))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// A test error that mimics a hyper cancellation.
    #[derive(Debug)]
    struct CancelError;

    impl fmt::Display for CancelError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "request canceled")
        }
    }

    impl Error for CancelError {}

    /// A wrapper error that has a source chain.
    #[derive(Debug)]
    struct WrappedError {
        source: CancelError,
    }

    impl fmt::Display for WrappedError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "connection failed")
        }
    }

    impl Error for WrappedError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.source)
        }
    }

    #[test]
    fn is_abort_error_direct() {
        let e = CancelError;
        assert!(is_abort_error(&e));
    }

    #[test]
    fn is_abort_error_wrapped() {
        let e = WrappedError {
            source: CancelError,
        };
        assert!(is_abort_error(&e));
    }

    #[test]
    fn is_abort_error_not_cancel() {
        let e = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe");
        assert!(!is_abort_error(&e));
    }

    #[test]
    fn is_abort_error_aborted_message() {
        let e = std::io::Error::other("operation aborted by user");
        assert!(is_abort_error(&e));
    }

    #[test]
    fn cast_to_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let boxed = cast_to_error(io_err);
        assert!(boxed.to_string().contains("file not found"));
    }

    #[test]
    fn cast_to_error_from_string_works() {
        let boxed = cast_to_error_from_string("something went wrong".into());
        assert_eq!(boxed.to_string(), "something went wrong");
    }
}
