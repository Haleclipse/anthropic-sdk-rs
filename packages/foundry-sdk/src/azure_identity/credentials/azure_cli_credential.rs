//! Maps to: `@azure/identity` `dist/esm/credentials/azureCliCredential.js`,
//! with the scope helpers it calls (`dist/esm/util/scopeUtils.js:15-28`).
//!
//! CC builds it through `DefaultAzureCredential` with no options
//! (`credentials/defaultAzureCredentialFunctions.js:126-128`), and the bearer
//! policy passes neither claims nor a tenant to `getToken`. So there is no
//! tenant, no subscription and no process timeout, and the claims branch
//! (`azureCliCredential.js:122-133`) and the tenant and subscription checks
//! (`:134-140`) never run; they are left out. Nothing is cached: every call
//! starts `az` again, as in TS. The `credentialLogger` lines (written to
//! stderr only under `AZURE_LOG_LEVEL`) and the tracing span are not ported.
//!
//! Every failure ends as a `CredentialUnavailableError` (`:173-178`), so this
//! credential never halts the chain.

use std::path::PathBuf;
use std::process::Stdio;

use chrono::{Days, Local, TimeZone};
use futures::future::BoxFuture;
use serde_json::Value;

use crate::azure_identity::js::{JsTruthy, js_string, parse_int, template_string, time_clip};
use crate::azure_identity::{
    AccessToken, CredentialError, Environment, TokenCredential, timestamp,
};

/// `azureCliPublicErrorMessages` (`:15-21`), less the claims one.
const NOT_INSTALLED: &str = "Azure CLI could not be found. Please visit https://aka.ms/azure-cli for installation instructions and then, once installed, authenticate to your Azure account using 'az login'.";
const LOGIN: &str =
    "Please run 'az login' from a command prompt to authenticate before using this credential.";
const UNKNOWN: &str = "Unknown error while trying to retrieve the access token";
const UNEXPECTED_RESPONSE: &str = "Unexpected response from Azure CLI when getting token. Expected \"expiresOn\" to be a RFC3339 date string. Got:";

/// Maps to: `AzureCliCredential`.
pub struct AzureCliCredential {
    /// The `process.env` `az` runs with.
    env: Environment,
}

impl AzureCliCredential {
    /// `new AzureCliCredential()` (`:100-111`), which never throws without
    /// options.
    pub fn new(env: Environment) -> Self {
        Self { env }
    }
}

impl TokenCredential for AzureCliCredential {
    /// `getToken(scopes)` (`:120-181`): `scopes[0]` is the one scope used.
    fn get_token<'a>(
        &'a self,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        Box::pin(async move {
            request_token(&self.env, scopes.first().map(String::as_str))
                .await
                .map(Some)
                // `:173-178`: anything but a `CredentialUnavailableError` is
                // rewrapped as one, `err.message || unknown`.
                .map_err(|error| match error {
                    CredentialError::Unavailable(_) => error,
                    other => {
                        let message = other.to_string();
                        CredentialError::Unavailable(if message.is_empty() {
                            UNKNOWN.to_owned()
                        } else {
                            message
                        })
                    }
                })
        })
    }
}

/// The `try` block of `:143-172`. [`CredentialError::Other`] stands for a JS
/// error that is not a credential one (`Error`, `SyntaxError`, `TypeError`).
async fn request_token(
    env: &Environment,
    scope: Option<&str>,
) -> Result<AccessToken, CredentialError> {
    // `scope.match` on `undefined`, in JSC's wording (the names in the bundled
    // CC are minified, so its text differs). CC always passes one scope.
    let scope = scope.ok_or_else(|| {
        CredentialError::Other("undefined is not an object (evaluating 'scope.match')".to_owned())
    })?;
    ensure_valid_scope_for_dev_time_creds(scope)?;
    let resource = get_scope_resource(scope);
    let obj = get_azure_cli_access_token(env, resource).await;
    // `:147-149`. Each string pattern becomes a `RegExp` whose `(.*)` may be
    // empty, so the two login tests are substring tests. The exit code is
    // never looked at.
    let specific_scope = obj.stderr.contains("az login --scope");
    let is_login_error = obj.stderr.contains("az login") && !specific_scope;
    let is_not_install_error =
        matches_az_not_found(&obj.stderr) || obj.stderr.starts_with("'az' is not recognized");
    if is_not_install_error {
        return Err(CredentialError::Unavailable(NOT_INSTALLED.to_owned()));
    }
    if is_login_error {
        return Err(CredentialError::Unavailable(LOGIN.to_owned()));
    }
    // `:160-171`: a parse failure gives way to whatever was on stderr, as is
    // (not trimmed; whitespace alone counts).
    parse_raw_response(&obj.stdout).map_err(|error| {
        if obj.stderr.is_empty() {
            error
        } else {
            CredentialError::Unavailable(obj.stderr)
        }
    })
}

