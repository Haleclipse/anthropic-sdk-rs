// Maps to: TS internal/utils/log.ts
//
//! Request logging helpers.
//!
//! Rust's public [`crate::SdkLogger`] surface is string-oriented rather than
//! JavaScript's variadic logger calls, but this module preserves the key
//! structured helper behavior from TS `formatRequestDetails()`: clone/remove
//! redundant `options.headers`, redact sensitive headers, and rename
//! `retryOfRequestLogID` to `retryOf` when present.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::client::{Anthropic, LogLevel, SdkLogger};

/// TS log-level names accepted by `parseLogLevel()`.
pub const LOG_LEVELS: [&str; 5] = ["off", "error", "warn", "info", "debug"];

/// Maps to TS `parseLogLevel()` without performing warning side effects.
///
/// The client constructor emits the TS-style warning when `ANTHROPIC_LOG` is
/// invalid; this lower-level helper is useful for parity tests and callers that
/// only need validation.
pub fn parse_log_level(maybe_level: Option<&str>) -> Option<&'static str> {
    let level = maybe_level?;
    LOG_LEVELS
        .iter()
        .copied()
        .find(|candidate| *candidate == level)
}

/// TS-style alias for [`parse_log_level`].
#[allow(non_snake_case)]
pub fn parseLogLevel(maybe_level: Option<&str>) -> Option<&'static str> {
    parse_log_level(maybe_level)
}

/// Maps to TS `parseLogLevel(maybeLevel, sourceName, client)`, including the
/// warning side effect for invalid values.
///
/// Rust keeps [`parse_log_level`] as a validation-only helper because most
/// callers do not have a client handy during parsing. This variant mirrors the
/// full TS helper by warning through [`logger_for_client`] when `maybe_level` is
/// present but not one of [`LOG_LEVELS`].
pub fn parse_log_level_with_warning(
    maybe_level: Option<&str>,
    source_name: &str,
    client: &Anthropic,
) -> Option<&'static str> {
    let level = maybe_level?;
    if let Some(parsed) = parse_log_level(Some(level)) {
        return Some(parsed);
    }

    let level_json = serde_json::to_string(level).unwrap_or_else(|_| format!("{level:?}"));
    logger_for_client(client).warn(&format!(
        "{source_name} was set to {level_json}, expected one of [\"off\",\"error\",\"warn\",\"info\",\"debug\"]"
    ));
    None
}

/// TS-style alias for [`parse_log_level_with_warning`].
#[allow(non_snake_case)]
pub fn parseLogLevelWithWarning(
    maybe_level: Option<&str>,
    source_name: &str,
    client: &Anthropic,
) -> Option<&'static str> {
    parse_log_level_with_warning(maybe_level, source_name, client)
}

/// Level-filtered logger returned by [`logger_for`] / [`loggerFor`].
///
/// This maps to TS `loggerFor(client)`. Rust log calls remain string-oriented
/// via [`SdkLogger`] rather than JavaScript variadic calls, but the level
/// filtering and no-op behavior match TS `makeLogFn()`.
#[derive(Clone)]
pub struct LevelLogger {
    logger: Option<Arc<dyn SdkLogger>>,
    log_level: LogLevel,
}

impl LevelLogger {
    /// Log an error-level message if enabled.
    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    /// Log a warn-level message if enabled.
    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    /// Log an info-level message if enabled.
    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    /// Log a debug-level message if enabled.
    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    /// Log an error-level message with TS-style structured details if enabled.
    pub fn error_with_details(&self, message: &str, details: &Map<String, Value>) {
        self.log_with_details(LogLevel::Error, message, details);
    }

    /// Log a warn-level message with TS-style structured details if enabled.
    pub fn warn_with_details(&self, message: &str, details: &Map<String, Value>) {
        self.log_with_details(LogLevel::Warn, message, details);
    }

    /// Log an info-level message with TS-style structured details if enabled.
    pub fn info_with_details(&self, message: &str, details: &Map<String, Value>) {
        self.log_with_details(LogLevel::Info, message, details);
    }

    /// Log a debug-level message with TS-style structured details if enabled.
    pub fn debug_with_details(&self, message: &str, details: &Map<String, Value>) {
        self.log_with_details(LogLevel::Debug, message, details);
    }

    /// Return the configured log level.
    pub fn log_level(&self) -> LogLevel {
        self.log_level
    }

    fn log(&self, level: LogLevel, message: &str) {
        if !log_level_enables(self.log_level, level) {
            return;
        }
        if let Some(logger) = &self.logger {
            match level {
                LogLevel::Off => {}
                LogLevel::Error => logger.error(message),
                LogLevel::Warn => logger.warn(message),
                LogLevel::Info => logger.info(message),
                LogLevel::Debug => logger.debug(message),
            }
        }
    }

