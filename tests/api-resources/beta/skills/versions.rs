// Integration tests for beta Skill Versions API resources.
//
// Ported from TS SDK: tests/api-resources/beta/skills/versions.test.ts.

use anthropic_sdk::resources::beta::skills::{
    SkillVersionCreateParams, SkillVersionDeleteParams, SkillVersionListParams,
    SkillVersionRetrieveParams,
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

fn skill_version_json() -> serde_json::Value {
    serde_json::json!({
        "id": "skillver_123",
        "created_at": "2026-01-01T00:00:00Z",
        "description": "Does weather things",
        "directory": "weather",
        "name": "weather",
        "skill_id": "skill_123",
        "type": "skill_version",
        "version": "12345"
    })
}

fn skill_version_deleted_json() -> serde_json::Value {
    serde_json::json!({
        "id": "12345",
        "type": "skill_version_deleted"
    })
}

#[tokio::test]
async fn beta_skill_versions_create_sends_multipart_files() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/skills/skill_123/versions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_version_json()))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = SkillVersionCreateParams {
        files: Some(vec![Uploadable::from_bytes(
            b"# Skill".to_vec(),
            "weather/SKILL.md",
        )]),
        betas: None,
    };

    let version = client
        .beta()
        .skills()
        .versions()
        .create("skill_123", &params)
        .await
        .unwrap();
    assert_eq!(version.id, "skillver_123");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.query(), Some("beta=true"));
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("name=\"files[]\""), "body was: {body}");
    // TS Versions.create uses default stripFilenames=true.
    assert!(body.contains("filename=\"SKILL.md\""), "body was: {body}");
}

