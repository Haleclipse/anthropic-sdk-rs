// Mirrors TS SDK tests/uploads.test.ts with Rust Uploadable/to_file equivalents.

use std::fs;
use std::path::PathBuf;

use anthropic_sdk::core::uploads::{Uploadable, to_file};

fn temp_upload_path(name: &str, contents: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "anthropic-sdk-rs-upload-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp upload dir");
    let path = dir.join(name);
    fs::write(&path, contents).expect("write temp upload file");
    path
}

#[test]
fn to_file_accepts_uploadable_without_copying_semantics() {
    let input =
        Uploadable::from_bytes_with_mime(b"foo".to_vec(), "input.jsonl", "application/jsonl");
    let file = to_file(input.clone());
    assert_eq!(file, input);
    assert_eq!(file.filename(true), "input.jsonl");
}

#[test]
fn to_file_from_path_extracts_file_name_like_ts_response_or_read_stream_names() {
    let path = temp_upload_path("audio.mp3", b"audio-bytes");
    let file = to_file(path.clone());
    assert_eq!(file.filename(true), "audio.mp3");
    assert_eq!(file.filename(false), path.to_string_lossy());
}

#[test]
fn to_file_from_str_is_a_rust_path_conversion() {
    let file = to_file("/tmp/my/input.jsonl");
    assert_eq!(file, Uploadable::from_path("/tmp/my/input.jsonl"));
    assert_eq!(file.filename(true), "input.jsonl");
}

#[test]
fn bytes_uploadable_allows_overriding_name_and_type_like_file_props() {
    let file = Uploadable::from_bytes_with_mime(
        b"foo".to_vec(),
        "my-uploads.test.ts",
        "application/typescript",
    );
    assert_eq!(file.filename(true), "my-uploads.test.ts");
    file.to_part(true)
        .expect("valid MIME type should build multipart part");
}

#[test]
fn uploadable_reports_invalid_mime_type_helpfully() {
    let file = Uploadable::from_bytes_with_mime(b"foo".to_vec(), "input.jsonl", "not a mime");
    let err = file.to_part(true).unwrap_err();
    assert!(err.to_string().contains("invalid upload MIME type"));
}

#[test]
fn uploadable_path_reads_existing_file_into_part() {
    let path = temp_upload_path("upload.txt", b"hello world");
    let file = Uploadable::from_path(&path);
    file.to_part(true)
        .expect("existing path should become multipart part");
}

#[test]
fn uploadable_path_read_error_is_helpful() {
    let missing = std::env::temp_dir().join(format!(
        "anthropic-sdk-rs-missing-upload-{}",
        std::process::id()
    ));
    let file = Uploadable::from_path(&missing);
    let err = file.to_part(true).unwrap_err();
    assert!(err.to_string().contains("failed to read upload file"));
    assert!(
        err.to_string()
            .contains(&missing.to_string_lossy().to_string())
    );
}
