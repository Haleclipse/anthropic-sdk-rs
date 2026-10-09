//! The JS built-ins the credential chain's npm packages lean on: `new Date`
//! for the expiries the sources return, and `String.prototype.trim`.
//!
//! `new Date(string)` is explicitly partial, as in the Foundry crate's
//! `azure_identity` port: the ES date-time string format, whose results are
//! the engine's. Any other string the engine's lenient parser accepts (RFC
//! 2822 dates, `+0000` offsets, a missing seconds field after `T`…) is an
//! invalid date here. An invalid date never expires in the chain's memoize,
//! as it does not in npm's.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Days, Local, TimeZone};
use serde_json::Value;

/// JS `\s` and the set `String.prototype.trim` strips: Unicode `White_Space`
/// plus U+FEFF, minus U+0085.
pub(crate) fn is_js_whitespace(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

/// `String.prototype.trim`.
pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// `new Date(text)` as a time.
pub(crate) fn date_from_string(text: &str) -> Option<SystemTime> {
    parse_iso_date(text).and_then(system_time)
}

/// `new Date(value)` for a JSON value: a string is parsed, a number is a time
/// in milliseconds, a boolean or `null` its `ToNumber` (`null` is the epoch);
/// an array or object is first turned into a string, which is no date here.
pub(crate) fn date_from_value(value: &Value) -> Option<SystemTime> {
    match value {
        Value::String(text) => date_from_string(text),
        Value::Number(number) => time_clip(number.as_f64()?).and_then(system_time),
        Value::Bool(value) => system_time(if *value { 1.0 } else { 0.0 }),
        Value::Null => system_time(0.0),
        Value::Array(_) | Value::Object(_) => None,
    }
}

/// ECMA-262 `TimeClip`: a time in milliseconds within ±8.64e15, truncated;
/// `None` for anything else (`NaN`, an invalid date).
fn time_clip(time: f64) -> Option<f64> {
    (time.is_finite() && time.abs() <= 8.64e15).then(|| time.trunc())
}

/// A JS time (milliseconds from the epoch) as a `SystemTime`, or `None` where
/// the platform's `SystemTime` cannot hold it.
fn system_time(milliseconds: f64) -> Option<SystemTime> {
    let offset = Duration::from_millis(milliseconds.abs() as u64);
    if milliseconds >= 0.0 {
        UNIX_EPOCH.checked_add(offset)
    } else {
        UNIX_EPOCH.checked_sub(offset)
    }
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
/// a skipped one takes the offset in force before the transition. Local time
/// is chrono's `Local`, which reads the OS `TZ`, not the chain's environment.
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

/// A byte cursor for the date format, which is ASCII.
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
