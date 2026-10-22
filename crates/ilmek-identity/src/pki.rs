//! Community CA and the leaf certificate profile (PROTOCOL.md §3.1).

use std::net::{IpAddr, Ipv4Addr};

use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::hex_lower;
use rcgen::string::Ia5String;
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DistinguishedName,
    DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType, SerialNumber,
};
use rustls_pki_types::{CertificateDer, CertificateSigningRequestDer};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::{FromDer, X509Certificate};

const IDENTITY_URN_PREFIX: &str = "urn:uuid:";

#[derive(Debug, Error)]
pub enum PkiError {
    #[error("certificate generation failed: {0}")]
    Generate(#[from] rcgen::Error),
    #[error("certificate is not parseable X.509")]
    Parse,
    #[error("certificate must carry exactly one `urn:uuid:<UUIDv7>` URI SAN")]
    PeerIdentity,
    #[error("system randomness unavailable")]
    Randomness,
}

/// Parameters the CA decides; nothing here comes from the requester's CSR.
#[derive(Debug, Clone, Copy)]
pub struct LeafSpec {
    pub peer_id: Uuid7,
    /// Present for servers, which must be verifiable by IP address.
    pub server_ip: Option<Ipv4Addr>,
    pub not_before: OffsetDateTime,
    pub lifetime: Duration,
}

pub struct CertificateAuthority {
    issuer: Issuer<'static, KeyPair>,
    certificate_der: Vec<u8>,
}

impl std::fmt::Debug for CertificateAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertificateAuthority")
            .finish_non_exhaustive()
    }
}

impl CertificateAuthority {
    pub fn generate(
        community_id: Uuid7,
        not_before: OffsetDateTime,
        lifetime: Duration,
    ) -> Result<Self, PkiError> {
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.not_before = not_before;
        params.not_after = not_before + lifetime;
        params.distinguished_name = common_name(&format!("ilmek-ca-{community_id}"));

        let key = KeyPair::generate()?;
        let certificate_der = params.self_signed(&key)?.der().to_vec();
        Ok(Self {
            issuer: Issuer::new(params, key),
            certificate_der,
        })
    }

    /// Restores a CA from its stored certificate and PKCS#8 private key.
    pub fn from_parts(certificate_der: &[u8], key_pkcs8_der: &[u8]) -> Result<Self, PkiError> {
        let key = KeyPair::try_from(key_pkcs8_der)?;
        let issuer = Issuer::from_ca_cert_der(&CertificateDer::from(certificate_der), key)?;
        Ok(Self {
            issuer,
            certificate_der: certificate_der.to_vec(),
        })
    }

    #[must_use]
    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    /// PKCS#8 private key, for the caller to store with restrictive permissions.
    #[must_use]
    pub fn private_key_pkcs8_der(&self) -> Vec<u8> {
        self.issuer.key().serialize_der()
    }

    /// Signs `csr_der` after checking its self-signature; every other requested field is ignored.
    pub fn issue(&self, csr_der: &[u8], spec: &LeafSpec) -> Result<Vec<u8>, PkiError> {
        let mut csr = CertificateSigningRequestParams::from_der(
            &CertificateSigningRequestDer::from(csr_der),
        )?;
        let params = &mut csr.params;
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        params.not_before = spec.not_before;
        params.not_after = spec.not_before + spec.lifetime;
        params.distinguished_name = common_name(&spec.peer_id.to_string());
        params.subject_alt_names = subject_alt_names(spec)?;
        params.serial_number = Some(random_serial()?);
        Ok(csr.signed_by(&self.issuer)?.der().to_vec())
    }
}

/// A device's private key, generated locally and never sent to the registry.
pub struct DeviceKey(KeyPair);

impl std::fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKey").finish_non_exhaustive()
    }
}

impl DeviceKey {
    pub fn generate() -> Result<Self, PkiError> {
        Ok(Self(KeyPair::generate()?))
    }

    pub fn csr_der(&self) -> Result<Vec<u8>, PkiError> {
        let request = CertificateParams::default().serialize_request(&self.0)?;
        Ok(request.der().to_vec())
    }

    #[must_use]
    pub fn pkcs8_der(&self) -> Vec<u8> {
        self.0.serialize_der()
    }

    #[must_use]
    pub fn into_identity(self, chain_der: Vec<Vec<u8>>) -> Identity {
        Identity {
            chain_der,
            key_pkcs8_der: self.0.serialize_der(),
        }
    }
}

/// Leaf-first certificate chain and its PKCS#8 private key.
pub struct Identity {
    pub chain_der: Vec<Vec<u8>>,
    pub key_pkcs8_der: Vec<u8>,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("chain_len", &self.chain_der.len())
            .finish_non_exhaustive()
    }
}

/// Extracts the peer identity from a leaf certificate.
pub fn peer_id_from_certificate(certificate_der: &[u8]) -> Result<Uuid7, PkiError> {
    let (_, certificate) =
        X509Certificate::from_der(certificate_der).map_err(|_| PkiError::Parse)?;
    let san = certificate
        .subject_alternative_name()
        .map_err(|_| PkiError::Parse)?
        .ok_or(PkiError::PeerIdentity)?;

    let mut uris = san
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::URI(uri) => Some(*uri),
            _ => None,
        });
    let (Some(uri), None) = (uris.next(), uris.next()) else {
        return Err(PkiError::PeerIdentity);
    };
    uri.strip_prefix(IDENTITY_URN_PREFIX)
        .and_then(|id| id.parse().ok())
        .ok_or(PkiError::PeerIdentity)
}

fn subject_alt_names(spec: &LeafSpec) -> Result<Vec<SanType>, PkiError> {
    let urn = format!("{IDENTITY_URN_PREFIX}{}", spec.peer_id);
    let mut names = vec![SanType::URI(Ia5String::try_from(urn)?)];
    if let Some(ip) = spec.server_ip {
        names.push(SanType::IpAddress(IpAddr::V4(ip)));
    }
    Ok(names)
}

fn common_name(value: &str) -> DistinguishedName {
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, value);
    name
}

fn random_serial() -> Result<SerialNumber, PkiError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| PkiError::Randomness)?;
    // Positive, non-zero leading byte keeps the DER integer at its full 16 bytes.
    bytes[0] = (bytes[0] & 0x7f) | 0x40;
    Ok(SerialNumber::from_slice(&bytes))
}

/// Lowercase hex certificate serial, the form used in trust snapshots.
pub fn certificate_serial_hex(certificate_der: &[u8]) -> Result<String, PkiError> {
    let (_, certificate) =
        X509Certificate::from_der(certificate_der).map_err(|_| PkiError::Parse)?;
    Ok(hex_lower(certificate.tbs_certificate.raw_serial()))
}

/// `notAfter` as Unix epoch milliseconds.
pub fn certificate_not_after_ms(certificate_der: &[u8]) -> Result<u64, PkiError> {
    let (_, certificate) =
        X509Certificate::from_der(certificate_der).map_err(|_| PkiError::Parse)?;
    let seconds = certificate.validity().not_after.timestamp();
    u64::try_from(seconds)
        .map(|s| s * 1000)
        .map_err(|_| PkiError::Parse)
}
