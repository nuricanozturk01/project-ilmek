mod common;

use std::sync::Arc;

use common::{Community, LOOPBACK, listen, mutual_server, target};
use ilmek_core::tls::{TlsClient, TlsError, TlsServer};
use rustls::CertificateError;
use rustls::HandshakeKind;
use time::{Duration, OffsetDateTime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn mutual_tls_authenticates_both_sides_and_carries_data() {
    let community = Community::new();
    let server = community.peer(Some(LOOPBACK));
    let client = community.peer(None);
    let tls_server = mutual_server(&community, &server);
    let (listener, address) = listen().await;

    let accepting = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut accepted = tls_server.accept(tcp).await.unwrap();
        let mut byte = [0u8; 1];
        accepted.stream.read_exact(&mut byte).await.unwrap();
        accepted.stream.write_all(&byte).await.unwrap();
        accepted.stream.flush().await.unwrap();
        accepted.peer
    });

    let tls_client = TlsClient::new(Some(&client.identity), community.ca_der()).unwrap();
    let mut connection = tls_client
        .connect(target(address, server.id))
        .await
        .unwrap();
    connection.stream.write_all(&[42]).await.unwrap();
    let mut echoed = [0u8; 1];
    connection.stream.read_exact(&mut echoed).await.unwrap();

    assert_eq!(echoed, [42]);
    assert_eq!(connection.server.id, server.id);
    assert_eq!(accepting.await.unwrap().map(|p| p.id), Some(client.id));
}

#[tokio::test]
async fn enrollment_endpoint_needs_no_client_certificate() {
    let community = Community::new();
    let registry = community.peer(Some(LOOPBACK));
    let tls_server = TlsServer::enrollment(&registry.identity).unwrap();
    let (listener, address) = listen().await;

    let accepting = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        tls_server.accept(tcp).await.unwrap().peer
    });

    let tls_client = TlsClient::new(None, community.ca_der()).unwrap();
    tls_client
        .connect(target(address, registry.id))
        .await
        .unwrap();
    assert_eq!(accepting.await.unwrap(), None);
}

#[tokio::test]
async fn mutual_endpoint_refuses_a_client_without_certificate() {
    let community = Community::new();
    let server = community.peer(Some(LOOPBACK));
    let tls_server = mutual_server(&community, &server);
    let (listener, address) = listen().await;
    let accepting = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        tls_server.accept(tcp).await.map(|_| ())
    });

    let tls_client = TlsClient::new(None, community.ca_der()).unwrap();
    let _ = tls_client.connect(target(address, server.id)).await;

    assert!(accepting.await.unwrap().is_err());
}

#[tokio::test]
async fn client_rejects_a_server_certificate_from_another_ca() {
    let community = Community::new();
    let rogue = Community::new();
    let server = rogue.peer(Some(LOOPBACK));
    let (listener, address) = listen().await;
    let tls_server = mutual_server(&rogue, &server);
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let _ = tls_server.accept(tcp).await;
    });

    let tls_client = TlsClient::new(None, community.ca_der()).unwrap();
    let error = tls_client
        .connect(target(address, server.id))
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            TlsError::Certificate(CertificateError::UnknownIssuer)
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn server_rejects_a_client_certificate_from_another_ca() {
    let community = Community::new();
    let rogue = Community::new();
    let server = community.peer(Some(LOOPBACK));
    let intruder = rogue.peer(None);
    let tls_server = mutual_server(&community, &server);
    let (listener, address) = listen().await;
    let accepting = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        tls_server.accept(tcp).await.map(|_| ())
    });

    let tls_client = TlsClient::new(Some(&intruder.identity), community.ca_der()).unwrap();
    let _ = tls_client.connect(target(address, server.id)).await;

    let error = accepting.await.unwrap().unwrap_err();
    assert!(
        matches!(
            error,
            TlsError::Certificate(CertificateError::UnknownIssuer)
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn server_certificate_for_another_ip_is_rejected() {
    let community = Community::new();
    let server = community.peer(Some("10.77.10.10".parse().unwrap()));
    let (listener, address) = listen().await;
    let tls_server = mutual_server(&community, &server);
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let _ = tls_server.accept(tcp).await;
    });

    let tls_client = TlsClient::new(None, community.ca_der()).unwrap();
    let error = tls_client
        .connect(target(address, server.id))
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            TlsError::Certificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. }
            )
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_valid_certificate_of_the_wrong_peer_is_rejected() {
    let community = Community::new();
    let impostor = community.peer(Some(LOOPBACK));
    let expected = community.peer(Some(LOOPBACK));
    let (listener, address) = listen().await;
    let tls_server = mutual_server(&community, &impostor);
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let _ = tls_server.accept(tcp).await;
    });

    let tls_client = TlsClient::new(None, community.ca_der()).unwrap();
    let error = tls_client
        .connect(target(address, expected.id))
        .await
        .unwrap_err();

    assert!(
        matches!(error, TlsError::UnexpectedPeer { expected: e, actual: a } if e == expected.id && a == impostor.id),
        "{error:?}"
    );
}

