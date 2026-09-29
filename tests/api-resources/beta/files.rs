// Integration tests for beta Files API resources.
//
// Ported from TS SDK: tests/api-resources/beta/files.test.ts

use anthropic_sdk::resources::beta::files::{
    FileDeleteParams, FileDownloadParams, FileListParams, FileRetrieveMetadataParams,
    FileUploadParams,
};
use anthropic_sdk::{Anthropic, ClientOptions, RequestOptions, Uploadable};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_client(server_url: &str) -> Anthropic {
    Anthropic::new(ClientOptions {
        api_key: "test-api-key".into(),
        base_url: Some(server_url.to_owned()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed")
}

fn file_metadata_json() -> serde_json::Value {
    serde_json::json!({
        "id": "file_123",
        "created_at": "2026-01-01T00:00:00Z",
        "filename": "hello.txt",
        "mime_type": "text/plain",
        "size_bytes": 5,
        "type": "file",
        "downloadable": true
    })
}

#[tokio::test]
async fn beta_files_upload_sends_multipart_file() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(file_metadata_json()))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = FileUploadParams::new(
        Uploadable::from_bytes(b"hello".to_vec(), "hello.txt")
            .with_stainless_helper("mcpResourceToFile"),
    );

    let uploaded = client.beta().files().upload(&params).await.unwrap();
    assert_eq!(uploaded.id, "file_123");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let body = String::from_utf8_lossy(&request.body);
    assert!(body.contains("name=\"file\""), "body was: {body}");
    assert!(body.contains("filename=\"hello.txt\""), "body was: {body}");
    assert!(body.contains("hello"), "body was: {body}");

    let content_type = request
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    assert_eq!(
        request
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("mcpResourceToFile")
    );
}

#[tokio::test]
async fn beta_files_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/files"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_upload")
                .set_body_json(file_metadata_json()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files/file_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_retrieve")
                .set_body_json(file_metadata_json()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_list")
                .set_body_json(serde_json::json!({
                    "data": [file_metadata_json()],
                    "has_more": false,
                    "first_id": "file_123",
                    "last_id": "file_123"
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/files/file_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_delete")
                .set_body_json(serde_json::json!({
                    "id": "file_123",
                    "type": "file_deleted"
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files/file_123/content"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_download")
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(vec![4, 5, 6]),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let files = client.beta().files();

    let upload = files
        .upload_with_response(&FileUploadParams::new(Uploadable::from_bytes(
            b"hello".to_vec(),
            "hello.txt",
        )))
        .await
        .unwrap();
    assert_eq!(upload.data.id, "file_123");
    assert_eq!(upload.request_id.as_deref(), Some("req_upload"));
    assert_eq!(upload.response.header("request-id"), Some("req_upload"));

    let retrieved = files
        .retrieve_metadata_with_response("file_123", None)
        .await
        .unwrap();
    assert_eq!(retrieved.data.id, "file_123");
    assert_eq!(retrieved.request_id.as_deref(), Some("req_retrieve"));

    let listed = files.list_with_response(None).await.unwrap();
    assert_eq!(listed.data.data.len(), 1);
    assert_eq!(listed.request_id.as_deref(), Some("req_list"));

    let deleted = files.delete_with_response("file_123", None).await.unwrap();
    assert_eq!(deleted.data.id, "file_123");
    assert_eq!(deleted.request_id.as_deref(), Some("req_delete"));

    let downloaded = files
        .download_with_response("file_123", None)
        .await
        .unwrap();
    assert_eq!(downloaded.data, vec![4, 5, 6]);
    assert_eq!(downloaded.response.body, vec![4, 5, 6]);
    assert_eq!(downloaded.request_id.as_deref(), Some("req_download"));
}

#[tokio::test]
async fn beta_files_upload_honors_request_options_path_override() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/files-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(file_metadata_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = FileUploadParams::new(Uploadable::from_bytes(b"hello".to_vec(), "hello.txt"));
    let options = RequestOptions {
        path: Some("/v1/files-override".to_owned()),
        ..Default::default()
    };

    let uploaded = client
        .beta()
        .files()
        .upload_with_options(&params, Some(&options))
        .await
        .unwrap();
    assert_eq!(uploaded.id, "file_123");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("name=\"file\""), "body was: {body}");
}

#[tokio::test]
async fn beta_files_retrieve_list_and_delete_send_beta_headers_and_query() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/files/file_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(file_metadata_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [file_metadata_json()],
            "has_more": false,
            "first_id": "file_123",
            "last_id": "file_123"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/files/file_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "file_123",
            "type": "file_deleted"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let retrieve_params = FileRetrieveMetadataParams {
        betas: Some(vec!["custom-beta".to_owned()]),
    };
    let file = client
        .beta()
        .files()
        .retrieveMetadata("file_123", Some(&retrieve_params))
        .await
        .unwrap();
    assert_eq!(file.id, "file_123");

    let page = client
        .beta()
        .files()
        .list(Some(&FileListParams {
            before_id: Some("before".to_owned()),
            after_id: Some("after".to_owned()),
            limit: Some(1),
            betas: Some(vec!["custom-beta".to_owned()]),
        }))
        .await
        .unwrap();
    assert_eq!(page.data.len(), 1);

    let deleted = client
        .beta()
        .files()
        .delete(
            "file_123",
            Some(&FileDeleteParams {
                betas: Some(vec!["custom-beta".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(deleted.id, "file_123");

    let requests = server.received_requests().await.unwrap();
    assert!(requests
        .iter()
        .all(|request| request.headers.get("anthropic-beta").unwrap()
            == "custom-beta,files-api-2025-04-14"));
    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/files" && request.method == "GET")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("limit").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn beta_files_request_options_path_override_applies_to_list_retrieve_delete_and_download() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/files-list-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [file_metadata_json()],
            "has_more": false,
            "first_id": "file_123",
            "last_id": "file_123"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files-retrieve-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(file_metadata_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/files-delete-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "file_123",
            "type": "file_deleted"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files-download-override"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(vec![9, 8, 7]),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let custom_beta = vec!["custom-beta".to_owned()];

    let list_options = RequestOptions {
        path: Some("/v1/files-list-override".to_owned()),
        ..Default::default()
    };
    let listed = client
        .beta()
        .files()
        .list_with_options(
            Some(&FileListParams {
                before_id: Some("before".to_owned()),
                after_id: Some("after".to_owned()),
                limit: Some(2),
                betas: Some(custom_beta.clone()),
            }),
            Some(&list_options),
        )
        .await
        .unwrap();
    assert_eq!(listed.data.len(), 1);

    let retrieve_options = RequestOptions {
        path: Some("/v1/files-retrieve-override".to_owned()),
        ..Default::default()
    };
    let retrieved = client
        .beta()
        .files()
        .retrieve_metadata_with_options(
            "ignored_file_id",
            Some(&FileRetrieveMetadataParams {
                betas: Some(custom_beta.clone()),
            }),
            Some(&retrieve_options),
        )
        .await
        .unwrap();
    assert_eq!(retrieved.id, "file_123");

    let delete_options = RequestOptions {
        path: Some("/v1/files-delete-override".to_owned()),
        ..Default::default()
    };
    let deleted = client
        .beta()
        .files()
        .delete_with_options(
            "ignored_file_id",
            Some(&FileDeleteParams {
                betas: Some(custom_beta.clone()),
            }),
            Some(&delete_options),
        )
        .await
        .unwrap();
    assert_eq!(deleted.id, "file_123");

    let download_options = RequestOptions {
        path: Some("/v1/files-download-override".to_owned()),
        ..Default::default()
    };
    let bytes = client
        .beta()
        .files()
        .download_with_options(
            "ignored_file_id",
            Some(&FileDownloadParams {
                betas: Some(custom_beta),
            }),
            Some(&download_options),
        )
        .await
        .unwrap();
    assert_eq!(bytes, vec![9, 8, 7]);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests
        .iter()
        .all(|request| request.headers.get("anthropic-beta").unwrap()
            == "custom-beta,files-api-2025-04-14"));

    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/files-list-override")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("limit").map(String::as_str), Some("2"));

    let download_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/files-download-override")
        .unwrap();
    assert_eq!(
        download_request
            .headers
            .get("accept")
            .and_then(|v| v.to_str().ok()),
        Some("application/binary")
    );
}

#[tokio::test]
async fn beta_files_download_returns_binary_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/files/file_123/content"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(vec![0, 1, 2, 3, 255]),
        )
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let bytes = client
        .beta()
        .files()
        .download("file_123", None)
        .await
        .unwrap();
    assert_eq!(bytes, vec![0, 1, 2, 3, 255]);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let accept = requests[0]
        .headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert_eq!(accept, "application/binary");
}