/// `"az:(.*)not found"`: `az:` then `not found` later on the same line (`.`
/// matches anything but a JS line terminator).
fn matches_az_not_found(stderr: &str) -> bool {
    stderr
        .split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .any(|line| {
            line.find("az:")
                .is_some_and(|start| line[start + "az:".len()..].contains("not found"))
        })
}

/// Maps to: `scopeUtils.js:15-21` `ensureValidScopeForDevTimeCreds`, the test
/// `/^[0-9a-zA-Z-_.:/]+$/` (the `-` after the `A-Z` range is a literal). Its
/// plain `Error` is rewrapped by the caller.
fn ensure_valid_scope_for_dev_time_creds(scope: &str) -> Result<(), CredentialError> {
    let valid = !scope.is_empty()
        && scope.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        });
    if valid {
        Ok(())
    } else {
        Err(CredentialError::Other(
            "Invalid scope was specified by the user or calling client".to_owned(),
        ))
    }
}

/// Maps to: `scopeUtils.js:26-28` `getScopeResource`,
/// `scope.replace(/\/.default$/, "")`. The `.` is unescaped, so any character
/// stands between the slash and `default`; the scope has been validated, so it
/// is ASCII and that character is one byte.
fn get_scope_resource(scope: &str) -> &str {
    const SUFFIX_LEN: usize = "/.default".len();
    let bytes = scope.as_bytes();
    if bytes.len() >= SUFFIX_LEN
        && bytes.ends_with(b"default")
        && bytes[bytes.len() - SUFFIX_LEN] == b'/'
    {
        &scope[..bytes.len() - SUFFIX_LEN]
    } else {
        scope
    }
}

/// What `child_process.exec` hands its callback, less the `error` the
/// credential ignores (`:71-73`).
#[derive(Default)]
struct ExecOutput {
    stdout: String,
    stderr: String,
}

/// Maps to: `cliCredentialInternals.getAzureCliAccessToken` (`:48-79`)
/// without the tenant and subscription sections and the timeout, which CC
/// never sets. The resource has passed the scope test, so joining it into a
/// shell command line cannot inject anything.
async fn get_azure_cli_access_token(env: &Environment, resource: &str) -> ExecOutput {
    let command = [
        "az",
        "account",
        "get-access-token",
        "--output",
        "json",
        "--resource",
        resource,
    ]
    .join(" ");
    exec(env, &command, get_safe_working_dir(env)).await
}

/// Maps to: `cliCredentialInternals.getSafeWorkingDir` (`:30-42`). TS warns
/// when it falls back to `C:\Windows`; the logger is not ported.
fn get_safe_working_dir(env: &Environment) -> PathBuf {
    if cfg!(windows) {
        env.var_os("SystemRoot")
            .truthy()
            .or_else(|| env.var_os("SYSTEMROOT").truthy())
            .map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from)
    } else {
        PathBuf::from("/bin")
    }
}

/// `child_process.exec(command, { cwd }, callback)` as `:71-73` calls it:
/// the shell gets `process.env` (here the passed-in one, exactly), its stdin
/// pipe stays open and unwritten, output is decoded as UTF-8, and there is
/// no timeout. The callback's `error` is ignored, so a shell that cannot
/// start yields empty output, as Node's callback receives.
///
/// Not ported: `exec`'s 1 MiB `maxBuffer`, past which Node kills the child
/// and truncates its output.
async fn exec(env: &Environment, command: &str, cwd: PathBuf) -> ExecOutput {
    let mut shell = shell_command(env, command);
    shell
        .env_clear()
        .envs(env.iter())
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Ok(mut child) = shell.spawn() else {
        return ExecOutput::default();
    };
    // `wait_with_output` closes a stdin it still holds; taking it keeps the
    // pipe open until the child has exited, as Node's is.
    let stdin = child.stdin.take();
    let output = child.wait_with_output().await;
    drop(stdin);
    match output {
        Ok(output) => ExecOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(_) => ExecOutput::default(),
    }
}

