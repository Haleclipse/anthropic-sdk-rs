// Maps to: TS internal/utils/path.ts
//
//! Path parameter encoding helpers.

use crate::core::error::ApiError;

/// Percent-encode one URI path parameter using the same safe character set as
/// the TS SDK's `encodeURIPath` helper.
///
/// Safe characters are RFC 3986 path `pchar` minus `%` and `/`:
/// alphanumerics, `-._~!$&'()*+,;=:@`.
pub fn encode_uri_path(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
            | b':'
            | b'@' => out.push(*byte as char),
            other => {
                out.push('%');
                out.push(hex_digit(other >> 4));
                out.push(hex_digit(other & 0x0f));
            }
        }
    }
    out
}

/// Encode a path parameter and reject values that would create unsafe `.` or
/// `..` path segments, matching the TS path tag's safety check for the common
/// generated-resource case of one parameter forming a whole segment.
pub fn encode_path_param(value: &str) -> Result<String, ApiError> {
    let encoded = encode_uri_path(value);
    if encoded == "." || encoded == ".." || encoded.eq_ignore_ascii_case("%2e") {
        return Err(ApiError::Sdk(format!(
            "Value \"{value}\" can't be safely passed as a path parameter"
        )));
    }
    Ok(encoded)
}

/// Render a TS-style path-template expression from static path pieces and
/// already-stringified parameters.
///
/// This maps to `createPathTagFunction(encodeURIPath)` / `path` in
/// `src/internal/utils/path.ts`: path parameters before a `?`/`#` use the
/// URI-path safe set, parameters after the query/hash boundary use ordinary URI
/// component encoding, and the final path is rejected if interpolation created
/// a `.` or `..` path segment (including `%2e` spellings).
pub fn create_path(statics: &[&str], params: &[&str]) -> Result<String, ApiError> {
    create_path_with_encoder(statics, params, encode_uri_path)
}

/// Lower-level equivalent of TS `createPathTagFunction(pathEncoder)`.
///
/// Rust does not have JavaScript template tags or dynamic `unknown` values, so
/// callers pass the template's static string pieces and parameter strings
/// directly. The `path_encoder` is used only for parameters that appear before
/// a query/hash boundary, matching TS `postPath` behavior.
pub fn create_path_with_encoder<F>(
    statics: &[&str],
    params: &[&str],
    mut path_encoder: F,
) -> Result<String, ApiError>
where
    F: FnMut(&str) -> String,
{
    if statics.len() != params.len() + 1 {
        return Err(ApiError::Sdk(format!(
            "path template expected statics.len() == params.len() + 1 but received {} statics and {} params",
            statics.len(),
            params.len()
        )));
    }

    if params.is_empty() {
        return Ok(statics.first().copied().unwrap_or_default().to_owned());
    }

    let mut post_path = false;
    let mut path = String::new();
    for (index, current) in statics.iter().enumerate() {
        if current.contains('?') || current.contains('#') {
            post_path = true;
        }
        path.push_str(current);
        if let Some(value) = params.get(index) {
            if post_path {
                path.push_str(&encode_uri_component(value));
            } else {
                path.push_str(&path_encoder(value));
            }
        }
    }

    validate_path_segments(&path)?;
    Ok(path)
}

