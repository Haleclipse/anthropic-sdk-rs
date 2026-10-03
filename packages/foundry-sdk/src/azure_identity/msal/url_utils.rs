//! Maps to: `@azure/msal-common` 16.4.0 `src/utils/UrlUtils.ts`.

use crate::azure_identity::js::encode_uri_component;

/// `UrlUtils.ts:94-102` `mapToQueryString`: `${key}=${encodeURIComponent(value)}`,
/// joined with `&`.
pub(crate) fn map_to_query_string(parameters: &[(&str, &str)]) -> String {
    parameters
        .iter()
        .map(|(key, value)| format!("{key}={}", encode_uri_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}
