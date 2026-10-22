//! Compatibility with `tests/protocol/fixtures/wire_examples.ilmek.json`.

#![expect(clippy::expect_used, reason = "panicking helpers are fine in tests")]

use ilmek_protocol::digest::{manifest_signing_preimage, payload_hash, sha256};
use ilmek_protocol::{Envelope, Frame, FrameDecoder, FrameError, JsonError, MessageType};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../tests/protocol/fixtures/wire_examples.ilmek.json");

#[derive(Deserialize)]
struct Fixtures {
    valid_frames: Vec<ValidFrame>,
    invalid_frames: Vec<InvalidFrame>,
    hash_example: HashExample,
    signature_example: SignatureExample,
}

#[derive(Deserialize)]
struct ValidFrame {
    name: String,
    metadata_utf8: String,
    body_hex: String,
    metadata_length: usize,
    body_length: usize,
    frame_hex: String,
}

#[derive(Deserialize)]
struct InvalidFrame {
    name: String,
    frame_hex: String,
    expected_local_decoder_error: String,
}

#[derive(Deserialize)]
struct HashExample {
    payload_hash: String,
}

#[derive(Deserialize)]
struct SignatureExample {
    manifest_utf8: String,
    manifest_hash: String,
    preimage_hex: String,
}

fn fixtures() -> Fixtures {
    serde_json::from_str(FIXTURE).expect("fixture file parses")
}

fn decode_all(bytes: &[u8]) -> Result<Vec<Frame>, FrameError> {
    let mut decoder = FrameDecoder::new();
    decoder.push(bytes);
    let mut frames = Vec::new();
    while let Some(frame) = decoder.next_frame()? {
        frames.push(frame);
    }
    decoder.finish()?;
    Ok(frames)
}

#[test]
fn valid_frames_round_trip_byte_for_byte() {
    for fx in fixtures().valid_frames {
        let wire = hex::decode(&fx.frame_hex).unwrap();
        let frames = decode_all(&wire).unwrap_or_else(|e| panic!("{}: {e}", fx.name));
        assert_eq!(frames.len(), 1, "{}", fx.name);

        let frame = &frames[0];
        assert_eq!(frame.metadata, fx.metadata_utf8.as_bytes(), "{}", fx.name);
        assert_eq!(
            frame.body,
            hex::decode(&fx.body_hex).unwrap(),
            "{}",
            fx.name
        );
        assert_eq!(frame.metadata.len(), fx.metadata_length, "{}", fx.name);
        assert_eq!(frame.body.len(), fx.body_length, "{}", fx.name);
        assert_eq!(frame.encode().unwrap(), wire, "{}", fx.name);
    }
}

#[test]
fn envelope_types_match_fixture_names() {
    let expected = [
        ("hello_control", MessageType::Hello),
        ("hello_ack", MessageType::HelloAck),
        ("resolve", MessageType::Resolve),
        ("write_call", MessageType::AppCall),
        ("write_result", MessageType::AppResult),
        ("get_manifest", MessageType::GetManifest),
        ("manifest", MessageType::Manifest),
        ("get_asset", MessageType::GetAsset),
        ("asset_info", MessageType::AssetInfo),
        ("asset_chunk", MessageType::AssetChunk),
        ("asset_end", MessageType::AssetEnd),
        ("error", MessageType::Error),
    ];
    for fx in fixtures().valid_frames {
        let want = expected
            .iter()
            .find(|(name, _)| *name == fx.name)
            .unwrap()
            .1;
        let got = Envelope::parse(fx.metadata_utf8.as_bytes())
            .unwrap()
            .message_type;
        assert_eq!(got, want, "{}", fx.name);
    }
}

#[test]
fn invalid_frames_are_rejected_with_expected_class() {
    for fx in fixtures().invalid_frames {
        let wire = hex::decode(&fx.frame_hex).unwrap();
        let result = decode_all(&wire);
        match fx.expected_local_decoder_error.as_str() {
            "bad_magic" => assert_eq!(result.unwrap_err(), FrameError::BadMagic, "{}", fx.name),
            "length_limit" => assert!(
                matches!(
                    result.unwrap_err(),
                    FrameError::MetadataLength(_) | FrameError::BodyLength(_)
                ),
                "{}",
                fx.name
            ),
            "length_mismatch" => {
                assert_eq!(result.unwrap_err(), FrameError::Truncated, "{}", fx.name);
            }
            "duplicate_key" => {
                let frame = &result.unwrap()[0];
                assert_eq!(
                    Envelope::parse(&frame.metadata).unwrap_err().to_string(),
                    JsonError::DuplicateKey("type".into()).to_string(),
                    "{}",
                    fx.name
                );
            }
            other => panic!("unknown expectation {other}"),
        }
    }
}

#[test]
fn rpc_payload_hash_matches_fixture() {
    let fx = fixtures();
    let call = fx
        .valid_frames
        .iter()
        .find(|f| f.name == "write_call")
        .unwrap();
    let envelope = Envelope::parse(call.metadata_utf8.as_bytes()).unwrap();
    let app_id = envelope.data["app_id"].as_str().unwrap().parse().unwrap();
    let method = envelope.data["method"].as_str().unwrap();
    let body = hex::decode(&call.body_hex).unwrap();

    let hash = payload_hash(app_id, method, &body);
    assert_eq!(hex::encode(hash), fx.hash_example.payload_hash);
    assert_eq!(
        envelope.data["payload_hash"].as_str().unwrap(),
        fx.hash_example.payload_hash
    );
}

#[test]
fn manifest_hash_and_signing_preimage_match_fixture() {
    let sig = fixtures().signature_example;
    let manifest = sig.manifest_utf8.as_bytes();
    assert_eq!(hex::encode(sha256(manifest)), sig.manifest_hash);
    assert_eq!(
        hex::encode(manifest_signing_preimage(manifest)),
        sig.preimage_hex
    );
}
