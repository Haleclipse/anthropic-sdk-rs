// Mirrors TS SDK tests/buildHeaders.test.ts for ordered/nullable headers.

use anthropic_sdk::internal::headers::{
    HeaderLayer, HeaderValueInput, build_nullable_headers, is_empty_header_layer,
};

fn header_value(
    headers: &anthropic_sdk::internal::headers::NullableHeaders,
    name: &str,
) -> Option<String> {
    headers
        .values
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

#[test]
fn build_headers_matches_ts_nullable_and_array_cases() {
    let headers = build_nullable_headers(&[HeaderLayer::object(vec![(
        "content-type",
        HeaderValueInput::from("text/plain"),
    )])]);
    assert_eq!(
        header_value(&headers, "content-type").as_deref(),
        Some("text/plain")
    );

    let headers = build_nullable_headers(&[
        HeaderLayer::object(vec![("content-type", HeaderValueInput::from("text/plain"))]),
        HeaderLayer::object(vec![("Content-Type", HeaderValueInput::Undefined)]),
    ]);
    assert_eq!(
        header_value(&headers, "content-type").as_deref(),
        Some("text/plain")
    );

    let headers = build_nullable_headers(&[
        HeaderLayer::object(vec![("content-type", HeaderValueInput::from("text/plain"))]),
        HeaderLayer::object(vec![("Content-Type", HeaderValueInput::Null)]),
    ]);
    assert!(header_value(&headers, "content-type").is_none());
    assert!(headers.nulls.contains("content-type"));

    let headers = build_nullable_headers(&[HeaderLayer::object(vec![
        ("cookie", HeaderValueInput::from("name1=value1")),
        ("Cookie", HeaderValueInput::from("name2=value2")),
    ])]);
    assert_eq!(
        header_value(&headers, "cookie").as_deref(),
        Some("name2=value2")
    );

    let headers = build_nullable_headers(&[HeaderLayer::object(vec![
        ("cookie", HeaderValueInput::from("name1=value1")),
        ("Cookie", HeaderValueInput::Undefined),
    ])]);
    assert_eq!(
        header_value(&headers, "cookie").as_deref(),
        Some("name1=value1")
    );

    let headers = build_nullable_headers(&[HeaderLayer::object(vec![(
        "cookie",
        HeaderValueInput::Array(vec![
            HeaderValueInput::from("name1=value1"),
            HeaderValueInput::from("name2=value2"),
        ]),
    )])]);
    assert_eq!(
        header_value(&headers, "cookie").as_deref(),
        Some("name1=value1; name2=value2")
    );

    let headers = build_nullable_headers(&[HeaderLayer::object(vec![(
        "x-foo",
        HeaderValueInput::Array(vec![
            HeaderValueInput::from("name1=value1"),
            HeaderValueInput::from("name2=value2"),
        ]),
    )])]);
    assert_eq!(
        header_value(&headers, "x-foo").as_deref(),
        Some("name1=value1, name2=value2")
    );
}

#[test]
fn build_headers_appends_repeated_tuple_headers_like_ts() {
    let headers = build_nullable_headers(&[HeaderLayer::pairs(vec![
        ("cookie", HeaderValueInput::from("name1=value1")),
        ("cookie", HeaderValueInput::from("name2=value2")),
        ("Cookie", HeaderValueInput::from("name3=value3")),
    ])]);

    assert_eq!(
        header_value(&headers, "cookie").as_deref(),
        Some("name1=value1; name2=value2; name3=value3")
    );
}

#[test]
fn empty_header_layer_ignores_undefined_like_ts() {
    assert!(is_empty_header_layer(&HeaderLayer::object(Vec::<(
        &str,
        HeaderValueInput,
    )>::new())));
    assert!(is_empty_header_layer(&HeaderLayer::object(vec![(
        "x-foo",
        HeaderValueInput::Undefined
    ),])));
    assert!(!is_empty_header_layer(&HeaderLayer::object(vec![(
        "x-foo",
        HeaderValueInput::Null
    ),])));
}
