// Integration tests for beta Skills API resources.
//
// Ported from TS SDK: tests/api-resources/beta/skills/skills.test.ts.

use anthropic_sdk::resources::beta::skills::{
    SkillCreateParams, SkillDeleteParams, SkillListParams, SkillRetrieveParams,
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

fn skill_json() -> serde_json::Value {
    serde_json::json!({
        "id": "skill_123",
        "created_at": "2026-01-01T00:00:00Z",
        "display_title": "Weather Skill",
        "latest_version": "12345",
        "source": "custom",
        "type": "skill",
        "updated_at": "2026-01-01T00:00:00Z"
    })
}

fn skill_deleted_json() -> serde_json::Value {
    serde_json::json!({
        "id": "skill_123",
        "type": "skill_deleted"
    })
}

#[tokio::test]
async fn beta_skills_create_sends_multipart_fields_and_files() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/skills"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_json()))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = SkillCreateParams {
        display_title: Some("Weather Skill".to_owned()),
        files: Some(vec![Uploadable::from_bytes(
            b"# Skill".to_vec(),
            "weather/SKILL.md",
        )]),
        betas: None,
    };

    let skill = client.beta().skills().create(&params).await.unwrap();
    assert_eq!(skill.id, "skill_123");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.query(), Some("beta=true"));
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("name=\"display_title\""), "body was: {body}");
    assert!(body.contains("Weather Skill"), "body was: {body}");
    assert!(body.contains("name=\"files[]\""), "body was: {body}");
    // TS Skills.create passes stripFilenames=false, so preserve the path-like name.
    assert!(
        body.contains("filename=\"weather/SKILL.md\""),
        "body was: {body}"
    );
}