#[tokio::test]
async fn beta_skill_versions_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/skills/skill_123/versions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_version_create")
                .set_body_json(skill_version_json()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills/skill_123/versions/12345"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_version_retrieve")
                .set_body_json(skill_version_json()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills/skill_123/versions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_version_list")
                .set_body_json(serde_json::json!({
                    "data": [skill_version_json()],
                    "has_more": false,
                    "next_page": null
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/skills/skill_123/versions/12345"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_version_delete")
                .set_body_json(skill_version_deleted_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let versions = client.beta().skills().versions();
    let custom_beta = vec!["custom-beta".to_owned()];

    let created = versions
        .create_with_response(
            "skill_123",
            &SkillVersionCreateParams {
                files: Some(vec![Uploadable::from_bytes(
                    b"# Skill".to_vec(),
                    "weather/SKILL.md",
                )]),
                betas: Some(custom_beta.clone()),
            },
        )
        .await
        .unwrap();
    assert_eq!(created.data.id, "skillver_123");
    assert_eq!(
        created.request_id.as_deref(),
        Some("req_skill_version_create")
    );
    assert_eq!(
        created.response.header("request-id"),
        Some("req_skill_version_create")
    );

    let retrieved = versions
        .retrieve_with_response(
            "12345",
            &SkillVersionRetrieveParams {
                skill_id: "skill_123".to_owned(),
                betas: Some(custom_beta.clone()),
            },
        )
        .await
        .unwrap();
    assert_eq!(retrieved.data.id, "skillver_123");
    assert_eq!(
        retrieved.request_id.as_deref(),
        Some("req_skill_version_retrieve")
    );

    let listed = versions
        .list_with_response(
            "skill_123",
            Some(&SkillVersionListParams {
                limit: Some(2),
                page: Some("page-token".to_owned()),
                betas: Some(custom_beta.clone()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(listed.data.data.len(), 1);
    assert_eq!(listed.request_id.as_deref(), Some("req_skill_version_list"));

    let deleted = versions
        .delete_with_response(
            "12345",
            &SkillVersionDeleteParams {
                skill_id: "skill_123".to_owned(),
                betas: Some(custom_beta),
            },
        )
        .await
        .unwrap();
    assert_eq!(deleted.data.id, "12345");
    assert_eq!(
        deleted.request_id.as_deref(),
        Some("req_skill_version_delete")
    );
}

#[tokio::test]
async fn beta_skill_versions_retrieve_list_and_delete_send_beta_headers_and_query() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/skills/skill_123/versions/12345"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_version_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills/skill_123/versions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [skill_version_json()],
            "has_more": false,
            "next_page": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/skills/skill_123/versions/12345"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_version_deleted_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let retrieved = client
        .beta()
        .skills()
        .versions()
        .retrieve(
            "12345",
            &SkillVersionRetrieveParams {
                skill_id: "skill_123".to_owned(),
                betas: Some(vec!["custom-beta".to_owned()]),
            },
        )
        .await
        .unwrap();
    assert_eq!(retrieved.id, "skillver_123");

    let listed = client
        .beta()
        .skills()
        .versions()
        .list(
            "skill_123",
            Some(&SkillVersionListParams {
                limit: Some(2),
                page: Some("page-token".to_owned()),
                betas: Some(vec!["custom-beta".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(listed.data.len(), 1);

    let deleted = client
        .beta()
        .skills()
        .versions()
        .delete(
            "12345",
            &SkillVersionDeleteParams {
                skill_id: "skill_123".to_owned(),
                betas: Some(vec!["custom-beta".to_owned()]),
            },
        )
        .await
        .unwrap();
    assert_eq!(deleted.id, "12345");

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| request.headers.get("anthropic-beta").unwrap()
                == "custom-beta,skills-2025-10-02")
    );

    let list_request = requests
        .iter()
        .find(|request| {
            request.url.path() == "/v1/skills/skill_123/versions" && request.method == "GET"
        })
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(query.get("limit").map(String::as_str), Some("2"));
    assert_eq!(query.get("page").map(String::as_str), Some("page-token"));
    assert!(!query.contains_key("cursor"));
}

#[tokio::test]
async fn beta_skill_versions_request_options_path_override_applies_to_create_retrieve_list_and_delete()
 {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/skill-versions-create-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_version_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skill-versions-retrieve-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_version_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skill-versions-list-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [skill_version_json()],
            "has_more": false,
            "next_page": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/skill-versions-delete-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_version_deleted_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let custom_beta = vec!["custom-beta".to_owned()];

    let create_options = RequestOptions {
        path: Some("/v1/skill-versions-create-override".to_owned()),
        ..Default::default()
    };
    let created = client
        .beta()
        .skills()
        .versions()
        .create_with_options(
            "ignored_skill_id",
            &SkillVersionCreateParams {
                files: Some(vec![Uploadable::from_bytes(
                    b"# Skill".to_vec(),
                    "weather/SKILL.md",
                )]),
                betas: Some(custom_beta.clone()),
            },
            Some(&create_options),
        )
        .await
        .unwrap();
    assert_eq!(created.id, "skillver_123");

    let retrieve_options = RequestOptions {
        path: Some("/v1/skill-versions-retrieve-override".to_owned()),
        ..Default::default()
    };
    let retrieved = client
        .beta()
        .skills()
        .versions()
        .retrieve_with_options(
            "ignored_version",
            &SkillVersionRetrieveParams {
                skill_id: "ignored_skill_id".to_owned(),
                betas: Some(custom_beta.clone()),
            },
            Some(&retrieve_options),
        )
        .await
        .unwrap();
    assert_eq!(retrieved.id, "skillver_123");

    let list_options = RequestOptions {
        path: Some("/v1/skill-versions-list-override".to_owned()),
        ..Default::default()
    };
    let listed = client
        .beta()
        .skills()
        .versions()
        .list_with_options(
            "ignored_skill_id",
            Some(&SkillVersionListParams {
                limit: Some(2),
                page: Some("page-token".to_owned()),
                betas: Some(custom_beta.clone()),
            }),
            Some(&list_options),
        )
        .await
        .unwrap();
    assert_eq!(listed.data.len(), 1);

    let delete_options = RequestOptions {
        path: Some("/v1/skill-versions-delete-override".to_owned()),
        ..Default::default()
    };
    let deleted = client
        .beta()
        .skills()
        .versions()
        .delete_with_options(
            "ignored_version",
            &SkillVersionDeleteParams {
                skill_id: "ignored_skill_id".to_owned(),
                betas: Some(custom_beta),
            },
            Some(&delete_options),
        )
        .await
        .unwrap();
    assert_eq!(deleted.id, "12345");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .all(|request| request.headers.get("anthropic-beta").unwrap()
                == "custom-beta,skills-2025-10-02")
    );

    let create_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/skill-versions-create-override")
        .unwrap();
    let body = String::from_utf8_lossy(&create_request.body);
    assert!(body.contains("name=\"files[]\""), "body was: {body}");
    assert!(body.contains("filename=\"SKILL.md\""), "body was: {body}");

    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/skill-versions-list-override")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("limit").map(String::as_str), Some("2"));
    assert_eq!(query.get("page").map(String::as_str), Some("page-token"));
}
