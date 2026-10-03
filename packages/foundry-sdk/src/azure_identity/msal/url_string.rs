//! Maps to: `@azure/msal-common` 16.4.0 `src/url/UrlString.ts`, as plain
//! functions over the URL text.

use super::auth_error::MsalError;

/// `new UrlString(url).urlString` (`UrlString.ts:24-60`): lowercased, a
/// trailing `?` or `?/` dropped and a trailing `/` ensured, unless the URL
/// holds a `#`. Every caller has a non-empty URL, which the constructor
/// would reject (`urlEmptyError`); an empty one is returned as it is.
pub(crate) fn url_string(url: &str) -> String {
    if url.contains('#') || url.is_empty() {
        return url.to_owned();
    }
    let mut lower = url.to_lowercase();
    if lower.ends_with('?') {
        lower.pop();
    } else if lower.ends_with("?/") {
        lower.truncate(lower.len() - 2);
    }
    if !lower.ends_with('/') {
        lower.push('/');
    }
    lower
}

/// The parts of `UrlString.getUrlComponents()` (`UrlString.ts:135-170`) read
/// here, from `^(([^:/?#]+):)?(//([^/?#]*))?([^?#]*)(\?([^#]*))?(#(.*))?`.
pub(crate) struct UrlComponents<'a> {
    /// With its `:`.
    pub protocol: Option<&'a str>,
    pub host_name_and_port: Option<&'a str>,
    /// The non-empty segments of the path.
    pub path_segments: Vec<&'a str>,
}

pub(crate) fn url_components(url: &str) -> UrlComponents<'_> {
    let mut rest = url;
    let protocol = match rest.find([':', '/', '?', '#']) {
        Some(index) if index > 0 && rest.as_bytes()[index] == b':' => {
            let protocol = &rest[..=index];
            rest = &rest[index + 1..];
            Some(protocol)
        }
        _ => None,
    };
    let mut host_name_and_port = None;
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        host_name_and_port = Some(&after[..end]);
        rest = &after[end..];
    }
    let path_end = rest.find(['?', '#']).unwrap_or(rest.len());
    UrlComponents {
        protocol,
        host_name_and_port,
        path_segments: rest[..path_end]
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect(),
    }
}

/// `UrlString.validateAsUri()` (`UrlString.ts:65-91`).
pub(crate) fn validate_as_uri(url: &str) -> Result<UrlComponents<'_>, MsalError> {
    let components = url_components(url);
    if components.host_name_and_port.is_none_or(str::is_empty) {
        return Err(MsalError::client_configuration("url_parse_error"));
    }
    if !components
        .protocol
        .is_some_and(|protocol| protocol.to_lowercase() == "https:")
    {
        return Err(MsalError::client_configuration("authority_uri_insecure"));
    }
    Ok(components)
}

/// `UrlString.getDomainFromUrl` (`UrlString.ts:172-184`):
/// `^([^:/?#]+://)?([^/?#]*)`.
pub(crate) fn domain_from_url(url: &str) -> &str {
    let rest = match url.find("://") {
        Some(index) if index > 0 && !url[..index].contains([':', '/', '?', '#']) => {
            &url[index + 3..]
        }
        _ => url,
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    &rest[..end]
}

/// `UrlString.ts:99-107` `appendQueryString`.
pub(crate) fn append_query_string(url: &str, query: &str) -> String {
    if query.is_empty() {
        url.to_owned()
    } else if url.contains('?') {
        format!("{url}&{query}")
    } else {
        format!("{url}?{query}")
    }
}