    fn log_with_details(&self, level: LogLevel, message: &str, details: &Map<String, Value>) {
        if !log_level_enables(self.log_level, level) {
            return;
        }
        if let Some(logger) = &self.logger {
            match level {
                LogLevel::Off => {}
                LogLevel::Error => logger.error_with_details(message, details),
                LogLevel::Warn => logger.warn_with_details(message, details),
                LogLevel::Info => logger.info_with_details(message, details),
                LogLevel::Debug => logger.debug_with_details(message, details),
            }
        }
    }
}

/// Return a level-filtered logger for explicit logger/level values.
pub fn logger_for(logger: Option<Arc<dyn SdkLogger>>, log_level: LogLevel) -> LevelLogger {
    LevelLogger { logger, log_level }
}

/// Return the no-op logger used when no custom logger exists.
pub fn noop_logger() -> LevelLogger {
    LevelLogger {
        logger: None,
        log_level: LogLevel::Off,
    }
}

/// Return a level-filtered logger for an [`Anthropic`] client.
pub fn logger_for_client(client: &Anthropic) -> LevelLogger {
    logger_for(client.logger(), client.log_level())
}

/// TS-style alias for [`logger_for_client`].
#[allow(non_snake_case)]
pub fn loggerFor(client: &Anthropic) -> LevelLogger {
    logger_for_client(client)
}

fn log_level_enables(configured: LogLevel, message_level: LogLevel) -> bool {
    configured != LogLevel::Off && log_level_number(message_level) <= log_level_number(configured)
}

fn log_level_number(level: LogLevel) -> u16 {
    match level {
        LogLevel::Off => 0,
        LogLevel::Error => 200,
        LogLevel::Warn => 300,
        LogLevel::Info => 400,
        LogLevel::Debug => 500,
    }
}

/// Maps to TS `formatRequestDetails(details)`.
///
/// The input/output shape is intentionally `serde_json::Map` so callers can use
/// arbitrary TS-like detail fields. The transformation matches the TS helper:
///
/// - `options.headers` is removed because it is redundant and may leak internals.
/// - `headers` values for `x-api-key`, `authorization`, `cookie`, and
///   `set-cookie` are replaced with `"***"` case-insensitively.
/// - `retryOfRequestLogID` is removed and copied to `retryOf` when truthy.
pub fn format_request_details(mut details: Map<String, Value>) -> Map<String, Value> {
    if let Some(Value::Object(options)) = details.get_mut("options") {
        options.remove("headers");
    }

    if let Some(Value::Object(headers)) = details.get_mut("headers") {
        for (name, value) in headers.iter_mut() {
            if is_sensitive_header(name) {
                *value = Value::String("***".to_owned());
            }
        }
    }

    if details.contains_key("retryOfRequestLogID") {
        let retry_of_request_log_id = details.remove("retryOfRequestLogID");
        if let Some(value) = retry_of_request_log_id {
            if js_truthy(&value) {
                details.insert("retryOf".to_owned(), value);
            }
        }
    }

    details
}

/// TS-style alias for [`format_request_details`].
#[allow(non_snake_case)]
pub fn formatRequestDetails(details: Map<String, Value>) -> Map<String, Value> {
    format_request_details(details)
}

/// Redact one header value using TS `formatRequestDetails()` rules.
pub fn redact_header_value(name: &str, value: impl Into<Value>) -> Value {
    if is_sensitive_header(name) {
        Value::String("***".to_owned())
    } else {
        value.into()
    }
}

