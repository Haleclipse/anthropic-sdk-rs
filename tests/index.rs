// Root export parity with TS SDK src/index.ts.

use std::any::TypeId;

use anthropic_sdk::{
    AI_PROMPT, APIConnectionError, APIConnectionTimeoutError, APIError, APIPromise, APIResponse,
    APIUserAbortError, Anthropic, AnthropicError, ApiError, ApiResponse, AuthenticationError,
    AutoParseableOutputFormat, BadRequestError, BaseAnthropic, ClientOptions, ConflictError,
    ExtractParsedContentFromParams, HUMAN_PROMPT, InternalServerError, NotFoundError, PagePromise,
    ParseableMessageCreateParams, ParsedContentBlock, ParsedMessage, PermissionDeniedError,
    RateLimitError, RawResponse, ToFileInput, UnprocessableEntityError, Uploadable, VERSION,
    to_file, toFile,
};

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

#[test]
fn top_level_compatibility_barrel_modules_match_ts_deprecated_files() {
    assert_same_type::<anthropic_sdk::api_promise::APIPromise<()>, anthropic_sdk::APIPromise<()>>();
    assert_same_type::<anthropic_sdk::error::APIError, anthropic_sdk::ApiError>();
    assert_same_type::<anthropic_sdk::pagination::Page<i32>, anthropic_sdk::Page<i32>>();
    assert_same_type::<anthropic_sdk::streaming::Stream, anthropic_sdk::core::streaming::Stream>();
    assert_same_type::<anthropic_sdk::uploads::Uploadable, anthropic_sdk::Uploadable>();
    assert_eq!(anthropic_sdk::version::VERSION, anthropic_sdk::VERSION);

    fn assert_api_resource_alias<T: anthropic_sdk::resource::APIResource>() {}
    struct TestResource {
        client: anthropic_sdk::Anthropic,
    }
    impl anthropic_sdk::core::resource::ApiResource for TestResource {
        fn client(&self) -> &anthropic_sdk::Anthropic {
            &self.client
        }
    }
    assert_api_resource_alias::<TestResource>();
}

#[test]
fn root_exports_prompt_constants_like_ts_index() {
    assert_eq!(HUMAN_PROMPT, "\n\nHuman:");
    assert_eq!(AI_PROMPT, "\n\nAssistant:");
    assert_eq!(VERSION, "0.74.0");
}

#[test]
fn root_exports_upload_helpers_like_ts_index() {
    let upload = to_file("/tmp/example/input.jsonl");
    assert_eq!(upload, Uploadable::from_path("/tmp/example/input.jsonl"));
    assert_eq!(toFile("/tmp/example/input.jsonl"), upload);
    assert_same_type::<ToFileInput, Uploadable>();

    let promise: APIPromise<()> = Box::pin(async { Ok(()) });
    drop(promise);
    assert_same_type::<PagePromise<anthropic_sdk::Page<i32>>, APIPromise<anthropic_sdk::Page<i32>>>(
    );
    assert_same_type::<APIResponse<()>, ApiResponse<()>>();
    let raw = RawResponse {
        status: 200,
        url: "https://example.com".to_owned(),
        headers: std::collections::HashMap::new(),
        body: Vec::new(),
    };
    assert_eq!(raw.status, 200);
}

#[test]
fn root_exports_parser_names_like_ts_index() {
    assert_same_type::<
        ParsedMessage<serde_json::Value>,
        anthropic_sdk::sdk_lib::parser::ParsedMessage<serde_json::Value>,
    >();
    assert_same_type::<
        ParsedContentBlock<serde_json::Value>,
        anthropic_sdk::sdk_lib::parser::ParsedContentBlock<serde_json::Value>,
    >();
    assert_same_type::<
        ParseableMessageCreateParams,
        anthropic_sdk::resources::messages::MessageCreateParams,
    >();
    assert_same_type::<ExtractParsedContentFromParams<serde_json::Value>, serde_json::Value>();

    let format = anthropic_sdk::resources::messages::JsonOutputFormat {
        schema: serde_json::json!({"type":"object"}),
        type_name: "json_schema".to_owned(),
    };
    let auto = AutoParseableOutputFormat::<serde_json::Value>::new(format.clone());
    assert_eq!(
        auto.as_format().type_name.as_str(),
        format.type_name.as_str()
    );
    let unwrapped = auto.into_format();
    assert_eq!(unwrapped.schema, format.schema);
}

#[test]
fn root_exports_client_names_like_ts_index() {
    assert_same_type::<BaseAnthropic, Anthropic>();
}

#[test]
fn client_ts_style_property_and_utility_aliases_are_available() {
    let client = Anthropic::new(ClientOptions {
        api_key: "test-api-key".into(),
        auth_token: "test-auth-token".into(),
        base_url: Some("http://localhost:5000/custom".to_owned()),
        timeout: Some(1234),
        max_retries: Some(4),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(client.apiKey(), Some("test-api-key"));
    assert_eq!(client.authToken(), Some("test-auth-token"));
    assert_eq!(client.baseURL(), "http://localhost:5000/custom");
    assert_eq!(client.timeout(), 1234);
    assert_eq!(client.logLevel(), anthropic_sdk::LogLevel::Warn);
    assert_eq!(client.logLevel().as_str(), "warn");
    assert_eq!(client.logLevel().to_string(), "warn");
    assert_eq!(
        "debug".parse::<anthropic_sdk::LogLevel>().unwrap(),
        anthropic_sdk::LogLevel::Debug
    );
    assert!(client.logger().is_none());
    assert_eq!(client.maxRetries(), 4);
    assert_eq!(
        client.buildURL("/foo", None, None).unwrap(),
        "http://localhost:5000/custom/foo"
    );
    assert_eq!(
        client
            .stringifyQuery(vec![(
                "a/b",
                anthropic_sdk::internal::query::QueryValue::from("c d"),
            )])
            .unwrap(),
        "a%2Fb=c%20d"
    );
    assert_eq!(
        client.calculateNonstreamingTimeout(1024, None).unwrap(),
        600_000
    );
    assert!(
        client
            .calculate_nonstreaming_timeout(25_000, None)
            .unwrap_err()
            .to_string()
            .contains("#long-requests")
    );
}

#[test]
fn root_exports_error_names_like_ts_index() {
    assert_same_type::<AnthropicError, ApiError>();
    assert_same_type::<APIError, ApiError>();
    assert_same_type::<APIConnectionError, ApiError>();
    assert_same_type::<APIConnectionTimeoutError, ApiError>();
    assert_same_type::<APIUserAbortError, ApiError>();
    assert_same_type::<BadRequestError, ApiError>();
    assert_same_type::<AuthenticationError, ApiError>();
    assert_same_type::<PermissionDeniedError, ApiError>();
    assert_same_type::<NotFoundError, ApiError>();
    assert_same_type::<ConflictError, ApiError>();
    assert_same_type::<UnprocessableEntityError, ApiError>();
    assert_same_type::<RateLimitError, ApiError>();
    assert_same_type::<InternalServerError, ApiError>();
}