/// Node's `normalizeSpawnArguments` for `shell: true` (Bun's
/// `child_process` has the same rules): `/bin/sh -c <command>`
/// (`/system/bin/sh` on Android).
#[cfg(not(windows))]
fn shell_command(_env: &Environment, command: &str) -> tokio::process::Command {
    let file = if cfg!(target_os = "android") {
        "/system/bin/sh"
    } else {
        "/bin/sh"
    };
    let mut shell = tokio::process::Command::new(file);
    shell.args(["-c", command]);
    shell
}

/// Windows: `process.env.comspec || 'cmd.exe'`; for `cmd` the arguments are
/// `/d /s /c "<command>"`, passed verbatim (`windowsVerbatimArguments`), and
/// for any other shell `-c <command>`.
#[cfg(windows)]
fn shell_command(env: &Environment, command: &str) -> tokio::process::Command {
    let file = env.var_os("comspec").truthy().map_or_else(
        || std::ffi::OsString::from("cmd.exe"),
        std::ffi::OsStr::to_os_string,
    );
    let mut shell = tokio::process::Command::new(&file);
    if is_cmd_shell(&file.to_string_lossy()) {
        shell.args(["/d", "/s", "/c"]);
        shell.raw_arg(format!("\"{command}\""));
    } else {
        shell.args(["-c", command]);
    }
    shell
}

/// `/^(?:.*\\)?cmd(?:\.exe)?$/i`: the part after the last backslash is `cmd`
/// or `cmd.exe`, in any case.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_cmd_shell(file: &str) -> bool {
    let name = file.rsplit_once('\\').map_or(file, |(_, name)| name);
    name.eq_ignore_ascii_case("cmd") || name.eq_ignore_ascii_case("cmd.exe")
}

/// Maps to: `AzureCliCredential.parseRawResponse` (`:192-217`), reading the
/// JSON as JS reads properties: a field of anything but an object is
/// `undefined`. `refreshAfterTimestamp` is never set.
///
/// The token is kept as `String(response.accessToken)`: the only reader is
/// the bearer policy's `Bearer ${token}`, so a missing one is `"undefined"`.
/// A `SyntaxError` carries serde_json's diagnostic, not JSC's (the same
/// boundary as the other native JSON parses here); it only surfaces when
/// stderr is empty.
fn parse_raw_response(raw_response: &str) -> Result<AccessToken, CredentialError> {
    let response: Value = serde_json::from_str(raw_response)
        .map_err(|error| CredentialError::Other(error.to_string()))?;
    if response.is_null() {
        // `response.accessToken` on `null`, in JSC's wording.
        return Err(CredentialError::Other(
            "null is not an object (evaluating 'response.accessToken')".to_owned(),
        ));
    }
    let field = |key: &str| response.as_object().and_then(|object| object.get(key));
    let token = template_string(field("accessToken"));
    // `Number.parseInt(response.expires_on, 10) * 1000`: seconds since the
    // epoch, from Azure CLI 2.54.0 on.
    if let Some(seconds) = field("expires_on")
        .map(|value| parse_int(&js_string(value)))
        .filter(|seconds| !seconds.is_nan())
    {
        return Ok(AccessToken {
            token,
            expires_on_timestamp: timestamp(seconds * 1000.0),
            refresh_after_timestamp: None,
        });
    }
    // `new Date(response.expiresOn).getTime()`, the older local-time string.
    let expires_on = field("expiresOn");
    let Some(expires_on_timestamp) = date_time_value(expires_on) else {
        return Err(CredentialError::Unavailable(format!(
            "{UNEXPECTED_RESPONSE} \"{}\"",
            template_string(expires_on)
        )));
    };
    Ok(AccessToken {
        token,
        expires_on_timestamp: timestamp(expires_on_timestamp),
        refresh_after_timestamp: None,
    })
}

/// `new Date(value).getTime()`, `None` for `NaN`. A string goes through
/// [`parse_date`]; `null` and booleans are numbers (`null` is the epoch); an
/// array or object is first turned into its string.
fn date_time_value(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Null => Some(0.0),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        Value::Number(number) => time_clip(number.as_f64()?),
        Value::String(text) => parse_date(text),
        other => parse_date(&js_string(other)),
    }
}