/// TS-style alias for [`redact_header_value`].
#[allow(non_snake_case)]
pub fn redactHeaderValue(name: &str, value: impl Into<Value>) -> Value {
    redact_header_value(name, value)
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "x-api-key" | "authorization" | "cookie" | "set-cookie"
    )
}

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use crate::{Anthropic, ClientOptions};

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn parse_log_level_accepts_ts_levels_only() {
        assert_eq!(parse_log_level(None), None);
        assert_eq!(parse_log_level(Some("debug")), Some("debug"));
        assert_eq!(parseLogLevel(Some("warn")), Some("warn"));
        assert_eq!(parse_log_level(Some("verbose")), None);
        assert_eq!(parse_log_level(Some("DEBUG")), None);
    }

    type CapturedLogDetails = (LogLevel, String, Map<String, Value>);

    #[derive(Default)]
    struct CaptureLogger {
        messages: Mutex<Vec<(LogLevel, String)>>,
        details: Mutex<Vec<CapturedLogDetails>>,
    }

    impl SdkLogger for CaptureLogger {
        fn log(&self, level: LogLevel, message: &str) {
            self.messages
                .lock()
                .unwrap()
                .push((level, message.to_owned()));
        }

        fn log_with_details(&self, level: LogLevel, message: &str, details: &Map<String, Value>) {
            self.log(level, message);
            self.details
                .lock()
                .unwrap()
                .push((level, message.to_owned(), details.clone()));
        }
    }

    #[test]
    fn logger_for_filters_by_level_like_ts_make_log_fn_and_noops_without_logger() {
        let logger = Arc::new(CaptureLogger::default());
        let level_logger = logger_for(Some(logger.clone()), LogLevel::Info);
        level_logger.debug("debug suppressed");
        level_logger.info("info emitted");
        level_logger.warn("warn emitted");

        let messages = logger.messages.lock().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], (LogLevel::Info, "info emitted".to_owned()));
        assert_eq!(messages[1], (LogLevel::Warn, "warn emitted".to_owned()));
        drop(messages);

        noop_logger().error("ignored");
        assert_eq!(logger.messages.lock().unwrap().len(), 2);
    }

    #[test]
    fn logger_for_preserves_structured_details_like_ts_variadic_logger_shape() {
        let logger = Arc::new(CaptureLogger::default());
        let level_logger = logger_for(Some(logger.clone()), LogLevel::Debug);
        let details =
            object(json!({"url": "https://example.test", "headers": {"x-api-key": "***"}}));
        level_logger.debug_with_details("[log_000000] sending request", &details);
        level_logger.info_with_details("info details", &details);

        let captured = logger.details.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].0, LogLevel::Debug);
        assert_eq!(captured[0].1, "[log_000000] sending request");
        assert_eq!(captured[0].2["url"], "https://example.test");
        assert_eq!(captured[0].2["headers"]["x-api-key"], "***");
    }

    #[test]
    fn logger_for_client_alias_uses_client_logger_and_log_level() {
        let logger = Arc::new(CaptureLogger::default());
        let client = Anthropic::new(ClientOptions {
            api_key: Some("test-api-key".to_owned()),
            logger: Some(logger.clone()),
            log_level: Some(LogLevel::Error),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(client.logLevel(), LogLevel::Error);
        assert_eq!(client.logLevel().as_str(), "error");

        let level_logger = loggerFor(&client);
        assert_eq!(level_logger.log_level(), LogLevel::Error);
        level_logger.warn("warn suppressed");
        level_logger.error("error emitted");

        let messages = logger.messages.lock().unwrap();
        assert_eq!(
            messages.as_slice(),
            &[(LogLevel::Error, "error emitted".to_owned())]
        );
    }

    #[test]
    fn parse_log_level_with_warning_matches_ts_side_effect_shape() {
        let logger = Arc::new(CaptureLogger::default());
        let client = Anthropic::new(ClientOptions {
            api_key: Some("test-api-key".to_owned()),
            logger: Some(logger.clone()),
            log_level: Some(LogLevel::Warn),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            parseLogLevelWithWarning(Some("debug"), "ClientOptions.logLevel", &client),
            Some("debug")
        );
        assert_eq!(
            parse_log_level_with_warning(
                Some("verbose\"level"),
                "process.env['ANTHROPIC_LOG']",
                &client,
            ),
            None
        );

        let messages = logger.messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, LogLevel::Warn);
        assert_eq!(
            messages[0].1,
            "process.env['ANTHROPIC_LOG'] was set to \"verbose\\\"level\", expected one of [\"off\",\"error\",\"warn\",\"info\",\"debug\"]"
        );
    }

    #[test]
    fn format_request_details_removes_option_headers_redacts_headers_and_sets_retry_of() {
        let details = object(json!({
            "options": {
                "method": "post",
                "headers": {"x-api-key": "inside-options"},
                "path": "/v1/messages"
            },
            "headers": {
                "x-api-key": "secret-key",
                "Authorization": "Bearer token",
                "cookie": "a=b",
                "set-cookie": "c=d",
                "content-type": "application/json"
            },
            "retryOfRequestLogID": "log_original",
            "url": "https://api.anthropic.com/v1/messages"
        }));

        let formatted = format_request_details(details);
        assert!(formatted["options"].get("headers").is_none());
        assert_eq!(formatted["options"]["method"], "post");
        assert_eq!(formatted["headers"]["x-api-key"], "***");
        assert_eq!(formatted["headers"]["Authorization"], "***");
        assert_eq!(formatted["headers"]["cookie"], "***");
        assert_eq!(formatted["headers"]["set-cookie"], "***");
        assert_eq!(formatted["headers"]["content-type"], "application/json");
        assert!(formatted.get("retryOfRequestLogID").is_none());
        assert_eq!(formatted["retryOf"], "log_original");
    }

    #[test]
    fn format_request_details_drops_retry_of_request_log_id_when_falsey_like_ts() {
        for value in [
            Value::Null,
            Value::Bool(false),
            Value::String(String::new()),
        ] {
            let mut details = Map::new();
            details.insert("retryOfRequestLogID".to_owned(), value);
            let formatted = formatRequestDetails(details);
            assert!(formatted.get("retryOfRequestLogID").is_none());
            assert!(formatted.get("retryOf").is_none());
        }
    }

    #[test]
    fn redact_header_value_alias_matches_ts_sensitive_header_set() {
        assert_eq!(redact_header_value("X-API-Key", "secret"), json!("***"));
        assert_eq!(redactHeaderValue("authorization", "token"), json!("***"));
        assert_eq!(redact_header_value("x-request-id", "req"), json!("req"));
    }
}
