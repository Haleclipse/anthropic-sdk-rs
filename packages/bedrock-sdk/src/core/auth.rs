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

fn canonical_uri(url: &url::Url) -> String {
    let path = url.path();
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
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
