// Maps to: TS internal/utils/uuid.ts
//
//! UUID generation helper.

use rand::RngExt;

/// Generate a random RFC 4122 version-4 UUID string.
///
/// Maps to TS `uuid4()`. The output is lowercase and hyphenated.
pub fn uuid4() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);

    // Version 4: high nibble of byte 6 is 0100.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    // RFC 4122 variant: high bits of byte 8 are 10.
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format_uuid4(bytes)
}

fn format_uuid4(bytes: [u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_uuid4(value: &str) -> bool {
        let bytes = value.as_bytes();
        if bytes.len() != 36 {
            return false;
        }
        for index in [8, 13, 18, 23] {
            if bytes[index] != b'-' {
                return false;
            }
        }
        if bytes[14] != b'4' {
            return false;
        }
        if !matches!(bytes[19], b'8' | b'9' | b'a' | b'b') {
            return false;
        }
        bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 8 | 13 | 18 | 23)
                || byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
        })
    }

    #[test]
    fn uuid4_generates_lowercase_rfc4122_v4_strings() {
        for _ in 0..32 {
            assert!(is_uuid4(&uuid4()));
        }
    }

    #[test]
    fn uuid4_is_random_enough_for_request_log_id_style_internal_use() {
        let a = uuid4();
        let b = uuid4();
        assert_ne!(a, b);
    }

    #[test]
    fn format_uuid4_sets_expected_string_layout_for_known_bytes() {
        let mut bytes = [0u8; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = i as u8;
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        assert_eq!(format_uuid4(bytes), "00010203-0405-4607-8809-0a0b0c0d0e0f");
    }
}
