use ilmek_runtime::{CONTENT_SECURITY_POLICY, Request};
use serde_json::{Value, json};

fn parse(value: Value) -> Result<Request, String> {
    Request::parse(value).map_err(|f| f.code)
}

#[test]
fn well_formed_requests_parse() {
    for request in [
        json!({"v":1,"op":"call","method":"a.b","input":{},"write":false,"operationId":null,"timeoutMs":null}),
        json!({"v":1,"op":"subscribe","topic":"a.b","cursor":null}),
        json!({"v":1,"op":"unsubscribe","subscriptionId":"x"}),
        json!({"v":1,"op":"storage.get","key":"draft"}),
        json!({"v":1,"op":"storage.set","key":"draft","value":{"a":1}}),
        json!({"v":1,"op":"storage.remove","key":"draft"}),
        json!({"v":1,"op":"connection.state"}),
    ] {
        assert!(parse(request.clone()).is_ok(), "{request}");
    }
}

#[test]
fn the_page_cannot_name_an_application_host_or_path() {
    for smuggled in [
        "appId",
        "app_id",
        "host",
        "address",
        "path",
        "endpoint",
        "serverId",
        "principal",
    ] {
        let mut call = json!({"v":1,"op":"call","method":"a.b","input":{},"write":false,"operationId":null,"timeoutMs":null});
        call[smuggled] = json!("01a1647f-de10-755f-beb4-79875d9e6368");
        assert_eq!(parse(call).unwrap_err(), "INVALID_MESSAGE", "{smuggled}");

        let mut storage = json!({"v":1,"op":"storage.get","key":"k"});
        storage[smuggled] = json!("other-app");
        assert_eq!(parse(storage).unwrap_err(), "INVALID_MESSAGE", "{smuggled}");
    }
}

#[test]
fn unknown_operations_versions_and_shapes_are_rejected() {
    for request in [
        json!({"v":1,"op":"shell.exec","command":"id"}),
        json!({"v":1,"op":"fs.read","path":"/etc/passwd"}),
        json!({"v":1,"op":"net.connect","host":"10.0.0.1"}),
        json!({"v":2,"op":"connection.state"}),
        json!({"op":"connection.state"}),
        json!({"v":1}),
        json!({"v":1,"op":7}),
        json!("call"),
        json!([1, 2]),
        json!({"v":1,"op":"call","method":"a.b","input":{}}),
        json!({"v":1,"op":"storage.set","key":"k"}),
    ] {
        assert_eq!(
            parse(request.clone()).unwrap_err(),
            "INVALID_MESSAGE",
            "{request}"
        );
    }
}

#[test]
fn the_content_security_policy_gives_pages_no_network_of_their_own() {
    let policy = CONTENT_SECURITY_POLICY;
    for required in [
        "default-src 'none'",
        "script-src ilmekapp:",
        "frame-src 'none'",
        "object-src 'none'",
        "form-action 'none'",
        "base-uri 'none'",
        "connect-src 'none'",
        "frame-ancestors tauri://localhost",
        "sandbox allow-scripts allow-forms",
    ] {
        assert!(policy.contains(required), "missing `{required}`");
    }
    for forbidden in [
        "unsafe-inline",
        "unsafe-eval",
        "http:",
        "https:",
        "ws:",
        "wss:",
        "*",
    ] {
        let without_ancestors = policy
            .split("frame-ancestors")
            .map(|part| part.split_once(';').map_or(part, |(_, rest)| rest))
            .collect::<String>()
            .replace("http://ilmekapp.localhost", "");
        assert!(
            !without_ancestors.contains(forbidden),
            "`{forbidden}` must not appear"
        );
    }
}

#[test]
fn the_embedded_sdk_matches_the_built_package() {
    let embedded = ilmek_runtime::SDK_SCRIPT;
    assert!(embedded.contains("postMessage") && !embedded.contains("__TAURI"));
    let built = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/sdk-js/dist/ilmek-sdk.iife.js");
    if let Ok(fresh) = std::fs::read_to_string(&built) {
        assert_eq!(
            fresh, embedded,
            "crates/ilmek-runtime/assets/ilmek-sdk.iife.js is stale; run scripts/packaging/sync-sdk.sh"
        );
    }
}
