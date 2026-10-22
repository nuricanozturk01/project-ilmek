#![expect(clippy::unwrap_used, reason = "panicking helpers are fine in tests")]

use std::net::Ipv4Addr;

use ilmek_identity::{
    CertificateAuthority, DeviceKey, LeafSpec, PkiError, peer_id_from_certificate,
};
use ilmek_protocol::Uuid7;
use rcgen::string::Ia5String;
use rcgen::{CertificateParams, KeyPair, SanType};
use time::{Duration, OffsetDateTime};
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::{FromDer, X509Certificate};

fn authority() -> CertificateAuthority {
    CertificateAuthority::generate(
        Uuid7::generate(),
        OffsetDateTime::now_utc(),
        Duration::days(365),
    )
    .unwrap()
}

fn spec(peer_id: Uuid7, server_ip: Option<Ipv4Addr>) -> LeafSpec {
    LeafSpec {
        peer_id,
        server_ip,
        not_before: OffsetDateTime::now_utc() - Duration::minutes(1),
        lifetime: Duration::days(30),
    }
}

fn uri_san(value: &str) -> SanType {
    SanType::URI(Ia5String::try_from(value.to_owned()).unwrap())
}

#[test]
fn issued_leaf_carries_the_ca_chosen_identity() {
    let ca = authority();
    let peer_id = Uuid7::generate();
    let ip = Ipv4Addr::new(10, 77, 10, 10);
    let csr = DeviceKey::generate().unwrap().csr_der().unwrap();

    let leaf = ca.issue(&csr, &spec(peer_id, Some(ip))).unwrap();

    assert_eq!(peer_id_from_certificate(&leaf).unwrap(), peer_id);
    let (_, cert) = X509Certificate::from_der(&leaf).unwrap();
    let names = &cert
        .subject_alternative_name()
        .unwrap()
        .unwrap()
        .value
        .general_names;
    assert_eq!(names.len(), 2);
    assert!(
        names
            .iter()
            .any(|n| matches!(n, GeneralName::IPAddress(b) if *b == ip.octets()))
    );
    assert!(!cert.is_ca());
}

#[test]
fn requested_names_in_the_csr_are_ignored() {
    let ca = authority();
    let mut params = CertificateParams::new(vec!["evil.example".into()]).unwrap();
    params
        .subject_alt_names
        .push(uri_san(&format!("urn:uuid:{}", Uuid7::generate())));
    let csr = params
        .serialize_request(&KeyPair::generate().unwrap())
        .unwrap();
    let granted = Uuid7::generate();

    let leaf = ca.issue(csr.der(), &spec(granted, None)).unwrap();

    assert_eq!(peer_id_from_certificate(&leaf).unwrap(), granted);
    let (_, cert) = X509Certificate::from_der(&leaf).unwrap();
    let names = &cert
        .subject_alternative_name()
        .unwrap()
        .unwrap()
        .value
        .general_names;
    assert_eq!(names.len(), 1);
}

#[test]
fn csr_with_a_broken_self_signature_is_refused() {
    let ca = authority();
    let mut csr = DeviceKey::generate().unwrap().csr_der().unwrap();
    let last = csr.len() - 1;
    csr[last] ^= 0xff;
    assert!(ca.issue(&csr, &spec(Uuid7::generate(), None)).is_err());
}

#[test]
fn certificates_without_exactly_one_identity_uri_are_rejected() {
    let ca_key = KeyPair::generate().unwrap();
    let build = |sans: Vec<SanType>| {
        let mut params = CertificateParams::default();
        params.subject_alt_names = sans;
        params.self_signed(&ca_key).unwrap().der().to_vec()
    };
    let urn = |id: Uuid7| uri_san(&format!("urn:uuid:{id}"));

    assert!(matches!(
        peer_id_from_certificate(&build(vec![])),
        Err(PkiError::PeerIdentity)
    ));
    assert!(matches!(
        peer_id_from_certificate(&build(vec![urn(Uuid7::generate()), urn(Uuid7::generate())])),
        Err(PkiError::PeerIdentity)
    ));
    assert!(matches!(
        peer_id_from_certificate(&build(vec![uri_san("https://example.test/")])),
        Err(PkiError::PeerIdentity)
    ));
    assert!(matches!(
        peer_id_from_certificate(&build(vec![uri_san(
            "urn:uuid:f47ac10b-58cc-4372-a567-0e02b2c3d479"
        )])),
        Err(PkiError::PeerIdentity)
    ));
}
