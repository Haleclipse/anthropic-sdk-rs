// Maps to: TS internal/utils/bytes.ts
//
//! Byte/UTF-8 helpers used by stream and upload utilities.

/// Concatenate byte slices into one contiguous byte vector.
///
/// Maps to TS `concatBytes(buffers)`.
pub fn concat_bytes(buffers: &[impl AsRef<[u8]>]) -> Vec<u8> {
    let total_len = buffers.iter().map(|buffer| buffer.as_ref().len()).sum();
    let mut out = Vec::with_capacity(total_len);
    for buffer in buffers {
        out.extend_from_slice(buffer.as_ref());
    }
    out
}

/// TS-style camelCase alias for [`concat_bytes`].
#[allow(non_snake_case)]
pub fn concatBytes(buffers: &[impl AsRef<[u8]>]) -> Vec<u8> {
    concat_bytes(buffers)
}

/// Encode a string as UTF-8 bytes.
///
/// Maps to TS `encodeUTF8(str)`.
pub fn encode_utf8(value: &str) -> Vec<u8> {
    value.as_bytes().to_vec()
}

/// TS-style camelCase alias for [`encode_utf8`].
#[allow(non_snake_case)]
pub fn encodeUTF8(value: &str) -> Vec<u8> {
    encode_utf8(value)
}

/// Decode UTF-8 bytes using replacement characters for invalid sequences.
///
/// This mirrors the default non-fatal `TextDecoder` behavior used by TS
/// `decodeUTF8(bytes)`.
pub fn decode_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// TS-style camelCase alias for [`decode_utf8`].
#[allow(non_snake_case)]
pub fn decodeUTF8(bytes: &[u8]) -> String {
    decode_utf8(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_bytes_matches_ts_order_and_empty_cases() {
        let chunks: [&[u8]; 4] = [b"ab", b"", b"cd", &[0xff]];
        assert_eq!(concat_bytes(&chunks), vec![b'a', b'b', b'c', b'd', 0xff]);
        let empty: [&[u8]; 0] = [];
        assert_eq!(concatBytes(&empty), Vec::<u8>::new());
    }

    #[test]
    fn encode_and_decode_utf8_match_text_encoder_decoder_defaults() {
        let text = "hello 😃 å";
        let bytes = encode_utf8(text);
        assert_eq!(bytes, text.as_bytes());
        assert_eq!(decode_utf8(&bytes), text);
        assert_eq!(encodeUTF8(text), bytes);
        assert_eq!(decodeUTF8(&bytes), text);
    }

    #[test]
    fn decode_utf8_replaces_invalid_sequences_like_nonfatal_text_decoder() {
        assert_eq!(decode_utf8(&[0xf0, 0x28, 0x8c, 0x28]), "�(�(");
    }
}
