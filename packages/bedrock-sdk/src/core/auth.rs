// Maps to: TS packages/bedrock-sdk/src/core/auth.ts

use std::collections::HashMap;

use anthropic_sdk::core::error::ApiError;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::client::BedrockConfig;

/// AWS credential material used for SigV4 signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

impl AwsCredentials {
    /// Resolve credentials from explicit [`BedrockConfig`] fields.
    pub fn from_config(config: &BedrockConfig) -> Result<Self, ApiError> {
        let access_key_id = config.aws_access_key.clone().ok_or_else(|| {
            ApiError::Sdk(
                "Missing AWS access key. Supply `aws_access_key` or set AWS_ACCESS_KEY_ID."
                    .to_owned(),
            )
        })?;
        let secret_access_key = config.aws_secret_key.clone().ok_or_else(|| {
            ApiError::Sdk(
                "Missing AWS secret key. Supply `aws_secret_key` or set AWS_SECRET_ACCESS_KEY."
                    .to_owned(),
            )
        })?;

        Ok(Self {
            access_key_id,
            secret_access_key,
            session_token: config.aws_session_token.clone(),
        })
    }
}

/// Return AWS SigV4 auth headers for a prepared Bedrock Runtime request.
///
/// Maps to TS `packages/bedrock-sdk/src/core/auth.ts#getAuthHeaders`, but uses
/// explicit/static credentials instead of the JavaScript AWS provider chain.
/// The returned map contains lowercase header names suitable for merging into
/// request headers.
pub fn get_auth_headers(
    method: &str,
    url: &str,
    body: &[u8],
    config: &BedrockConfig,
) -> Result<HashMap<String, String>, ApiError> {
    let credentials = AwsCredentials::from_config(config)?;
    let now = time::OffsetDateTime::now_utc();
    let amz_date = format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    );
    get_auth_headers_at(method, url, body, config, &credentials, &amz_date)
}

fn get_auth_headers_at(
    method: &str,
    url: &str,
    body: &[u8],
    config: &BedrockConfig,
    credentials: &AwsCredentials,
    amz_date: &str,
) -> Result<HashMap<String, String>, ApiError> {
    if amz_date.len() < 8 {
        return Err(ApiError::Sdk(
            "SigV4 amz_date must start with YYYYMMDD".to_owned(),
        ));
    }
    let date = &amz_date[..8];
    let parsed = url::Url::parse(url)
        .map_err(|err| ApiError::Sdk(format!("invalid Bedrock request URL '{url}': {err}")))?;
    let host = parsed.host_str().ok_or_else(|| {
        ApiError::Sdk(format!("invalid Bedrock request URL '{url}': missing host"))
    })?;

    let mut headers = HashMap::new();
    headers.insert("host".to_owned(), host.to_owned());
    headers.insert("x-amz-date".to_owned(), amz_date.to_owned());
    headers.insert("x-amz-content-sha256".to_owned(), sha256_hex(body));
    if let Some(token) = &credentials.session_token {
        headers.insert("x-amz-security-token".to_owned(), token.clone());
    }

    let mut sorted_headers: Vec<_> = headers.iter().collect();
    sorted_headers.sort_by_key(|(name, _)| *name);
    let signed_headers = sorted_headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers = sorted_headers
        .iter()
        .map(|(name, value)| format!("{}:{}\n", name, normalize_header_value(value)))
        .collect::<String>();

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        canonical_uri(&parsed),
        canonical_query(&parsed),
        canonical_headers,
        signed_headers,
        headers["x-amz-content-sha256"],
    );
    let credential_scope = format!("{date}/{}/bedrock/aws4_request", config.aws_region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = signing_key(&credentials.secret_access_key, date, &config.aws_region);
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    headers.insert(
        "authorization".to_owned(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            credentials.access_key_id,
        ),
    );

    Ok(headers)
}

