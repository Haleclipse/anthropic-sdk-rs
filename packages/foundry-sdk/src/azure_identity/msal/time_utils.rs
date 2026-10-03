//! Maps to: `@azure/msal-common` 16.4.0 `src/utils/TimeUtils.ts`.

use crate::azure_identity::js::time_clip;
use crate::azure_identity::now_ms;

/// `:13-16` `nowSeconds`: `Math.round(Date.now() / 1000)`, a JS number. The
/// time is positive, where `f64::round` (half away from zero) and
/// `Math.round` (half towards +∞) agree.
pub(crate) fn now_seconds() -> f64 {
    (now_ms() as f64 / 1000.0).round()
}

/// `:31-36` `toDateFromSeconds`, as `getTime()`: a cached time is a
/// non-empty string, so always truthy; `new Date` truncates to whole
/// milliseconds and rejects the out-of-range (`TimeClip`), whose time is
/// `NaN`.
pub(crate) fn to_date_from_seconds(seconds: f64) -> f64 {
    time_clip(seconds * 1000.0).unwrap_or(f64::NAN)
}

/// `:42-49` `isTokenExpired`: `Number(expiresOn) || 0`, so `NaN` counts as 0.
pub(crate) fn is_token_expired(expires_on: f64, offset: f64) -> bool {
    let expiration_sec = if expires_on.is_nan() { 0.0 } else { expires_on };
    now_seconds() + offset > expiration_sec
}