#[tokio::test]
async fn beta_skills_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/skills"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_create")
                .set_body_json(skill_json()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills/skill_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_retrieve")
                .set_body_json(skill_json()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_list")
                .set_body_json(serde_json::json!({
                    "data": [skill_json()],
                    "has_more": false,
                    "next_page": null
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/skills/skill_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_skill_delete")
                .set_body_json(skill_deleted_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let skills = client.beta().skills();
    let custom_beta = vec!["custom-beta".to_owned()];

    let created = skills
        .create_with_response(&SkillCreateParams {
            display_title: Some("Weather Skill".to_owned()),
            files: Some(vec![Uploadable::from_bytes(
                b"# Skill".to_vec(),
                "weather/SKILL.md",
            )]),
            betas: Some(custom_beta.clone()),
        })
        .await
        .unwrap();
    assert_eq!(created.data.id, "skill_123");
    assert_eq!(created.request_id.as_deref(), Some("req_skill_create"));
    assert_eq!(
        created.response.header("request-id"),
        Some("req_skill_create")
    );

    let retrieved = skills
        .retrieve_with_response(
            "skill_123",
            Some(&SkillRetrieveParams {
                betas: Some(custom_beta.clone()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(retrieved.data.id, "skill_123");
    assert_eq!(retrieved.request_id.as_deref(), Some("req_skill_retrieve"));

    let listed = skills
        .list_with_response(Some(&SkillListParams {
            source: Some("custom".to_owned()),
            limit: Some(2),
            page: Some("page-token".to_owned()),
            betas: Some(custom_beta.clone()),
        }))
        .await
        .unwrap();
    assert_eq!(listed.data.data.len(), 1);
    assert_eq!(listed.request_id.as_deref(), Some("req_skill_list"));

    let deleted = skills
        .delete_with_response(
            "skill_123",
            Some(&SkillDeleteParams {
                betas: Some(custom_beta),
            }),
        )
        .await
        .unwrap();
    assert_eq!(deleted.data.id, "skill_123");
    assert_eq!(deleted.request_id.as_deref(), Some("req_skill_delete"));
}

#[tokio::test]
async fn beta_skills_retrieve_list_and_delete_send_beta_headers_and_query() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/skills/skill_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [skill_json()],
            "has_more": false,
            "next_page": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/skills/skill_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_deleted_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let retrieved = client
        .beta()
        .skills()
        .retrieve(
            "skill_123",
            Some(&SkillRetrieveParams {
                betas: Some(vec!["custom-beta".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(retrieved.id, "skill_123");

    let listed = client
        .beta()
        .skills()
        .list(Some(&SkillListParams {
            source: Some("custom".to_owned()),
            limit: Some(2),
            page: Some("page-token".to_owned()),
            betas: Some(vec!["custom-beta".to_owned()]),
        }))
        .await
        .unwrap();
    assert_eq!(listed.data.len(), 1);

    let deleted = client
        .beta()
        .skills()
        .delete(
            "skill_123",
            Some(&SkillDeleteParams {
                betas: Some(vec!["custom-beta".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(deleted.id, "skill_123");

    let requests = server.received_requests().await.unwrap();
    assert!(requests
        .iter()
        .all(|request| request.headers.get("anthropic-beta").unwrap()
            == "custom-beta,skills-2025-10-02"));

    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/skills" && request.method == "GET")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(query.get("source").map(String::as_str), Some("custom"));
    assert_eq!(query.get("limit").map(String::as_str), Some("2"));
    assert_eq!(query.get("page").map(String::as_str), Some("page-token"));
    assert!(!query.contains_key("cursor"));
}

#[tokio::test]
async fn beta_skills_request_options_path_override_applies_to_create_retrieve_list_and_delete() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/skills-create-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills-retrieve-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_json()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/skills-list-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [skill_json()],
            "has_more": false,
            "next_page": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/skills-delete-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(skill_deleted_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let custom_beta = vec!["custom-beta".to_owned()];

    let create_options = RequestOptions {
        path: Some("/v1/skills-create-override".to_owned()),
        ..Default::default()
    };
    let created = client
        .beta()
        .skills()
        .create_with_options(
            &SkillCreateParams {
                display_title: Some("Weather Skill".to_owned()),
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
    assert_eq!(created.id, "skill_123");

    let retrieve_options = RequestOptions {
        path: Some("/v1/skills-retrieve-override".to_owned()),
        ..Default::default()
    };
    let retrieved = client
        .beta()
        .skills()
        .retrieve_with_options(
            "ignored_skill_id",
            Some(&SkillRetrieveParams {
                betas: Some(custom_beta.clone()),
            }),
            Some(&retrieve_options),
        )
        .await
        .unwrap();
    assert_eq!(retrieved.id, "skill_123");

    let list_options = RequestOptions {
        path: Some("/v1/skills-list-override".to_owned()),
        ..Default::default()
    };
    let listed = client
        .beta()
        .skills()
        .list_with_options(
            Some(&SkillListParams {
                source: Some("custom".to_owned()),
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
        path: Some("/v1/skills-delete-override".to_owned()),
        ..Default::default()
    };
    let deleted = client
        .beta()
        .skills()
        .delete_with_options(
            "ignored_skill_id",
            Some(&SkillDeleteParams {
                betas: Some(custom_beta),
            }),
            Some(&delete_options),
        )
        .await
        .unwrap();
    assert_eq!(deleted.id, "skill_123");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests
        .iter()
        .all(|request| request.headers.get("anthropic-beta").unwrap()
            == "custom-beta,skills-2025-10-02"));

    let create_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/skills-create-override")
        .unwrap();
    let body = String::from_utf8_lossy(&create_request.body);
    assert!(body.contains("name=\"display_title\""), "body was: {body}");
    assert!(body.contains("name=\"files[]\""), "body was: {body}");

    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/skills-list-override")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("source").map(String::as_str), Some("custom"));
    assert_eq!(query.get("limit").map(String::as_str), Some("2"));
    assert_eq!(query.get("page").map(String::as_str), Some("page-token"));
}