/// Validate that `path` contains no interpolated special path segments.
///
/// This mirrors TS `invalidSegmentPattern = /(?<=^|\/)(?:\.|%2e){1,2}(?=\/|$)/gi`
/// against the path-only prefix before `?` or `#`.
pub fn validate_path_segments(path: &str) -> Result<(), ApiError> {
    let path_only_end = path.find(['?', '#']).unwrap_or(path.len());
    let path_only = &path[..path_only_end];
    let invalid_segments = find_invalid_segments(path_only);

    if invalid_segments.is_empty() {
        return Ok(());
    }

    let mut last_end = 0;
    let mut underline = String::new();
    for segment in &invalid_segments {
        underline.push_str(&" ".repeat(segment.start.saturating_sub(last_end)));
        underline.push_str(&"^".repeat(segment.length));
        last_end = segment.start + segment.length;
    }

    let errors = invalid_segments
        .iter()
        .map(|segment| {
            format!(
                "Value \"{}\" can't be safely passed as a path parameter",
                &path_only[segment.start..segment.start + segment.length]
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Err(ApiError::Sdk(format!(
        "Path parameters result in path with invalid segments:\n{errors}\n{path}\n{underline}"
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InvalidSegment {
    start: usize,
    length: usize,
}

fn find_invalid_segments(path_only: &str) -> Vec<InvalidSegment> {
    let mut out = Vec::new();
    let mut segment_start = 0;
    for (index, byte) in path_only.bytes().enumerate() {
        if byte == b'/' {
            push_invalid_segment(path_only, segment_start, index, &mut out);
            segment_start = index + 1;
        }
    }
    push_invalid_segment(path_only, segment_start, path_only.len(), &mut out);
    out
}

fn push_invalid_segment(path_only: &str, start: usize, end: usize, out: &mut Vec<InvalidSegment>) {
    if start > end || end > path_only.len() {
        return;
    }
    let segment = &path_only[start..end];
    if is_invalid_dot_segment(segment) {
        out.push(InvalidSegment {
            start,
            length: end - start,
        });
    }
}

fn is_invalid_dot_segment(segment: &str) -> bool {
    let mut rest = segment;
    let mut atoms = 0;
    while !rest.is_empty() && atoms < 2 {
        if let Some(stripped) = rest.strip_prefix('.') {
            rest = stripped;
            atoms += 1;
        } else if rest.len() >= 3
            && rest.as_bytes()[0] == b'%'
            && rest.as_bytes()[1] == b'2'
            && rest.as_bytes()[2].eq_ignore_ascii_case(&b'e')
        {
            rest = &rest[3..];
            atoms += 1;
        } else {
            return false;
        }
    }
    rest.is_empty() && (1..=2).contains(&atoms)
}

fn encode_uri_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match *byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(*byte as char),
            other => {
                out.push('%');
                out.push(hex_digit(other >> 4));
                out.push(hex_digit(other & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'A' + (n - 10)) as char,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_uri_path_encodes_slash_space_percent_and_unicode() {
        assert_eq!(
            encode_uri_path("model/with space%☃"),
            "model%2Fwith%20space%25%E2%98%83"
        );
    }

    #[test]
    fn encode_uri_path_leaves_safe_path_chars() {
        assert_eq!(
            encode_uri_path("abc-._~!$&'()*+,;=:@123"),
            "abc-._~!$&'()*+,;=:@123"
        );
    }

    #[test]
    fn encode_uri_path_matches_ts_encode_uri_path_invariants_for_ascii_and_unicode() {
        let mut cases = vec![String::new(), "å".to_owned(), "😃".to_owned()];
        cases.extend((0..0x7f_u8).map(|byte| (byte as char).to_string()));

        for param in cases {
            let encoded = encode_uri_path(&param);
            let naive_encoded = encode_uri_component(&param);
            assert!(
                naive_encoded.len() >= encoded.len(),
                "encode_uri_path encoded more than encodeURIComponent equivalent for {param:?}: {encoded} vs {naive_encoded}"
            );
            assert_eq!(percent_decode_for_test(&encoded), param);
        }

        assert_eq!(encode_uri_path(":"), ":");
        assert_eq!(encode_uri_path("@"), "@");
    }

    fn percent_decode_for_test(encoded: &str) -> String {
        let bytes = encoded.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len() {
                let hi = from_hex_for_test(bytes[index + 1]);
                let lo = from_hex_for_test(bytes[index + 2]);
                out.push((hi << 4) | lo);
                index += 3;
            } else {
                out.push(bytes[index]);
                index += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn from_hex_for_test(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'A'..=b'F' => byte - b'A' + 10,
            b'a'..=b'f' => byte - b'a' + 10,
            other => panic!("invalid hex byte in percent encoding: {other}"),
        }
    }

    #[test]
    fn encode_path_param_rejects_dot_segments() {
        assert!(encode_path_param(".").is_err());
        assert!(encode_path_param("..").is_err());
    }

    #[test]
    fn create_path_raw_encoder_matches_ts_path_test_permutation_invariant() {
        let test_params = [
            "", ".", "..", "x", "%2e", "%2E", "%2e%2e", "%2E%2e", "%2e%2E", "%2E%2E",
        ];
        let test_cases: Vec<Vec<&str>> = vec![
            vec!["/path_params/", "/a"],
            vec!["/path_params/", "/"],
            vec!["/path_params/", ""],
            vec!["", "/a"],
            vec!["", "/"],
            vec!["", ""],
            vec!["a"],
            vec![""],
            vec!["/path_params/", ":initiate"],
            vec!["/path_params/", ".json"],
            vec!["/path_params/", "?beta=true"],
            vec!["/path_params/", ".?beta=true"],
            vec!["/path_params/", "/", "/download"],
            vec!["/path_params/", "-", "/download"],
            vec!["/path_params/", "", "/download"],
            vec!["/path_params/", ".", "/download"],
            vec!["/path_params/", "..", "/download"],
            vec!["/plain/path"],
        ];

        for statics in test_cases {
            let params_len = statics.len().saturating_sub(1);
            for params in param_permutations(&test_params, params_len) {
                let string_raw = render_template(&statics, &params);
                let plain_string = render_template(
                    &statics
                        .iter()
                        .map(|part| part.replace('.', "x"))
                        .collect::<Vec<_>>()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    &params
                        .iter()
                        .map(|param| "X".repeat(param.len()))
                        .collect::<Vec<_>>()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                );
                let normalized_raw_len = normalized_href_len(&string_raw);
                let normalized_plain_len = normalized_href_len(&plain_string);

                let result = create_path_with_encoder(&statics, &params, |s| s.to_owned());
                match result {
                    Ok(path) => {
                        assert_eq!(path, string_raw, "statics={statics:?} params={params:?}");
                        assert_eq!(
                            normalized_raw_len, normalized_plain_len,
                            "successful TS path tag cases should not normalize away dot segments; statics={statics:?} params={params:?}"
                        );
                    }
                    Err(error) => {
                        let message = error.to_string();
                        assert!(
                            message.contains("Path parameters result in path with invalid segment"),
                            "unexpected error for statics={statics:?} params={params:?}: {message}"
                        );
                        assert_ne!(
                            normalized_raw_len, normalized_plain_len,
                            "failing TS path tag cases should correspond to URL dot-segment normalization; statics={statics:?} params={params:?} message={message}"
                        );
                    }
                }
            }
        }
    }

    fn param_permutations<'a>(params: &'a [&'a str], len: usize) -> Vec<Vec<&'a str>> {
        match len {
            0 => vec![Vec::new()],
            1 => params.iter().map(|param| vec![*param]).collect(),
            _ => {
                let rest = param_permutations(params, len - 1);
                params
                    .iter()
                    .flat_map(|param| {
                        rest.iter().map(move |tail| {
                            let mut permutation = Vec::with_capacity(len);
                            permutation.push(*param);
                            permutation.extend(tail.iter().copied());
                            permutation
                        })
                    })
                    .collect()
            }
        }
    }

    fn render_template(statics: &[&str], params: &[&str]) -> String {
        let mut out = String::new();
        for (index, static_part) in statics.iter().enumerate() {
            out.push_str(static_part);
            if let Some(param) = params.get(index) {
                out.push_str(param);
            }
        }
        out
    }

    fn normalized_href_len(path: &str) -> usize {
        let base = url::Url::parse("https://example.com").unwrap();
        base.join(path).unwrap().as_str().len()
    }

    #[test]
    fn create_path_matches_ts_path_tag_for_representative_segments() {
        assert_eq!(
            create_path(&["/path_params/", "/a"], &["x"]).unwrap(),
            "/path_params/x/a"
        );
        assert_eq!(
            create_path(&["/path_params/", "?beta=true"], &[""]).unwrap(),
            "/path_params/?beta=true"
        );
        assert_eq!(
            create_path(&["/path_params/", ":initiate"], &["."]).unwrap(),
            "/path_params/.:initiate"
        );
        assert_eq!(
            create_path(&["/path_params/", "-", "/download"], &["%2E", ".."]).unwrap(),
            "/path_params/%252E-../download"
        );
        assert_eq!(create_path(&["", ""], &["%2E"]).unwrap(), "%252E");
    }

    #[test]
    fn create_path_rejects_static_and_param_combinations_that_form_dot_segments_like_ts() {
        let err =
            create_path_with_encoder(&["/path_params/", ".?beta=true"], &[""], |s| s.to_owned())
                .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Path parameters result in path with invalid segments"));
        assert!(message.contains("Value \".\" can't be safely passed as a path parameter"));
        assert!(message.contains("/path_params/.?beta=true"));
        assert!(message.contains("             ^"));

        let err = create_path_with_encoder(
            &["/path_params/", "/", "/download"],
            &["%2E%2e", "%2e"],
            |s| s.to_owned(),
        )
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Value \"%2E%2e\" can't be safely passed as a path parameter"));
        assert!(message.contains("Value \"%2e\" can't be safely passed as a path parameter"));
        assert!(message.contains("/path_params/%2E%2e/%2e/download"));
        assert!(message.contains("             ^^^^^^ ^^^"));
    }

    #[test]
    fn create_path_uses_encode_uri_component_after_query_or_hash_like_ts_post_path() {
        assert_eq!(
            create_path(&["/v1/messages?", "="], &["a:b@c d"]).unwrap(),
            "/v1/messages?a%3Ab%40c%20d="
        );
        assert_eq!(
            create_path(&["/v1/messages#", ""], &["a/b:c"]).unwrap(),
            "/v1/messages#a%2Fb%3Ac"
        );
    }

    #[test]
    fn validate_path_segments_handles_percent_dot_spellings_and_unicode_safely() {
        assert!(validate_path_segments("/safe/%252e/😃").is_ok());
        assert!(validate_path_segments("/safe/%2e%2E").is_err());
        assert!(validate_path_segments("/safe/.%2e?ok=true").is_err());
        assert!(validate_path_segments("/safe/.../file").is_ok());
        assert!(validate_path_segments("/safe/%2e./file").is_err());
    }
}
