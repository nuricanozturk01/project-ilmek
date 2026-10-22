//! First-time enrollment with a single-use token (PROTOCOL.md §3.2).

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use ilmek_core::session::{ClientParams, ClientSession};
use ilmek_core::tls::{ServerTarget, TlsClient};
use ilmek_identity::storage::{create_private_dir, write_secret};
use ilmek_identity::{CommunityProfile, DeviceKey, Identity, peer_id_from_certificate};
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Enroll, Message, Mode, ServerRole, Token, Version};
use serde::{Deserialize, Serialize};

use crate::device::Device;
use crate::error::Error;

const CLIENT_NAME: &str = "ilmek-client";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// The request that is retried verbatim if the answer is lost.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    operation_id: Uuid7,
    csr_der: Token,
}

impl Pending {
    /// Reuses an earlier attempt's key, CSR and operation id so a lost reply can be recovered.
    fn load_or_create(dir: &Path) -> Result<(Self, Vec<u8>), Error> {
        create_private_dir(dir)?;
        let record_path = dir.join("pending-enrollment.json");
        let key_path = dir.join("pending-enrollment.key.der");
        if record_path.exists() && key_path.exists() {
            let pending = serde_json::from_slice(&std::fs::read(&record_path)?)
                .map_err(|_| Error::Corrupt("pending-enrollment.json"))?;
            return Ok((pending, std::fs::read(&key_path)?));
        }
        let key = DeviceKey::generate()?;
        let pending = Self {
            operation_id: Uuid7::generate(),
            csr_der: Token::new(key.csr_der()?),
        };
        let key_der = key.pkcs8_der();
        write_secret(&key_path, &key_der)?;
        std::fs::write(
            &record_path,
            serde_json::to_vec(&pending).map_err(|_| Error::Corrupt("pending enrollment"))?,
        )?;
        Ok((pending, key_der))
    }

    fn discard(dir: &Path) {
        let _ = std::fs::remove_file(dir.join("pending-enrollment.json"));
        let _ = std::fs::remove_file(dir.join("pending-enrollment.key.der"));
    }
}

/// Enrolls this device and stores its identity in `dir`.
///
/// Calling again with the same `dir` and token after a lost reply returns the same certificate
/// instead of consuming a second token.
pub async fn enroll(
    profile: &CommunityProfile,
    token: &str,
    display_name: &str,
    dir: &Path,
) -> Result<Device, Error> {
    let token: Token = token
        .parse()
        .map_err(|_| Error::InvalidInput("token is not base64url"))?;
    let display_name = display_name
        .parse()
        .map_err(|_| Error::InvalidInput("display name too long"))?;
    let (pending, key_der) = Pending::load_or_create(dir)?;

    let tls = TlsClient::new(None, profile.ca_der())?;
    let target = ServerTarget {
        address: SocketAddr::V4(profile.registry_enroll),
        server_id: profile.registry_id,
    };
    let connection = tls.connect(target).await?;
    let session = ClientSession::open(
        connection.stream,
        ClientParams {
            mode: Mode::Enroll,
            community_id: profile.community_id,
            expected_server_id: profile.registry_id,
            expected_role: ServerRole::Registry,
            client_name: CLIENT_NAME.to_owned(),
            client_version: Version::new(1, 0, 0),
            request_timeout: REQUEST_TIMEOUT,
        },
    )
    .await?;

    let request = Message::Enroll(Enroll {
        operation_id: pending.operation_id,
        token,
        display_name,
        csr_der: pending.csr_der,
    });
    let reply = session.request(&request).await?;
    let Message::Enrolled(enrolled) = reply else {
        return Err(Error::UnexpectedReply("expected ENROLLED"));
    };

    let certificate = enrolled.certificate_der.into_bytes();
    if peer_id_from_certificate(&certificate)? != enrolled.peer_id {
        return Err(Error::UnexpectedReply(
            "certificate identity differs from the reply",
        ));
    }
    let device = Device {
        profile: profile.clone(),
        peer_id: enrolled.peer_id,
        roles: enrolled.roles,
        expires_at_ms: enrolled.expires_at_ms,
        identity: Identity {
            chain_der: vec![certificate],
            key_pkcs8_der: key_der,
        },
    };
    device.save(dir)?;
    Pending::discard(dir);
    session.close().await;
    Ok(device)
}