/// `Date.parse`, explicitly partial (as in `utils/plugins/install_counts.rs`):
/// the ES date-time string format, and the shape Azure CLI writes for
/// `expiresOn`, `YYYY-MM-DD HH:MM:SS.ffffff` in local time. Within those two
/// shapes the results are Bun's (fields that overflow the month roll over,
/// `24:00` is the next midnight, the legacy two-digit-year rule applies to
/// the second shape); any other string Bun's lenient parser accepts is `NaN`
/// here. Local time is chrono's `Local`, which reads the OS `TZ`, not the
/// environment carrier.
fn parse_date(text: &str) -> Option<f64> {
    parse_iso_date(text).or_else(|| parse_legacy_date(text))
}

/// The ES date-time string format: `YYYY[-MM[-DD]]` or `±YYYYYY…`, then
/// optionally `THH:mm[:ss[.s…]]` and `Z` or `±HH:mm`. A date alone is UTC, a
/// date-time without an offset local time.
fn parse_iso_date(text: &str) -> Option<f64> {
    let mut scanner = Scanner::new(text);
    let year = match scanner.peek() {
        Some(sign @ (b'+' | b'-')) => {
            scanner.advance();
            let year = scanner.digits(6, 6)?;
            // `-000000` is not a year.
            if sign == b'-' && year == 0 {
                return None;
            }
            if sign == b'-' { -year } else { year }
        }
        _ => scanner.digits(4, 4)?,
    };
    let (mut month, mut day) = (1, 1);
    if scanner.eat(b'-') {
        month = scanner.digits(2, 2)?;
        if scanner.eat(b'-') {
            day = scanner.digits(2, 2)?;
        }
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if scanner.at_end() {
        return time_clip(make_date(year, month, day, 0, 0, 0, 0) as f64);
    }
    if !scanner.eat(b'T') {
        return None;
    }
    let hour = scanner.digits(2, 2)?;
    if !scanner.eat(b':') {
        return None;
    }
    let minute = scanner.digits(2, 2)?;
    let (mut second, mut millisecond) = (0, 0);
    if scanner.eat(b':') {
        second = scanner.digits(2, 2)?;
        if scanner.eat(b'.') {
            millisecond = scanner.fraction()?;
        }
    }
    let offset_minutes = if scanner.eat(b'Z') {
        Some(0)
    } else if let Some(sign @ (b'+' | b'-')) = scanner.peek() {
        scanner.advance();
        let hours = scanner.digits(2, 2)?;
        if !scanner.eat(b':') {
            return None;
        }
        let minutes = scanner.digits(2, 2)?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        let offset = hours * 60 + minutes;
        Some(if sign == b'-' { -offset } else { offset })
    } else {
        None
    };
    if !scanner.at_end() || !valid_time(hour, minute, second, millisecond) {
        return None;
    }
    let date = make_date(year, month, day, hour, minute, second, millisecond);
    match offset_minutes {
        Some(offset) => time_clip((date - offset * 60_000) as f64),
        None => local_milliseconds(date),
    }
}

/// Azure CLI's `expiresOn`, read by Bun's legacy parser: optional whitespace,
/// `Y-M-D`, whitespace, then `H:M[:S[.f…]]` and optional whitespace, as
/// local time. A 0-49 year is 20xx and a 50-99 one 19xx. A first field of
/// 1-31 is read by Bun as a month (`M-D-Y`), which is not ported; such
/// strings are `NaN` here. Fields past the year have one or two digits.
fn parse_legacy_date(text: &str) -> Option<f64> {
    let is_space = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r');
    let mut scanner = Scanner::new(text);
    scanner.skip(is_space);
    let year = scanner.digits(1, 6)?;
    if !scanner.eat(b'-') {
        return None;
    }
    let month = scanner.digits(1, 2)?;
    if !scanner.eat(b'-') {
        return None;
    }
    let day = scanner.digits(1, 2)?;
    if scanner.skip(is_space) == 0 {
        return None;
    }
    let hour = scanner.digits(1, 2)?;
    if !scanner.eat(b':') {
        return None;
    }
    let minute = scanner.digits(1, 2)?;
    let (mut second, mut millisecond) = (0, 0);
    if scanner.eat(b':') {
        second = scanner.digits(1, 2)?;
        if scanner.eat(b'.') {
            millisecond = scanner.fraction()?;
        }
    }
    scanner.skip(is_space);
    if !scanner.at_end()
        || (1..=31).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !valid_time(hour, minute, second, millisecond)
    {
        return None;
    }
    let year = match year {
        0..=49 => year + 2000,
        50..=99 => year + 1900,
        _ => year,
    };
    local_milliseconds(make_date(
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
    ))
}

/// Hours 0-24, where 24 is only `24:00:00.000`; minutes and seconds 0-59.
fn valid_time(hour: i64, minute: i64, second: i64, millisecond: i64) -> bool {
    let midnight_end = hour == 24 && minute == 0 && second == 0 && millisecond == 0;
    (hour < 24 || midnight_end) && minute < 60 && second < 60
}

/// `MakeDate(MakeDay(year, month - 1, day), MakeTime(…))` in milliseconds
/// from the epoch, on the proleptic Gregorian calendar (the day count is
/// Howard Hinnant's `days_from_civil`). It is linear in `day`, so a day past
/// the end of the month rolls into the next, as `MakeDay` does. Callers have
/// bounded every field, so nothing overflows.
fn make_date(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millisecond: i64,
) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * 86_400_000 + ((hour * 60 + minute) * 60 + second) * 1000 + millisecond
}