#[tokio::test]
async fn expired_server_certificate_is_rejected() {
    let community = Community::new();
    let now = OffsetDateTime::now_utc();
    let server =
        community.peer_valid_for(Some(LOOPBACK), now - Duration::days(3), Duration::days(1));
    let (listener, address) = listen().await;
    let tls_server = mutual_server(&community, &server);
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let _ = tls_server.accept(tcp).await;
    });

    let tls_client = TlsClient::new(None, community.ca_der()).unwrap();
    let error = tls_client
        .connect(target(address, server.id))
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            TlsError::Certificate(
                CertificateError::Expired | CertificateError::ExpiredContext { .. }
            )
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn peer_that_offers_a_different_alpn_is_refused() {
    let community = Community::new();
    let server = community.peer(Some(LOOPBACK));
    let client = community.peer(None);
    let tls_server = mutual_server(&community, &server);
    let (listener, address) = listen().await;
    let accepting = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        tls_server.accept(tcp).await.map(|_| ())
    });

    let mut roots = rustls::RootCertStore::empty();
    roots.add(community.ca_der().to_vec().into()).unwrap();
    let (chain, key) = (
        client
            .identity
            .chain_der
            .iter()
            .cloned()
            .map(Into::into)
            .collect(),
        rustls::pki_types::PrivateKeyDer::Pkcs8(client.identity.key_pkcs8_der.clone().into()),
    );
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_root_certificates(roots)
    .with_client_auth_cert(chain, key)
    .unwrap();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let tcp = TcpStream::connect(address).await.unwrap();
    let name = rustls::pki_types::ServerName::IpAddress(LOOPBACK.into());
    let _ = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await;

    assert!(accepting.await.unwrap().is_err());
}

#[tokio::test]
async fn non_tls_bytes_are_refused() {
    let community = Community::new();
    let server = community.peer(Some(LOOPBACK));
    let tls_server = mutual_server(&community, &server);
    let (listener, address) = listen().await;
    let accepting = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        tls_server.accept(tcp).await.map(|_| ())
    });

    let mut tcp = TcpStream::connect(address).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();

    assert!(matches!(
        accepting.await.unwrap(),
        Err(TlsError::Handshake(_))
    ));
}

#[tokio::test]
async fn sessions_are_never_resumed() {
    let community = Community::new();
    let server = community.peer(Some(LOOPBACK));
    let tls_server = Arc::new(mutual_server(&community, &server));
    let (listener, address) = listen().await;
    tokio::spawn({
        let tls_server = Arc::clone(&tls_server);
        async move {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let tls_server = Arc::clone(&tls_server);
                tokio::spawn(async move {
                    if let Ok(mut accepted) = tls_server.accept(tcp).await {
                        let _ = accepted.stream.read_u8().await;
                    }
                });
            }
        }
    });

    let client = community.peer(None);
    let tls_client = TlsClient::new(Some(&client.identity), community.ca_der()).unwrap();
    for _ in 0..2 {
        let mut connection = tls_client
            .connect(target(address, server.id))
            .await
            .unwrap();
        connection.stream.write_u8(0).await.unwrap();
        assert_eq!(
            connection.stream.get_ref().1.handshake_kind(),
            Some(HandshakeKind::Full)
        );
    }
}