/// `@smithy/signature-v4` `getCanonicalPath` with `uriEscapePath`, which the
/// TS signer leaves on: the path's empty and `.` segments are dropped and `..`
/// segments resolved, then the whole path is escaped again with `/` kept. The
/// path is already percent-encoded, so `:` becomes `%3A` and an escape such as
/// `%2F` becomes `%252F`, as every service but S3 expects.
fn canonical_uri(url: &url::Url) -> String {
    let path = url.path();
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    let leading = if path.starts_with('/') { "/" } else { "" };
    let trailing = if !segments.is_empty() && path.ends_with('/') {
        "/"
    } else {
        ""
    };
    let normalized = format!("{leading}{}{trailing}", segments.join("/"));
    sigv4_encode(&normalized).replace("%2F", "/")
}

fn canonical_query(url: &url::Url) -> String {
    let Some(query) = url.query() else {
        return String::new();
    };
    let mut pairs: Vec<_> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (sigv4_encode(&key), sigv4_encode(&value)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn sigv4_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn normalize_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn signing_key(secret: &str, date: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"bedrock");
    hmac_sha256(&k_service, b"aws4_request")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paths as `new URL(url).pathname` gives them, and the signatures
    /// `@smithy/signature-v4` 3.1.2 (the TS SDK's signer) makes for a `POST`
    /// of `{}` to them, signed at 2026-01-02T03:04:05Z with the AWS
    /// documentation's example keys.
    const TS_SIGNATURES: [(&str, &str); 6] = [
        (
            "/model/claude-test/invoke",
            "8cb82b4d27d9bedc825a4b568135e65299cf7f714857c1ec250c17e0ae7dd13b",
        ),
        (
            "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke",
            "134f0a67da5949ec6492d6bd8091088188469978baa163368e20d17a6eb5c92f",
        ),
        (
            "/model/us.anthropic.claude-sonnet-4-6%3A0/count-tokens",
            "e1bab66793295fc333fb1380fbf2f72aa5b2b60135c2b092d72452f4e8188a63",
        ),
        (
            "/model/arn:aws:bedrock:us-east-2:1234:inference-profile%2Fus.anthropic.claude-3-7-sonnet-20250219-v1:0/invoke",
            "0810715906d515fcd70693ce86b26fc36ac893d68e5b9c2e4c2acc5c623f9c26",
        ),
        (
            "/model/provider%2Fmodel%20with%20snowman%20%E2%98%83%25/invoke",
            "b9712169f997fafb76d4a01ee60057dda7254ee1f5d4f308fa25d0a718f852ae",
        ),
        (
            "/model//a/invoke/",
            "120c42ea95c448a0ec2bce1917d50ccb5a3bcddec5430d4ab8cdd85cc391be7d",
        ),
    ];

    #[test]
    fn signatures_match_the_ts_signer() {
        let config = BedrockConfig {
            aws_region: "us-east-1".to_owned(),
            aws_access_key: Some("AKIDEXAMPLE".to_owned()),
            aws_secret_key: Some("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned()),
            aws_session_token: None,
            base_url: None,
            credential_provider: None,
            skip_auth: false,
        };
        let credentials = AwsCredentials::from_config(&config).unwrap();
        for (path, signature) in TS_SIGNATURES {
            let url = format!("https://bedrock-runtime.us-east-1.amazonaws.com{path}");
            let headers = get_auth_headers_at(
                "POST",
                &url,
                b"{}",
                &config,
                &credentials,
                "20260102T030405Z",
            )
            .unwrap();
            assert_eq!(
                headers["authorization"],
                format!(
                    "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20260102/us-east-1/bedrock/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}"
                ),
                "{path}"
            );
        }
    }

    /// `getCanonicalPath`'s results for the same paths.
    #[test]
    fn the_canonical_path_is_normalized_and_escaped_again() {
        let canonical = |path: &str| {
            canonical_uri(&url::Url::parse(&format!("https://bedrock.local{path}")).unwrap())
        };
        assert_eq!(
            canonical("/model/anthropic.claude-3-5-sonnet-20241022-v2:0/invoke"),
            "/model/anthropic.claude-3-5-sonnet-20241022-v2%3A0/invoke"
        );
        assert_eq!(
            canonical("/model/provider%2Fmodel%20with%20snowman%20%E2%98%83%25/invoke"),
            "/model/provider%252Fmodel%2520with%2520snowman%2520%25E2%2598%2583%2525/invoke"
        );
        assert_eq!(canonical("/model//a/invoke/"), "/model/a/invoke/");
        assert_eq!(canonical("/"), "/");
    }
}