/// ECMA-262 `UTC(t)` on the local zone, for `local` (the local wall time
/// counted as if it were UTC): a repeated local time is the earlier instant;
/// a skipped one takes the offset in force before the transition. Years
/// beyond chrono's range (±262 142) are `NaN` here, though Bun reaches the
/// `TimeClip` limit.
fn local_milliseconds(local: i64) -> Option<f64> {
    use chrono::LocalResult;
    let date_time = chrono::DateTime::from_timestamp_millis(local)?.naive_utc();
    let milliseconds = match Local.from_local_datetime(&date_time) {
        LocalResult::Single(instant) | LocalResult::Ambiguous(instant, _) => {
            instant.timestamp_millis()
        }
        LocalResult::None => {
            let before = Local.offset_from_utc_datetime(&date_time.checked_sub_days(Days::new(1))?);
            local - i64::from(before.local_minus_utc()) * 1000
        }
    };
    time_clip(milliseconds as f64)
}

/// A byte cursor for the two date shapes, which are ASCII.
struct Scanner<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn advance(&mut self) {
        self.position += 1;
    }

    fn at_end(&self) -> bool {
        self.position == self.bytes.len()
    }

    fn eat(&mut self, byte: u8) -> bool {
        let matched = self.peek() == Some(byte);
        if matched {
            self.advance();
        }
        matched
    }

    /// Skips bytes matching `predicate`; returns how many.
    fn skip(&mut self, predicate: impl Fn(u8) -> bool) -> usize {
        let start = self.position;
        while self.peek().is_some_and(&predicate) {
            self.advance();
        }
        self.position - start
    }

    /// Between `min` and `max` decimal digits (stopping at `max`).
    fn digits(&mut self, min: usize, max: usize) -> Option<i64> {
        let start = self.position;
        let mut value = 0i64;
        while self.position - start < max {
            let Some(digit @ b'0'..=b'9') = self.peek() else {
                break;
            };
            value = value * 10 + i64::from(digit - b'0');
            self.advance();
        }
        (self.position - start >= min).then_some(value)
    }

    /// A fraction of a second, one digit or more, as whole milliseconds
    /// (digits past the third are dropped).
    fn fraction(&mut self) -> Option<i64> {
        let start = self.position;
        let mut millisecond = 0;
        while let Some(digit @ b'0'..=b'9') = self.peek() {
            if self.position - start < 3 {
                millisecond = millisecond * 10 + i64::from(digit - b'0');
            }
            self.advance();
        }
        let count = self.position - start;
        (count > 0).then(|| millisecond * 10_i64.pow(3 - count.min(3) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use serde_json::json;

    const SCOPE: &str = "https://cognitiveservices.azure.com/.default";

    fn local_ms(
        (year, month, day): (i32, u32, u32),
        (hour, minute, second, millisecond): (u32, u32, u32, u32),
    ) -> f64 {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_milli_opt(hour, minute, second, millisecond)
            .unwrap();
        Local
            .from_local_datetime(&naive)
            .earliest()
            .unwrap()
            .timestamp_millis() as f64
    }

    fn unavailable(message: &str) -> CredentialError {
        CredentialError::Unavailable(message.to_owned())
    }

    /// `scopeUtils.js:15-28`, and `:173-178` turning the plain `Error`s into
    /// unavailable ones before anything is started.
    #[tokio::test]
    async fn scope_handling_matches_official_scope_utils() {
        assert_eq!(
            get_scope_resource(SCOPE),
            "https://cognitiveservices.azure.com"
        );
        // The unescaped `.` matches any character.
        assert_eq!(get_scope_resource("https://x/Xdefault"), "https://x");
        assert_eq!(
            get_scope_resource("https://x/.default/"),
            "https://x/.default/"
        );
        assert_eq!(get_scope_resource("api://app"), "api://app");
        assert!(ensure_valid_scope_for_dev_time_creds("api://a-b_c.d:1/x").is_ok());
        for invalid in ["", "a b", "https://x/.default?q", "scope\n", "é"] {
            assert!(
                ensure_valid_scope_for_dev_time_creds(invalid).is_err(),
                "{invalid:?}"
            );
        }

        let credential = AzureCliCredential::new(Environment::default());
        assert_eq!(
            credential.get_token(&["bad scope".to_owned()]).await,
            Err(unavailable(
                "Invalid scope was specified by the user or calling client"
            ))
        );
        assert_eq!(
            credential.get_token(&[]).await,
            Err(unavailable(
                "undefined is not an object (evaluating 'scope.match')"
            ))
        );
    }

    /// `:147-149` and Node's shell selection.
    #[test]
    fn stderr_and_shell_matchers_match_official_patterns() {
        assert!(matches_az_not_found("/bin/sh: az: command not found\n"));
        assert!(matches_az_not_found("/bin/sh: 1: az: not found\n"));
        assert!(matches_az_not_found("warning\nsh: az:x not found"));
        assert!(!matches_az_not_found("az:\nnot found"));
        assert!(!matches_az_not_found("not found az:"));
        assert!(is_cmd_shell("C:\\WINDOWS\\system32\\cmd.exe"));
        assert!(is_cmd_shell("CMD"));
        assert!(!is_cmd_shell("C:/Windows/System32/cmd.exe"));
        assert!(!is_cmd_shell("C:\\tools\\pwsh.exe"));
    }

    /// `:192-217`, with Bun's `parseInt`, `String()` and `new Date()`.
    #[test]
    fn parse_raw_response_matches_official_field_rules() {
        let parsed = |raw: &str| parse_raw_response(raw);
        let token = |raw: &str| parsed(raw).unwrap();

        let from_seconds = token(r#"{"accessToken":"t","expires_on":" 42abc","expiresOn":"x"}"#);
        assert_eq!(from_seconds.token, "t");
        assert_eq!(from_seconds.expires_on_timestamp, 42_000);
        assert_eq!(from_seconds.refresh_after_timestamp, None);
        // `String(1e21)` is `1e+21`, which `parseInt` reads as 1.
        assert_eq!(token(r#"{"expires_on":1e21}"#).expires_on_timestamp, 1000);
        assert_eq!(token(r#"{"expires_on":[17]}"#).expires_on_timestamp, 17_000);
        assert_eq!(token(r#"{"expires_on":"-5"}"#).expires_on_timestamp, 0);
        // A missing or non-string token is rendered as the template would.
        assert_eq!(token(r#"{"expires_on":"1"}"#).token, "undefined");
        assert_eq!(
            token(r#"{"accessToken":123,"expires_on":"1"}"#).token,
            "123"
        );
        // `parseInt("null")` is NaN, then `new Date(null)` is the epoch.
        assert_eq!(
            token(r#"{"expires_on":null,"expiresOn":null}"#).expires_on_timestamp,
            0
        );
        assert_eq!(
            token(r#"{"expiresOn":"2023-10-31T21:59:10Z"}"#).expires_on_timestamp,
            1_698_789_550_000
        );

        let got = |shown: &str| unavailable(&format!("{UNEXPECTED_RESPONSE} \"{shown}\""));
        assert_eq!(parsed(r#"{"expiresOn":"soon"}"#), Err(got("soon")));
        assert_eq!(parsed(r#"{"accessToken":"t"}"#), Err(got("undefined")));
        assert_eq!(parsed(r#"{"expiresOn":{}}"#), Err(got("[object Object]")));
        assert_eq!(parsed("7"), Err(got("undefined")));
        assert_eq!(
            parsed("null"),
            Err(CredentialError::Other(
                "null is not an object (evaluating 'response.accessToken')".to_owned()
            ))
        );
        assert!(matches!(parsed(""), Err(CredentialError::Other(_))));
        assert!(matches!(parsed("{} x"), Err(CredentialError::Other(_))));
    }

    /// `new Date(value).getTime()` for the shapes ported, against values
    /// sampled from Bun 1.4.2 (the local ones are recomputed in this zone).
    #[test]
    fn date_parse_matches_bun_for_supported_shapes() {
        for (text, expected) in [
            ("2023-10-31T21:59:10Z", 1_698_789_550_000.0),
            ("2023-10-31", 1_698_710_400_000.0),
            ("2023-10", 1_696_118_400_000.0),
            ("2023", 1_672_531_200_000.0),
            ("2023-02-30", 1_677_715_200_000.0),
            ("2023-10-31T21:59:10.000000+08:00", 1_698_760_750_000.0),
            ("2023-10-31T21:59:10+23:59", 1_698_703_210_000.0),
            ("2023-10-31T21:59Z", 1_698_789_540_000.0),
            ("2023-10-31T24:00Z", 1_698_796_800_000.0),
            ("2023-10-31T21:59:10.1234567890123Z", 1_698_789_550_123.0),
            ("+275760-09-13T00:00:00Z", 8.64e15),
            ("-000001-01-01T00:00:00Z", -62_198_755_200_000.0),
        ] {
            assert_eq!(parse_date(text), Some(expected), "{text:?}");
        }
        for (text, date, time) in [
            (
                "2023-10-31 21:59:10.000000",
                (2023, 10, 31),
                (21, 59, 10, 0),
            ),
            (
                "2023-10-31 21:59:10.123456",
                (2023, 10, 31),
                (21, 59, 10, 123),
            ),
            ("  2023-1-5\t 1:02:03.5\n", (2023, 1, 5), (1, 2, 3, 500)),
            ("2023-10-31 21:59", (2023, 10, 31), (21, 59, 0, 0)),
            ("2023-02-30 01:00:00", (2023, 3, 2), (1, 0, 0, 0)),
            ("2023-10-31 24:00:00", (2023, 11, 1), (0, 0, 0, 0)),
            ("0049-01-01 00:00:00", (2049, 1, 1), (0, 0, 0, 0)),
            ("0000-01-01 00:00:00", (2000, 1, 1), (0, 0, 0, 0)),
            ("0100-01-01 00:00:00", (100, 1, 1), (0, 0, 0, 0)),
            ("2023-10-31T21:59:10.5", (2023, 10, 31), (21, 59, 10, 500)),
        ] {
            assert_eq!(parse_date(text), Some(local_ms(date, time)), "{text:?}");
        }
        for text in [
            "not a date",
            "2023-10-31 25:59:10",
            "2023-10-31 24:00:00.001",
            "2023-10-31 23:60:00",
            "2023-13-01 00:00:00",
            "2023-10-32 00:00:00",
            "2023-10-31 21:59:10.",
            "2023-10-31 21",
            "2023-10-31T24:00:01",
            "2023-10-31T23:59:60Z",
            "2023-10-31T21:59:10+24:00",
            "2023-10-31T21:59:10 ",
            " 2023-10-31T21:59:10Z",
            "2023-1-31T21:59:10Z",
            "2023-10-31T21Z",
            "2023-10-32T00:00:00",
            "-000000-01-01T00:00:00Z",
            "+275760-09-13T00:00:00.001Z",
        ] {
            assert_eq!(parse_date(text), None, "{text:?}");
        }

        assert_eq!(date_time_value(None), None);
        assert_eq!(date_time_value(Some(&json!(null))), Some(0.0));
        assert_eq!(date_time_value(Some(&json!(true))), Some(1.0));
        assert_eq!(date_time_value(Some(&json!(1.5))), Some(1.0));
        assert_eq!(date_time_value(Some(&json!(8.64e15 + 1.0))), None);
        assert_eq!(date_time_value(Some(&json!({}))), None);
        assert_eq!(
            date_time_value(Some(&json!(["2023-10-31T21:59:10Z"]))),
            Some(1_698_789_550_000.0)
        );
    }

    /// A directory with an `az` script.
    #[cfg(unix)]
    struct FakeAz {
        dir: PathBuf,
    }

    #[cfg(unix)]
    impl FakeAz {
        /// `script` is the body of `az`; `None` leaves `az` out.
        fn install(script: Option<&str>) -> Self {
            use std::os::unix::fs::PermissionsExt as _;
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "foundry-azure-cli-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            if let Some(script) = script {
                let az = dir.join("az");
                std::fs::write(&az, format!("#!/bin/sh\n{script}\n")).unwrap();
                std::fs::set_permissions(&az, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            Self { dir }
        }

        /// The environment `az` is found in: the shell searches the child's
        /// `PATH`, which is the passed-in one.
        fn env(&self) -> Environment {
            Environment::new([("PATH", self.dir.as_os_str())])
        }
    }

    #[cfg(unix)]
    impl Drop for FakeAz {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[cfg(unix)]
    async fn get_token_with(script: Option<&str>) -> Result<Option<AccessToken>, CredentialError> {
        let az = FakeAz::install(script);
        AzureCliCredential::new(az.env())
            .get_token(&[SCOPE.to_owned()])
            .await
    }

    /// `:48-79` and `:192-205`: the command line, the `/bin` working
    /// directory, and `expires_on` winning; a warning on stderr does not
    /// matter once stdout parses.
    #[cfg(unix)]
    #[tokio::test]
    async fn az_success_matches_official_expires_on_seconds() {
        let token = get_token_with(Some(
            r#"printf 'WARNING: upgrade available\n' >&2
printf '{"accessToken":"%s","expires_on":1700000000,"expiresOn":"bad"}' "$(pwd -P)|$*""#,
        ))
        .await
        .unwrap()
        .unwrap();
        let bin = std::fs::canonicalize("/bin").unwrap();
        assert_eq!(
            token,
            AccessToken {
                token: format!(
                    "{}|account get-access-token --output json --resource https://cognitiveservices.azure.com",
                    bin.display()
                ),
                expires_on_timestamp: 1_700_000_000_000,
                refresh_after_timestamp: None,
            }
        );
    }

    /// `:206-216`: without `expires_on`, Azure CLI's local `expiresOn`.
    #[cfg(unix)]
    #[tokio::test]
    async fn az_success_matches_official_local_expires_on() {
        let token = get_token_with(Some(
            r#"printf '{"accessToken":"tok","expiresOn":"2023-10-31 21:59:10.123456"}'"#,
        ))
        .await
        .unwrap()
        .unwrap();
        assert_eq!(token.token, "tok");
        assert_eq!(
            token.expires_on_timestamp as f64,
            local_ms((2023, 10, 31), (21, 59, 10, 123))
        );
        assert_eq!(token.refresh_after_timestamp, None);
    }

    /// `:149-154`: the shell's "not found" on stderr.
    #[cfg(unix)]
    #[tokio::test]
    async fn az_missing_matches_official_not_installed_message() {
        assert_eq!(get_token_with(None).await, Err(unavailable(NOT_INSTALLED)));
    }

    /// `:147-148, 155-159`: `az login` on stderr, unless it is the
    /// `az login --scope` advice, which falls through to stdout and then to
    /// stderr as is.
    #[cfg(unix)]
    #[tokio::test]
    async fn az_logged_out_matches_official_login_message() {
        assert_eq!(
            get_token_with(Some(
                r#"printf "ERROR: Please run 'az login' to setup account.\n" >&2; exit 1"#
            ))
            .await,
            Err(unavailable(LOGIN))
        );
        assert_eq!(
            get_token_with(Some(
                r#"printf "Run 'az login --scope https://x/.default'\n" >&2; exit 1"#
            ))
            .await,
            Err(unavailable("Run 'az login --scope https://x/.default'\n"))
        );
    }

    /// `:160-171`: stdout that does not parse yields stderr verbatim, or the
    /// parse error when stderr is empty.
    #[cfg(unix)]
    #[tokio::test]
    async fn az_bad_stdout_matches_official_stderr_passthrough() {
        assert_eq!(
            get_token_with(Some(r#"printf 'not json'; printf 'boom\n' >&2; exit 2"#)).await,
            Err(unavailable("boom\n"))
        );
        let silent = get_token_with(Some("printf 'not json'")).await;
        assert!(
            matches!(silent, Err(CredentialError::Unavailable(_))),
            "{silent:?}"
        );
    }

    /// `:207-211`: an `expiresOn` that is not a date, which stderr overrides
    /// when there is any.
    #[cfg(unix)]
    #[tokio::test]
    async fn az_bad_expires_on_matches_official_unexpected_response() {
        assert_eq!(
            get_token_with(Some(
                r#"printf '{"accessToken":"tok","expiresOn":"not a date"}'"#
            ))
            .await,
            Err(unavailable(&format!(
                "{UNEXPECTED_RESPONSE} \"not a date\""
            )))
        );
        assert_eq!(
            get_token_with(Some(
                r#"printf '{"accessToken":"tok","expiresOn":"not a date"}'; printf 'note' >&2"#
            ))
            .await,
            Err(unavailable("note"))
        );
    }
}
