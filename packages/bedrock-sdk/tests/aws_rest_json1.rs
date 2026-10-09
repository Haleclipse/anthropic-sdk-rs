use anthropic_sdk_bedrock::AWS_restJson1::{ResponseStream, de_ResponseStream};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use bytes::Bytes;
use futures::{StreamExt, stream};

#[tokio::test]
async fn de_response_stream_decodes_chunk_payload_part_like_ts_aws_rest_json1() {
    let inner = br#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#;
    let wrapper = serde_json::json!({"bytes": BASE64_STANDARD.encode(inner)});
    let body = bedrock_event_stream_body(vec![bedrock_event_stream_frame(
        "chunk",
        &serde_json::to_vec(&wrapper).unwrap(),
    )]);

    let chunks = stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(body))]);
    let mut decoded = de_ResponseStream(chunks);

    let event = decoded.next().await.unwrap().unwrap();
    match event {
        ResponseStream::Chunk { chunk } => assert_eq!(chunk.bytes, inner),
        other => panic!("expected chunk event, got {other:?}"),
    }
    assert!(decoded.next().await.is_none());
}

#[tokio::test]
async fn de_response_stream_decodes_model_stream_error_exception_like_ts_aws_rest_json1() {
    let body = bedrock_event_stream_body(vec![bedrock_event_stream_frame(
        "modelStreamErrorException",
        &serde_json::to_vec(&serde_json::json!({
            "message": "stream failed",
            "originalMessage": "upstream failed",
            "originalStatusCode": 424
        }))
        .unwrap(),
    )]);

    let chunks = stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(body))]);
    let mut decoded = de_ResponseStream(chunks);

    let event = decoded.next().await.unwrap().unwrap();
    match event {
        ResponseStream::ModelStreamErrorException {
            model_stream_error_exception,
        } => {
            assert_eq!(
                model_stream_error_exception.message.as_deref(),
                Some("stream failed")
            );
            assert_eq!(
                model_stream_error_exception.original_message.as_deref(),
                Some("upstream failed")
            );
            assert_eq!(model_stream_error_exception.original_status_code, Some(424));
        }
        other => panic!("expected model stream error exception, got {other:?}"),
    }
    assert!(decoded.next().await.is_none());
}

#[tokio::test]
async fn de_response_stream_handles_split_frames_and_unknown_events() {
    let frame = bedrock_event_stream_frame("futureEvent", br#"{"ok":true}"#);
    let split_at = 5;
    let chunks = stream::iter(vec![
        Ok::<_, std::io::Error>(Bytes::copy_from_slice(&frame[..split_at])),
        Ok::<_, std::io::Error>(Bytes::copy_from_slice(&frame[split_at..])),
    ]);
    let mut decoded = de_ResponseStream(chunks);

    let event = decoded.next().await.unwrap().unwrap();
    match event {
        ResponseStream::Unknown {
            event_type,
            payload,
        } => {
            assert_eq!(event_type.as_deref(), Some("futureEvent"));
            assert_eq!(payload, br#"{"ok":true}"#);
        }
        other => panic!("expected unknown event, got {other:?}"),
    }
    assert!(decoded.next().await.is_none());
}

#[tokio::test]
async fn de_response_stream_rejects_invalid_event_stream_crc_like_smithy() {
    let mut invalid_prelude = bedrock_event_stream_frame("futureEvent", br#"{"ok":true}"#);
    invalid_prelude[8] ^= 0xff;
    let mut decoded = de_ResponseStream(stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(
        invalid_prelude,
    ))]));
    let error = decoded.next().await.unwrap().unwrap_err();
    assert!(error.to_string().contains("prelude CRC mismatch"));

    let mut invalid_message = bedrock_event_stream_frame("futureEvent", br#"{"ok":true}"#);
    let payload_index = invalid_message.len() - 5;
    invalid_message[payload_index] ^= 0xff;
    let mut decoded = de_ResponseStream(stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(
        invalid_message,
    ))]));
    let error = decoded.next().await.unwrap().unwrap_err();
    assert!(error.to_string().contains("message CRC mismatch"));
}

fn bedrock_event_stream_body(frames: Vec<Vec<u8>>) -> Vec<u8> {
    frames.into_iter().flatten().collect()
}

fn bedrock_event_stream_frame(event_type: &str, payload: &[u8]) -> Vec<u8> {
    let headers = bedrock_event_headers(event_type);
    let total_len = 12 + headers.len() + payload.len() + 4;
    let mut frame = Vec::new();
    frame.extend_from_slice(&(total_len as u32).to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    let prelude_crc = crc32fast::hash(&frame);
    frame.extend_from_slice(&prelude_crc.to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(payload);
    let message_crc = crc32fast::hash(&frame);
    frame.extend_from_slice(&message_crc.to_be_bytes());
    frame
}

fn bedrock_event_headers(event_type: &str) -> Vec<u8> {
    let mut headers = Vec::new();
    write_string_header(&mut headers, ":message-type", "event");
    write_string_header(&mut headers, ":event-type", event_type);
    headers
}

fn write_string_header(out: &mut Vec<u8>, name: &str, value: &str) {
    out.push(name.len() as u8);
    out.extend_from_slice(name.as_bytes());
    out.push(7);
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}
