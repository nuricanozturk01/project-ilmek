#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panicking helpers are fine in tests"
)]

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ilmek_identity::{PublisherError, PublisherKey, PublisherSigner};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../tests/protocol/fixtures/wire_examples.ilmek.json");

#[derive(Deserialize)]
struct Fixtures {
    signature_example: Example,
}

#[derive(Deserialize)]
struct Example {
    test_only_seed_hex: String,
    public_key_base64url: String,
    publisher_key_id: String,
    manifest_utf8: String,
    signature_base64url: String,
}

fn example() -> Example {
    serde_json::from_str::<Fixtures>(FIXTURE)
        .expect("fixture parses")
        .signature_example
}

fn signature(example: &Example) -> Vec<u8> {
    URL_SAFE_NO_PAD
        .decode(&example.signature_base64url)
        .unwrap()
}

#[test]
fn fixture_signature_verifies_and_key_id_matches() {
    let ex = example();
    let key = PublisherKey::from_base64url(&ex.public_key_base64url).unwrap();
    assert_eq!(key.key_id(), ex.publisher_key_id);
    key.verify_manifest(ex.manifest_utf8.as_bytes(), &signature(&ex))
        .unwrap();
}

#[test]
fn signing_with_the_fixture_seed_reproduces_the_fixture_signature() {
    let ex = example();
    let seed: [u8; 32] = hex::decode(&ex.test_only_seed_hex)
        .unwrap()
        .try_into()
        .unwrap();
    let signer = PublisherSigner::from_seed(&seed);
    assert_eq!(signer.public_key().to_base64url(), ex.public_key_base64url);
    assert_eq!(
        signer.sign_manifest(ex.manifest_utf8.as_bytes()).to_vec(),
        signature(&ex)
    );
}

#[test]
fn any_manifest_byte_change_invalidates_the_signature() {
    let ex = example();
    let key = PublisherKey::from_base64url(&ex.public_key_base64url).unwrap();
    let mut manifest = ex.manifest_utf8.into_bytes();
    manifest.push(b' ');
    assert_eq!(
        key.verify_manifest(
            &manifest,
            &URL_SAFE_NO_PAD.decode(ex.signature_base64url).unwrap()
        ),
        Err(PublisherError::BadSignature)
    );
}

#[test]
fn another_publishers_key_does_not_verify() {
    let ex = example();
    let stranger = PublisherSigner::generate().unwrap().public_key();
    assert_eq!(
        stranger.verify_manifest(ex.manifest_utf8.as_bytes(), &signature(&ex)),
        Err(PublisherError::BadSignature)
    );
}

#[test]
fn malformed_keys_and_signatures_are_rejected() {
    assert_eq!(
        PublisherKey::from_base64url("AAAA"),
        Err(PublisherError::PublicKey)
    );
    assert_eq!(
        PublisherKey::from_base64url("not base64!"),
        Err(PublisherError::PublicKey)
    );
    let key = PublisherSigner::generate().unwrap().public_key();
    assert_eq!(
        key.verify_manifest(b"{}", &[0; 63]),
        Err(PublisherError::SignatureLength)
    );
}
