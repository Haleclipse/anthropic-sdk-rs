// Maps to: TS internal/decoders/line.ts
//
//! Incremental line decoder that splits a byte stream into lines, correctly
//! handling `\n`, `\r`, and `\r\n` endings as well as incomplete lines
//! spanning chunk boundaries and multi-byte UTF-8 codepoints split across
//! chunks.
//!
//! This is a faithful Rust port of the TypeScript `LineDecoder` class found
//! in `internal/decoders/line.ts`, which itself is a re-implementation of
//! httpx's `LineDecoder` in Python.

// ---------------------------------------------------------------------------
// NewlineMatch
// ---------------------------------------------------------------------------

/// Result of scanning a buffer for the next newline character.
struct NewlineMatch {
    /// Byte index of the newline character itself.
    preceding: usize,
    /// Byte index immediately *after* the newline character.
    index: usize,
    /// `true` when the matched character is `\r` (carriage return).
    carriage: bool,
}

// ---------------------------------------------------------------------------
// find_newline_index
// ---------------------------------------------------------------------------

/// Maps to: TS `findNewlineIndex()` in internal/decoders/line.ts
///
/// Scans `buffer` starting from `start_index` (or 0) for the next `\n` or
/// `\r` byte.  Returns `None` if no newline is found.
fn find_newline_index(buffer: &[u8], start_index: Option<usize>) -> Option<NewlineMatch> {
    let start = start_index.unwrap_or(0);
    for (i, &byte) in buffer.iter().enumerate().skip(start) {
        if byte == b'\n' {
            return Some(NewlineMatch {
                preceding: i,
                index: i + 1,
                carriage: false,
            });
        }
        if byte == b'\r' {
            return Some(NewlineMatch {
                preceding: i,
                index: i + 1,
                carriage: true,
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// find_double_newline_index
// ---------------------------------------------------------------------------

/// Maps to: TS `findDoubleNewlineIndex()` in internal/decoders/line.ts
///
/// Scans `buffer` for the first occurrence of `\n\n`, `\r\r`, or `\r\n\r\n`
/// and returns the index **after** the matched pattern.  Returns `None` if no
/// double-newline pattern is found.
pub(crate) fn find_double_newline_index(buffer: &[u8]) -> Option<usize> {
    if buffer.len() < 2 {
        return None;
    }

    let len = buffer.len();
    let mut i = 0;

    while i < len - 1 {
        // \n\n
        if buffer[i] == b'\n' && buffer[i + 1] == b'\n' {
            return Some(i + 2);
        }
        // \r\r
        if buffer[i] == b'\r' && buffer[i + 1] == b'\r' {
            return Some(i + 2);
        }
        // \r\n\r\n
        if buffer[i] == b'\r'
            && buffer[i + 1] == b'\n'
            && i + 3 < len
            && buffer[i + 2] == b'\r'
            && buffer[i + 3] == b'\n'
        {
            return Some(i + 4);
        }
        i += 1;
    }

    None
}

// ---------------------------------------------------------------------------
// LineDecoder
// ---------------------------------------------------------------------------

/// Maps to: TS `LineDecoder` class in internal/decoders/line.ts
///
/// Splits an incoming byte stream into lines, correctly handling `\n`, `\r`,
/// and `\r\n` endings as well as incomplete lines spanning chunk boundaries
/// and multi-byte UTF-8 codepoints split across chunks.
pub(crate) struct LineDecoder {
    buffer: Vec<u8>,
    /// Index (into `buffer`) of a pending `\r` whose resolution is deferred
    /// until we see the next character. `None` if no `\r` is pending.
    /// Maps to: TS `#carriageReturnIndex`
    carriage_return_index: Option<usize>,
}

impl LineDecoder {
    /// Creates a new, empty `LineDecoder`.
    pub(crate) fn new() -> Self {
        Self {
            buffer: Vec::new(),
            carriage_return_index: None,
        }
    }

    /// Maps to: TS `LineDecoder.decode()`
    ///
    /// Faithful port of the TS deferred-`\r` algorithm:
    /// 1. On `\r` with no pending CR -> record position, continue (don't emit yet)
    /// 2. On next newline when CR is pending:
    ///    - `\n` immediately after `\r` -> treat as `\r\n`, emit line up to `\r`
    ///    - Another `\r` or non-adjacent `\n` -> the pending `\r` was standalone,
    ///      emit line up to that `\r`, reprocess from there
    pub(crate) fn decode(&mut self, chunk: &[u8]) -> Vec<String> {
        if chunk.is_empty() {
            return Vec::new();
        }

        self.buffer.extend_from_slice(chunk);

        let mut lines: Vec<String> = Vec::new();

        loop {
            let result = find_newline_index(&self.buffer, self.carriage_return_index);
            let Some(nl) = result else {
                break;
            };

            // First time seeing a CR: defer it (we need to see the next byte
            // to know if it's \r\n or standalone \r). The loop will re-enter
            // and find_newline_index (starting from cr_idx) will re-find this
            // same \r, triggering the standalone-CR resolution path below.
            if nl.carriage && self.carriage_return_index.is_none() {
                self.carriage_return_index = Some(nl.preceding);
                continue;
            }

            // We have a deferred CR -- check what follows it.
            if let Some(cr_idx) = self.carriage_return_index {
                if nl.preceding == cr_idx && nl.carriage {
                    // Re-found the same \r. If it's the last byte in the
                    // buffer we cannot resolve yet -- need more data.
                    if cr_idx + 1 >= self.buffer.len() {
                        break;
                    }
                    // Check what's at cr_idx+1.
                    if self.buffer[cr_idx + 1] == b'\n' {
                        // \r\n pair. Emit line up to the \r, skip past \r\n.
                        let line = String::from_utf8_lossy(&self.buffer[..cr_idx]).into_owned();
                        lines.push(line);
                        self.buffer = self.buffer[cr_idx + 2..].to_vec();
                        self.carriage_return_index = None;
                        continue;
                    } else {
                        // Standalone \r (next byte is not \n).
                        let line = String::from_utf8_lossy(&self.buffer[..cr_idx]).into_owned();
                        lines.push(line);
                        self.buffer = self.buffer[cr_idx + 1..].to_vec();
                        self.carriage_return_index = None;
                        continue;
                    }
                } else if !nl.carriage && nl.preceding == cr_idx + 1 {
                    // \r\n pair: \n is immediately after the deferred \r.
                    let line = String::from_utf8_lossy(&self.buffer[..cr_idx]).into_owned();
                    lines.push(line);
                    self.buffer = self.buffer[nl.index..].to_vec();
                    self.carriage_return_index = None;
                    continue;
                } else {
                    // Something else follows the \r (another \r further away,
                    // or a \n that's not adjacent). The deferred \r was
                    // standalone.
                    let line = String::from_utf8_lossy(&self.buffer[..cr_idx]).into_owned();
                    lines.push(line);
                    self.buffer = self.buffer[cr_idx + 1..].to_vec();
                    self.carriage_return_index = None;
                    continue;
                }
            }

            // No deferred CR, and nl is a plain \n.
            let line = String::from_utf8_lossy(&self.buffer[..nl.preceding]).into_owned();
            lines.push(line);
            self.buffer = self.buffer[nl.index..].to_vec();
        }

        lines
    }

    /// Maps to: TS `LineDecoder.flush()`
    pub(crate) fn flush(&mut self) -> Vec<String> {
        if self.buffer.is_empty() && self.carriage_return_index.is_none() {
            return Vec::new();
        }
        self.decode(b"\n")
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- LineDecoder tests ----

    #[test]
    fn line_decoder_lf() {
        let mut dec = LineDecoder::new();
        let lines = dec.decode(b"hello\nworld\n");
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn line_decoder_crlf() {
        let mut dec = LineDecoder::new();
        let lines = dec.decode(b"hello\r\nworld\r\n");
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn line_decoder_cr() {
        // Standalone \r is treated as a line ending once resolved.
        // The first \r is deferred, then the next byte ('w') resolves it as
        // standalone, producing "hello". The second \r is at the end of the
        // buffer and stays deferred until flush().
        let mut dec = LineDecoder::new();
        let lines = dec.decode(b"hello\rworld\r");
        assert_eq!(lines, vec!["hello"]);
        let flushed = dec.flush();
        assert_eq!(flushed, vec!["world"]);
    }

    #[test]
    fn line_decoder_split_across_chunks() {
        let mut dec = LineDecoder::new();
        let l1 = dec.decode(b"hel");
        assert!(l1.is_empty());
        let l2 = dec.decode(b"lo\nwor");
        assert_eq!(l2, vec!["hello"]);
        let l3 = dec.decode(b"ld\n");
        assert_eq!(l3, vec!["world"]);
    }

    #[test]
    fn line_decoder_crlf_split_across_chunks() {
        let mut dec = LineDecoder::new();
        // \r at end of first chunk -- TS defers, so nothing emitted yet
        let l1 = dec.decode(b"hello\r");
        assert!(l1.is_empty());
        // \n at start of second chunk resolves the \r\n pair
        let l2 = dec.decode(b"\nworld\n");
        assert_eq!(l2, vec!["hello", "world"]);
    }

    #[test]
    fn line_decoder_flush_partial() {
        let mut dec = LineDecoder::new();
        let l1 = dec.decode(b"no newline");
        assert!(l1.is_empty());
        let flushed = dec.flush();
        assert_eq!(flushed, vec!["no newline"]);
    }

    #[test]
    fn line_decoder_empty_chunk() {
        let mut dec = LineDecoder::new();
        let lines = dec.decode(b"");
        assert!(lines.is_empty());
    }

    #[test]
    fn line_decoder_multibyte_utf8_across_chunks() {
        let mut dec = LineDecoder::new();
        // e-acute (U+00E9) is 0xC3 0xA9 in UTF-8.
        let l1 = dec.decode(&[b'c', b'a', b'f', 0xC3]);
        assert!(l1.is_empty()); // incomplete UTF-8, buffered
        let l2 = dec.decode(&[0xA9, b'\n']);
        assert_eq!(l2, vec!["caf\u{00E9}"]);
    }

    #[test]
    fn line_decoder_multiple_lines_no_trailing_newline() {
        let mut dec = LineDecoder::new();
        let lines = dec.decode(b"a\nb\nc");
        assert_eq!(lines, vec!["a", "b"]);
        let flushed = dec.flush();
        assert_eq!(flushed, vec!["c"]);
    }

    // ---- find_double_newline_index tests ----

    #[test]
    fn double_newline_lf() {
        assert_eq!(find_double_newline_index(b"abc\n\ndef"), Some(5));
    }

    #[test]
    fn double_newline_cr() {
        assert_eq!(find_double_newline_index(b"abc\r\rdef"), Some(5));
    }

    #[test]
    fn double_newline_crlf() {
        assert_eq!(find_double_newline_index(b"abc\r\n\r\ndef"), Some(7));
    }

    #[test]
    fn double_newline_none() {
        assert_eq!(find_double_newline_index(b"abc\ndef"), None);
    }

    #[test]
    fn double_newline_empty() {
        assert_eq!(find_double_newline_index(b""), None);
    }

    #[test]
    fn double_newline_single_byte() {
        assert_eq!(find_double_newline_index(b"\n"), None);
    }

    #[test]
    fn double_newline_at_start() {
        assert_eq!(find_double_newline_index(b"\n\nabc"), Some(2));
    }

    #[test]
    fn double_newline_only_crlf_not_lf_cr() {
        // \r\n alone is not a double newline
        assert_eq!(find_double_newline_index(b"abc\r\ndef"), None);
    }

    // ---- LineDecoder additional tests ----

    #[test]
    fn line_decoder_basic_two_chunks_concatenated() {
        // TS: decodeChunks(['foo', ' bar\nbaz']) == ['foo bar']
        let mut dec = LineDecoder::new();
        let l1 = dec.decode(b"foo");
        assert!(l1.is_empty());
        let l2 = dec.decode(b" bar\nbaz");
        assert_eq!(l2, vec!["foo bar"]);
    }

    #[test]
    fn line_decoder_basic_with_crlf() {
        // TS: decodeChunks(['foo', ' bar\r\nbaz']) without flush == ['foo bar']
        let mut dec = LineDecoder::new();
        dec.decode(b"foo");
        let lines = dec.decode(b" bar\r\nbaz");
        assert_eq!(lines, vec!["foo bar"]);

        // With flush: ['foo bar', 'baz']
        let mut dec2 = LineDecoder::new();
        dec2.decode(b"foo");
        let lines = dec2.decode(b" bar\r\nbaz");
        assert_eq!(lines, vec!["foo bar"]);
        let flushed = dec2.flush();
        assert_eq!(flushed, vec!["baz"]);
    }

    #[test]
    fn line_decoder_trailing_new_lines() {
        // TS: decodeChunks(['foo bar', 'baz\n', 'thing\n']) == ['foo barbaz', 'thing']
        let mut dec = LineDecoder::new();
        dec.decode(b"foo bar");
        let l2 = dec.decode(b"baz\n");
        assert_eq!(l2, vec!["foo barbaz"]);
        let l3 = dec.decode(b"thing\n");
        assert_eq!(l3, vec!["thing"]);
    }

    #[test]
    fn line_decoder_trailing_new_lines_with_crlf() {
        // Same as trailing new lines but with \r\n endings
        let mut dec = LineDecoder::new();
        dec.decode(b"foo bar");
        let l2 = dec.decode(b"baz\r\n");
        assert_eq!(l2, vec!["foo barbaz"]);
        let l3 = dec.decode(b"thing\r\n");
        assert_eq!(l3, vec!["thing"]);
    }

    #[test]
    fn line_decoder_escaped_new_lines() {
        // Literal backslash-n in content should not be treated as a newline
        let mut dec = LineDecoder::new();
        dec.decode(b"foo bar");
        let lines = dec.decode(b"\\nbaz\n");
        assert_eq!(lines, vec!["foo bar\\nbaz"]);
    }

    #[test]
    fn line_decoder_escaped_new_lines_with_cr() {
        // Literal backslash-r-backslash-n in content preserved
        let mut dec = LineDecoder::new();
        dec.decode(b"foo bar");
        let lines = dec.decode(b"\\r\\nbaz\n");
        assert_eq!(lines, vec!["foo bar\\r\\nbaz"]);
    }

    #[test]
    fn line_decoder_single_cr_cross_chunk() {
        // TS: decodeChunks(['foo\r', 'bar'], {flush:true}) == ['foo', 'bar']
        // The standalone \r is deferred at end of first chunk, then resolved
        // when the second chunk arrives (non-\n follows the deferred \r).
        let mut dec = LineDecoder::new();
        let l1 = dec.decode(b"foo\r");
        assert!(l1.is_empty()); // CR deferred (at end of buffer)
        let l2 = dec.decode(b"bar");
        assert_eq!(l2, vec!["foo"]); // CR resolved as standalone
        let flushed = dec.flush();
        assert_eq!(flushed, vec!["bar"]);
    }

    #[test]
    fn line_decoder_double_cr() {
        // Sub-scenario 1: ['foo\r', 'bar\r'] flush -> ['foo', 'bar']
        {
            let mut dec = LineDecoder::new();
            let mut all_lines = Vec::new();
            all_lines.extend(dec.decode(b"foo\r"));
            all_lines.extend(dec.decode(b"bar\r"));
            all_lines.extend(dec.flush());
            assert_eq!(all_lines, vec!["foo", "bar"]);
        }

        // Sub-scenario 2: ['foo\r', '\r', 'bar'] flush -> ['foo', '', 'bar']
        {
            let mut dec = LineDecoder::new();
            let mut all_lines = Vec::new();
            all_lines.extend(dec.decode(b"foo\r"));
            all_lines.extend(dec.decode(b"\r"));
            all_lines.extend(dec.decode(b"bar"));
            all_lines.extend(dec.flush());
            assert_eq!(all_lines, vec!["foo", "", "bar"]);
        }

        // Sub-scenario 3: without flush -> only 'foo'
        {
            let mut dec = LineDecoder::new();
            let mut all_lines = Vec::new();
            all_lines.extend(dec.decode(b"foo\r"));
            all_lines.extend(dec.decode(b"\r"));
            all_lines.extend(dec.decode(b"bar"));
            assert_eq!(all_lines, vec!["foo", ""]);
        }
    }

    #[test]
    fn line_decoder_double_cr_then_crlf() {
        // Sequence \r\r\r\n should produce three line breaks.
        // Also tests equivalent \n\n\n pattern.
        {
            let mut dec = LineDecoder::new();
            let mut all_lines = Vec::new();
            all_lines.extend(dec.decode(b"foo\r"));
            all_lines.extend(dec.decode(b"\r"));
            all_lines.extend(dec.decode(b"\r"));
            all_lines.extend(dec.decode(b"\n"));
            all_lines.extend(dec.decode(b"bar"));
            all_lines.extend(dec.decode(b"\n"));
            assert_eq!(all_lines, vec!["foo", "", "", "bar"]);
        }
        {
            let mut dec = LineDecoder::new();
            let mut all_lines = Vec::new();
            all_lines.extend(dec.decode(b"foo\n"));
            all_lines.extend(dec.decode(b"\n"));
            all_lines.extend(dec.decode(b"\n"));
            all_lines.extend(dec.decode(b"bar"));
            all_lines.extend(dec.decode(b"\n"));
            assert_eq!(all_lines, vec!["foo", "", "", "bar"]);
        }
    }

    #[test]
    fn line_decoder_double_newline() {
        // \n\n produces an empty line between two content lines.
        let mut dec = LineDecoder::new();
        let lines = dec.decode(b"foo\n\nbar\n");
        assert_eq!(lines, vec!["foo", "", "bar"]);
    }

    #[test]
    fn line_decoder_double_newline_split_patterns() {
        // 4 different chunk-split patterns all produce ['foo', '', 'bar']
        // Pattern 1: single chunk
        {
            let mut dec = LineDecoder::new();
            let lines = dec.decode(b"foo\n\nbar\n");
            assert_eq!(lines, vec!["foo", "", "bar"]);
        }
        // Pattern 2: split after first \n
        {
            let mut dec = LineDecoder::new();
            let mut all = Vec::new();
            all.extend(dec.decode(b"foo\n"));
            all.extend(dec.decode(b"\nbar\n"));
            assert_eq!(all, vec!["foo", "", "bar"]);
        }
        // Pattern 3: split between the two \n
        {
            let mut dec = LineDecoder::new();
            let mut all = Vec::new();
            all.extend(dec.decode(b"foo\n\n"));
            all.extend(dec.decode(b"bar\n"));
            assert_eq!(all, vec!["foo", "", "bar"]);
        }
        // Pattern 4: each chunk separate
        {
            let mut dec = LineDecoder::new();
            let mut all = Vec::new();
            all.extend(dec.decode(b"foo\n"));
            all.extend(dec.decode(b"\n"));
            all.extend(dec.decode(b"bar\n"));
            assert_eq!(all, vec!["foo", "", "bar"]);
        }
    }

    #[test]
    fn line_decoder_flushing_trailing_newlines() {
        let mut dec = LineDecoder::new();
        let lines = dec.decode(b"foo\n\nbar");
        assert_eq!(lines, vec!["foo", ""]);
        let flushed = dec.flush();
        assert_eq!(flushed, vec!["bar"]);
    }

    #[test]
    fn line_decoder_flushing_empty_buffer() {
        let mut dec = LineDecoder::new();
        let flushed = dec.flush();
        assert!(flushed.is_empty());
    }

    // ---- find_double_newline_index additional tests ----

    #[test]
    fn double_newline_lf_at_end() {
        assert_eq!(find_double_newline_index(b"foo\n\n"), Some(5));
    }

    #[test]
    fn double_newline_lf_only() {
        assert_eq!(find_double_newline_index(b"\n\n"), Some(2));
    }

    #[test]
    fn double_newline_cr_at_start() {
        assert_eq!(find_double_newline_index(b"\r\rbar"), Some(2));
    }

    #[test]
    fn double_newline_cr_at_end() {
        assert_eq!(find_double_newline_index(b"foo\r\r"), Some(5));
    }

    #[test]
    fn double_newline_cr_only() {
        assert_eq!(find_double_newline_index(b"\r\r"), Some(2));
    }

    #[test]
    fn double_newline_crlf_at_start() {
        assert_eq!(find_double_newline_index(b"\r\n\r\nbar"), Some(4));
    }

    #[test]
    fn double_newline_crlf_at_end() {
        assert_eq!(find_double_newline_index(b"foo\r\n\r\n"), Some(7));
    }

    #[test]
    fn double_newline_crlf_only() {
        assert_eq!(find_double_newline_index(b"\r\n\r\n"), Some(4));
    }

    #[test]
    fn double_newline_single_cr_returns_none() {
        // Single \r in the middle should not be a double newline
        assert_eq!(find_double_newline_index(b"foo\rbar"), None);
    }

    #[test]
    fn double_newline_incomplete_crlf_pattern() {
        // \r\n\r (3 of 4 bytes of \r\n\r\n) should return None
        assert_eq!(find_double_newline_index(b"foo\r\n\r"), None);
        // Just \r\n should return None
        assert_eq!(find_double_newline_index(b"foo\r\n"), None);
    }
}
