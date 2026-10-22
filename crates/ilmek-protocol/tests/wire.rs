#![expect(clippy::expect_used, reason = "panicking helpers are fine in tests")]

use ilmek_protocol::wire::{ErrorCode, Message, Mode, Version, WireError};
use ilmek_protocol::{Frame, MessageType};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../tests/protocol/fixtures/wire_examples.ilmek.json");

#[derive(Deserialize)]
struct Fixtures {
    valid_frames: Vec<ValidFrame>,
}

#[derive(Deserialize)]
struct ValidFrame {
    name: String,
    metadata_utf8: String,
    body_hex: String,
}

const COMMUNITY: &str = "01a160e8-5001-7e8d-beb8-a77c859b95da";
const REQUEST: &str = "01a160e8-5064-78b9-9504-af4963fa6df3";

fn frame(kind: &str, data: &str, body: &[u8]) -> Frame {
    let metadata = format!(
        r#"{{"type":"{kind}","request_id":"{REQUEST}","community_id":"{COMMUNITY}","data":{data}}}"#
    );
    Frame {
        metadata: metadata.into_bytes(),
        body: body.to_vec(),
    }
}

fn decode_error(kind: &str, data: &str, body: &[u8]) -> WireError {
    Message::decode(&frame(kind, data, body)).expect_err("must be rejected")
}

#[test]
fn every_fixture_decodes_and_re_encodes_byte_for_byte() {
    let fixtures: Fixtures = serde_json::from_str(FIXTURE).expect("fixture parses");
    for fx in fixtures.valid_frames {
        let original = Frame {
            metadata: fx.metadata_utf8.into_bytes(),
            body: hex::decode(fx.body_hex).expect("hex"),
        };
        let (header, message) =
            Message::decode(&original).unwrap_or_else(|e| panic!("{}: {e}", fx.name));
        assert_eq!(
            message.encode(header).expect("encode"),
            original,
            "{}",
            fx.name
        );
    }
}

#[test]
fn hello_fields_are_typed() {
    let (header, message) = Message::decode(&frame(
        "HELLO",
        r#"{"version":1,"mode":"bulk","client_name":"x","client_version":"1.2.3"}"#,
        b"",
    ))
    .expect("decodes");
    let Message::Hello(hello) = message else {
        panic!("not HELLO")
    };
    assert_eq!(hello.mode, Mode::Bulk);
    assert_eq!(hello.client_version, Version::new(1, 2, 3));
    assert_eq!(header.request_id.to_string(), REQUEST);
}

#[test]
fn unknown_data_fields_are_rejected() {
    let error = decode_error("RESOLVE", r#"{"namespace":"a","slug":"b","extra":1}"#, b"");
    assert!(matches!(
        error,
        WireError::Data {
            message_type: MessageType::Resolve,
            ..
        }
    ));
}

#[test]
fn nullable_fields_must_still_be_present() {
    let app = "01a160e8-5002-7283-82aa-567144a5a6d3";
    let hash = "a".repeat(64);
    let without = format!(
        r#"{{"app_id":"{app}","client_app_version":"1.0.0","method":"m","payload_hash":"{hash}"}}"#
    );
    assert!(matches!(
        decode_error("APP_CALL", &without, b"{}"),
        WireError::Data { .. }
    ));
    let with = without.replace(r#""payload_hash""#, r#""operation_id":null,"payload_hash""#);
    assert!(Message::decode(&frame("APP_CALL", &with, b"{}")).is_ok());
}

#[test]
fn scalar_grammar_is_enforced() {
    for (kind, data) in [
        ("RESOLVE", r#"{"namespace":"Nuri","slug":"forum"}"#),
        ("RESOLVE", r#"{"namespace":"-a","slug":"forum"}"#),
        ("RESOLVE", r#"{"namespace":"a","slug":""}"#),
        (
            "GET_ASSET",
            r#"{"app_id":"01a160e8-5002-7283-82aa-567144a5a6d3","version":"01.0.0","manifest_hash":"00","path":"a","asset_hash":"00","offset":0}"#,
        ),
        ("PING", r#"{"nonce":"AAAA="}"#),
        (
            "HELLO",
            r#"{"version":1,"mode":"control","client_name":"x","client_version":"1.0"}"#,
        ),
    ] {
        assert!(
            matches!(decode_error(kind, data, b""), WireError::Data { .. }),
            "{kind} {data}"
        );
    }
}

#[test]
fn bodies_are_only_allowed_where_defined() {
    assert!(matches!(
        decode_error("GOODBYE_ACK", "{}", b"x"),
        WireError::UnexpectedBody(MessageType::GoodbyeAck)
    ));
}

#[test]
fn json_bodies_must_be_strict_objects() {
    let app = "01a160e8-5002-7283-82aa-567144a5a6d3";
    let data = format!(r#"{{"app_id":"{app}","operation_id":null}}"#);
    for body in [&b"[]"[..], b"{\"a\":1,\"a\":2}", b""] {
        assert!(matches!(
            decode_error("APP_RESULT", &data, body),
            WireError::BodyJson(..)
        ));
    }
}

#[test]
fn error_codes_round_trip_in_wire_spelling() {
    let data = r#"{"code":"OPERATION_CONFLICT","message":"m","retryable":false,"retry_after_ms":null,"details":{}}"#;
    let (_, message) = Message::decode(&frame("ERROR", data, b"")).expect("decodes");
    let Message::Error(error) = message else {
        panic!("not ERROR")
    };
    assert_eq!(error.code, ErrorCode::OperationConflict);
}

#[test]
fn ipc_types_are_not_network_messages() {
    let error = Message::decode(&frame("IPC_CALL", "{}", b"")).expect_err("rejected");
    assert!(matches!(error, WireError::Envelope(_)));
}
