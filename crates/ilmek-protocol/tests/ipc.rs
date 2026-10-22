#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use ilmek_protocol::Uuid7;
use ilmek_protocol::ipc::{IpcCall, IpcError, IpcHello, IpcMessage, IpcReady, IpcSubscribe};
use ilmek_protocol::wire::{
    AppResult, ErrorCode, ErrorData, Hash, Header, Message, Payload, Token, Version,
};

fn header() -> Header {
    Header {
        request_id: Uuid7::generate(),
        community_id: Uuid7::generate(),
    }
}

fn call() -> IpcCall {
    IpcCall {
        app_id: Uuid7::generate(),
        client_app_version: Version::new(1, 0, 0),
        method: "topics.create".parse().unwrap(),
        operation_id: Some(Uuid7::generate()),
        payload_hash: Hash::from_bytes([3; 32]),
        principal_id: Uuid7::generate(),
    }
}

#[test]
fn every_message_survives_an_encode_decode_round_trip() {
    let app_id = Uuid7::generate();
    let messages = [
        IpcMessage::Hello(IpcHello {
            token: Token::new(vec![9; 32]),
            app_id,
        }),
        IpcMessage::Ready(IpcReady { app_id }),
        IpcMessage::Call(Payload {
            data: call(),
            body: b"{\"a\":1}".to_vec(),
        }),
        IpcMessage::Result(Payload {
            data: AppResult {
                app_id,
                operation_id: None,
            },
            body: b"{}".to_vec(),
        }),
        IpcMessage::Error(ErrorData {
            code: ErrorCode::NotFound,
            message: "x".parse().unwrap(),
            retryable: false,
            retry_after_ms: None,
            details: serde_json::Map::new(),
        }),
        IpcMessage::Subscribe(IpcSubscribe {
            app_id,
            client_app_version: Version::new(1, 0, 0),
            topic: "forum.events".parse().unwrap(),
            cursor: None,
            principal_id: Uuid7::generate(),
        }),
    ];
    for message in messages {
        let hdr = header();
        let frame = message.encode(hdr).unwrap();
        let (decoded_header, decoded) = IpcMessage::decode(&frame).unwrap();
        assert_eq!((decoded_header, decoded), (hdr, message));
    }
}

#[test]
fn network_and_ipc_vocabularies_do_not_overlap() {
    let ipc_frame = IpcMessage::Ready(IpcReady {
        app_id: Uuid7::generate(),
    })
    .encode(header())
    .unwrap();
    assert!(
        Message::decode(&ipc_frame).is_err(),
        "network endpoints must reject IPC_*"
    );

    let network_frame = Message::Ping(ilmek_protocol::wire::Nonce {
        nonce: Token::new(vec![0; 16]),
    })
    .encode(header())
    .unwrap();
    assert!(matches!(
        IpcMessage::decode(&network_frame),
        Err(IpcError::UnknownType(_))
    ));
}

#[test]
fn principal_and_nullable_fields_are_mandatory() {
    let mut frame = IpcMessage::Call(Payload {
        data: call(),
        body: b"{}".to_vec(),
    })
    .encode(header())
    .unwrap();
    let text = String::from_utf8(frame.metadata.clone()).unwrap();
    frame.metadata = text
        .replace("\"principal_id\"", "\"principle_id\"")
        .into_bytes();
    assert!(matches!(
        IpcMessage::decode(&frame),
        Err(IpcError::Data { .. })
    ));

    let mut frame = IpcMessage::Call(Payload {
        data: call(),
        body: b"{}".to_vec(),
    })
    .encode(header())
    .unwrap();
    frame.body = b"[]".to_vec();
    assert!(matches!(
        IpcMessage::decode(&frame),
        Err(IpcError::BodyJson(..))
    ));
}
