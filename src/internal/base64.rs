// Maps to: TS internal/utils/base64.ts
//
//! Small base64 helpers used by tests and helper code.
//!
//! The TypeScript SDK exposes `toBase64()` / `fromBase64()` from an internal
//! utilities module. Rust cannot overload strings, byte arrays, and `undefined`
//! like TS does, so this module provides explicit byte/string/optional helpers.

use crate::core::error::ApiError;

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Maps to: TS `toBase64(input)` for string inputs.
pub fn to_base64_str(input: &str) -> String {
    to_base64_bytes(input.as_bytes())
}

/// Maps to: TS `toBase64(input)` for Uint8Array/byte inputs.
pub fn to_base64_bytes(input: impl AsRef<[u8]>) -> String {
    let bytes = input.as_ref();
    if bytes.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }

        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Maps to TS `toBase64(undefined)`, which returns an empty string.
pub fn to_base64_optional(input: Option<&[u8]>) -> String {
    input.map(to_base64_bytes).unwrap_or_default()
}

/// Maps to: TS `fromBase64(input)`.
///
/// Accepts padded and unpadded standard base64 input, matching the TS utility's
/// Buffer/atob behavior for SDK-generated payloads.
pub fn from_base64(input: &str) -> Result<Vec<u8>, ApiError> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut values = Vec::with_capacity(input.len());
    let mut seen_padding = false;
    let mut padding_count = 0usize;

    for byte in input.bytes() {
        match byte {
            b'=' => {
                seen_padding = true;
                padding_count += 1;
                if padding_count > 2 {
                    return Err(ApiError::Sdk("invalid base64 padding".to_owned()));
                }
            }
            _ if seen_padding => {
                return Err(ApiError::Sdk(
                    "invalid base64 character after padding".to_owned(),
                ));
            }
            _ => values.push(base64_value(byte).ok_or_else(|| {
                ApiError::Sdk(format!("invalid base64 character: {}", byte as char))
            })?),
        }
    }

    if padding_count > 0 && input.len() % 4 != 0 {
        return Err(ApiError::Sdk("invalid base64 padding".to_owned()));
    }
    if values.len() % 4 == 1 {
        return Err(ApiError::Sdk("invalid base64 length".to_owned()));
    }

    let mut out = Vec::with_capacity(values.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for value in values {
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
            if bits > 0 {
                buffer &= (1 << bits) - 1;
            } else {
                buffer = 0;
            }
        }
    }

    // Leftover bits are legal for unpadded base64 only when they are zero
    // padding bits (2 or 4 bits). Six leftover bits would mean a single
    // dangling base64 digit, rejected above via len % 4 == 1.
    if bits >= 6 || (bits > 0 && buffer != 0) {
        return Err(ApiError::Sdk("invalid base64 trailing bits".to_owned()));
    }

    Ok(out)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
