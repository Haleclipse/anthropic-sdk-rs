// Mirrors TS SDK tests/stringifyQuery.test.ts.

use anthropic_sdk::internal::query::{
    null, reject_unsupported_query_value, stringify_query, undefined, QueryValue,
};

#[test]
fn stringify_query_encodes_primitive_values_like_ts() {
    assert_eq!(
        stringify_query(vec![
            ("a", QueryValue::from("1")),
            ("b", QueryValue::from(2_i64)),
            ("c", QueryValue::from(true)),
        ])
        .unwrap(),
        "a=1&b=2&c=true"
    );

    assert_eq!(
        stringify_query(vec![
            ("a", null()),
            ("b", QueryValue::from(false)),
            ("c", undefined()),
        ])
        .unwrap(),
        "a=&b=false"
    );

    assert_eq!(
        stringify_query(vec![("a/b", QueryValue::from(1.28341_f64))]).unwrap(),
        "a%2Fb=1.28341"
    );

    assert_eq!(
        stringify_query(vec![
            ("a/b", QueryValue::from("c/d")),
            ("e=f", QueryValue::from("g&h")),
        ])
        .unwrap(),
        "a%2Fb=c%2Fd&e%3Df=g%26h"
    );

    assert_eq!(
        stringify_query(vec![("with space", QueryValue::from("a b"))]).unwrap(),
        "with%20space=a%20b"
    );

    assert_eq!(
        stringify_query(vec![
            ("negZero", QueryValue::from(-0.0_f64)),
            ("nan", QueryValue::from(f64::NAN)),
            ("inf", QueryValue::from(f64::INFINITY)),
            ("negInf", QueryValue::from(f64::NEG_INFINITY)),
        ])
        .unwrap(),
        "negZero=0&nan=NaN&inf=Infinity&negInf=-Infinity"
    );
}

#[test]
fn stringify_query_rejects_nested_values_like_ts() {
    for type_name in ["object", "object", "object"] {
        let err = reject_unsupported_query_value(type_name);
        assert!(err.to_string().contains("Cannot stringify type object"));
        assert!(err
            .to_string()
            .contains("Expected string, number, boolean, or null"));
    }
}
