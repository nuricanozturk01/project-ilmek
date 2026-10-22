use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use ilmek_identity::{CertificateAuthority, CommunityProfile, DeviceKey, LeafSpec};
use ilmek_protocol::Uuid7;
use ilmek_store::Database;
use ilmek_store::rusqlite::params;
use time::{Duration, OffsetDateTime};

use crate::error::RegistryError;
use crate::files::{self, Layout};
use crate::schema::MIGRATIONS;
use crate::settings::Settings;

const CA_LIFETIME_DAYS: i64 = 3650;
const DEFAULT_IDENTITY_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy)]
pub struct InitOptions {
    /// The address clients reach the registry on; becomes the IP SAN of its certificate.
    pub advertised_ip: Ipv4Addr,
    pub identity_lifetime_days: Option<i64>,
}

fn endpoint(listen: SocketAddr, ip: Ipv4Addr) -> SocketAddrV4 {
    SocketAddrV4::new(ip, listen.port())
}

/// Creates a new community: CA, registry identity, database and the bootstrap profile.
pub fn initialize(
    settings: &Settings,
    options: &InitOptions,
    now: OffsetDateTime,
) -> Result<CommunityProfile, RegistryError> {
    let layout = Layout::new(&settings.data_dir);
    files::create_dir(layout.root())?;

    let community_id = Uuid7::generate();
    let registry_id = Uuid7::generate();
    let ca = CertificateAuthority::generate(
        community_id,
        now - Duration::minutes(1),
        Duration::days(CA_LIFETIME_DAYS),
    )
    .map_err(|e| RegistryError::Internal(e.to_string()))?;
    files::write_secret(&layout.ca_key(), &ca.private_key_pkcs8_der())?;
    files::write_public(&layout.ca_certificate(), ca.certificate_der())?;

    issue_identity(&layout, &ca, registry_id, options, now)?;

    let profile = CommunityProfile::new(
        community_id,
        registry_id,
        endpoint(settings.control_listen, options.advertised_ip),
        endpoint(settings.enroll_listen, options.advertised_ip),
        ca.certificate_der().to_vec(),
    );
    files::write_public(
        &layout.profile(),
        &profile
            .to_json()
            .map_err(|e| RegistryError::Internal(e.to_string()))?,
    )?;

    let db = Database::open(&layout.database(), MIGRATIONS)?;
    db.call_blocking::<_, RegistryError>(|connection| {
        connection.execute(
            "INSERT INTO community (id, registry_id, created_at_ms) VALUES (?1, ?2, ?3)",
            params![
                community_id.to_string(),
                registry_id.to_string(),
                to_ms(now)
            ],
        )?;
        Ok(())
    })?;
    Ok(profile)
}

/// Issues a fresh server certificate for the registry (the previous one expires after 30 days).
pub fn reissue_identity(
    settings: &Settings,
    options: &InitOptions,
    now: OffsetDateTime,
) -> Result<(), RegistryError> {
    let layout = Layout::new(&settings.data_dir);
    let profile = CommunityProfile::from_json(&files::read(&layout.profile())?)
        .map_err(|e| RegistryError::Internal(e.to_string()))?;
    let ca = CertificateAuthority::from_parts(
        &files::read(&layout.ca_certificate())?,
        &files::read(&layout.ca_key())?,
    )
    .map_err(|e| RegistryError::Internal(e.to_string()))?;
    issue_identity_replacing(&layout, &ca, profile.registry_id, options, now)
}

fn leaf(
    ca: &CertificateAuthority,
    registry_id: Uuid7,
    options: &InitOptions,
    now: OffsetDateTime,
) -> Result<(Vec<u8>, Vec<u8>), RegistryError> {
    let key = DeviceKey::generate().map_err(|e| RegistryError::Internal(e.to_string()))?;
    let csr = key
        .csr_der()
        .map_err(|e| RegistryError::Internal(e.to_string()))?;
    let days = options
        .identity_lifetime_days
        .unwrap_or(DEFAULT_IDENTITY_DAYS);
    let spec = LeafSpec {
        peer_id: registry_id,
        server_ip: Some(options.advertised_ip),
        not_before: now - Duration::minutes(1),
        lifetime: Duration::days(days),
    };
    let certificate = ca
        .issue(&csr, &spec)
        .map_err(|e| RegistryError::Internal(e.to_string()))?;
    Ok((certificate, key.pkcs8_der()))
}

fn issue_identity(
    layout: &Layout,
    ca: &CertificateAuthority,
    registry_id: Uuid7,
    options: &InitOptions,
    now: OffsetDateTime,
) -> Result<(), RegistryError> {
    let (certificate, key) = leaf(ca, registry_id, options, now)?;
    files::write_secret(&layout.identity_key(), &key)?;
    files::write_public(&layout.identity_certificate(), &certificate)
}

fn issue_identity_replacing(
    layout: &Layout,
    ca: &CertificateAuthority,
    registry_id: Uuid7,
    options: &InitOptions,
    now: OffsetDateTime,
) -> Result<(), RegistryError> {
    let (certificate, key) = leaf(ca, registry_id, options, now)?;
    files::replace_secret(&layout.identity_key(), &key)?;
    files::write_public(&layout.identity_certificate(), &certificate)
}

fn to_ms(moment: OffsetDateTime) -> i64 {
    i64::try_from(moment.unix_timestamp_nanos() / 1_000_000).unwrap_or(0)
}
