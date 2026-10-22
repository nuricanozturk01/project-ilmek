#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use ilmek_protocol::JsonError;
use ilmek_protocol::strict_json::parse_object;

fn err(text: &str) -> JsonError {
    parse_object(text.as_bytes()).unwrap_err()
}

#[test]
fn accepts_plain_objects() {
    let map = parse_object(br#"{"a":1,"b":[true,null,"x"],"c":{"d":9007199254740991}}"#).unwrap();
    assert_eq!(map.len(), 3);
}

#[test]
fn rejects_duplicate_keys_even_when_nested() {
    assert_eq!(err(r#"{"a":1,"a":2}"#), JsonError::DuplicateKey("a".into()));
    assert_eq!(
        err(r#"{"x":{"k":1,"k":1}}"#),
        JsonError::DuplicateKey("k".into())
    );
}

#[test]
fn rejects_bom_and_bad_utf8() {
    assert_eq!(parse_object(b"\xef\xbb\xbf{}").unwrap_err(), JsonError::Bom);
    assert_eq!(
        parse_object(b"{\"a\":\"\xff\"}").unwrap_err(),
        JsonError::Utf8
    );
}

#[test]
fn rejects_non_u53_numbers() {
    for text in [
        r#"{"a":1.5}"#,
        r#"{"a":1.0}"#,
        r#"{"a":1e3}"#,
        r#"{"a":-1}"#,
        r#"{"a":9007199254740992}"#,
        r#"{"a":18446744073709551616}"#,
    ] {
        assert!(
            matches!(err(text), JsonError::Number | JsonError::Syntax(_)),
            "{text}"
        );
    }
    assert_eq!(err(r#"{"a":-1}"#), JsonError::Number);
    assert_eq!(err(r#"{"a":1.0}"#), JsonError::Number);
}

#[test]
fn rejects_nan_and_infinity_literals() {
    assert!(matches!(err(r#"{"a":NaN}"#), JsonError::Syntax(_)));
    assert!(matches!(err(r#"{"a":Infinity}"#), JsonError::Syntax(_)));
}

#[test]
fn depth_limit_is_sixteen_containers() {
    let nest = |levels: usize| {
        format!(
            "{}1{}",
            "{\"k\":".repeat(levels - 1),
            "}".repeat(levels - 1)
        )
    };
    let deep = |levels: usize| {
        let inner = "[".repeat(levels - 1) + &"]".repeat(levels - 1);
        format!(r#"{{"k":{inner}}}"#)
    };
    assert!(parse_object(deep(16).as_bytes()).is_ok());
    assert_eq!(err(&deep(17)), JsonError::TooDeep);
    let _ = nest;
}

#[test]
fn container_size_limits() {
    let object: String = (0..257)
        .map(|i| format!("\"k{i}\":0"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(err(&format!("{{{object}}}")), JsonError::TooManyEntries);

    let array = vec!["0"; 257].join(",");
    assert_eq!(
        err(&format!(r#"{{"a":[{array}]}}"#)),
        JsonError::TooManyEntries
    );
    let ok = vec!["0"; 256].join(",");
    assert!(parse_object(format!(r#"{{"a":[{ok}]}}"#).as_bytes()).is_ok());
}

#[test]
fn rejects_non_objects_and_trailing_data() {
    assert_eq!(err("[1]"), JsonError::NotObject);
    assert!(matches!(err("{} {}"), JsonError::Syntax(_)));
}

#[test]
fn body_profile_allows_application_data_but_keeps_the_structural_rules() {
    use ilmek_protocol::strict_json::{Profile, parse_object_with};
    let body = |text: &str| parse_object_with(text.as_bytes(), Profile::Body);

    let wide = vec!["0"; 600].join(",");
    assert!(
        body(&format!(
            r#"{{"a":[{wide}],"price":12.5,"debt":-3,"big":18446744073709551615}}"#
        ))
        .is_ok()
    );

    assert_eq!(
        body(r#"{"a":1,"a":2}"#).unwrap_err(),
        JsonError::DuplicateKey("a".into())
    );
    assert_eq!(
        parse_object_with(b"\xef\xbb\xbf{}", Profile::Body).unwrap_err(),
        JsonError::Bom
    );
    let deep = format!(r#"{{"k":{}{}}}"#, "[".repeat(17), "]".repeat(17));
    assert_eq!(body(&deep).unwrap_err(), JsonError::TooDeep);
    assert!(
        parse_object(br#"{"price":12.5}"#).is_err(),
        "metadata stays integer-only"
    );
}
