//! `ENROLL`: token + CSR in, community certificate out (PROTOCOL.md §3.2, §8.1).

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use ilmek_identity::{LeafSpec, certificate_not_after_ms, certificate_serial_hex};
use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::{hex_lower, sha256};
use ilmek_protocol::wire::{Endpoint, Enroll, Enrolled, PeerRole, Token};
use ilmek_store::Unsigned;
use ilmek_store::rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use time::Duration;

use crate::error::RegistryError;
use crate::registry::Registry;
use crate::time_util::datetime_from_ms;

const LEAF_LIFETIME_DAYS: i64 = 30;
const MAX_CSR_BYTES: usize = 8192;
const MAX_TOKEN_BYTES: usize = 128;
const RESULT_RETENTION_MS: u64 = 24 * 60 * 60 * 1000;

struct TokenRow {
    roles: Vec<PeerRole>,
    bound_peer: Option<String>,
    server_ip: Option<Ipv4Addr>,
    expires_at_ms: u64,
}

struct Issued {
    certificate: Vec<u8>,
    serial: String,
    not_after_ms: u64,
}

impl Registry {
    pub(crate) async fn enroll(
        self: &Arc<Self>,
        remote: Option<IpAddr>,
        request: Enroll,
    ) -> Result<Enrolled, RegistryError> {
        let now = self.clock.now_ms();
        self.limiter.check(remote, now)?;

        let outcome = self.try_enroll(request).await;
        if matches!(outcome, Err(RegistryError::Unauthorized)) {
            self.limiter.record_failure(remote, now);
        }
        outcome
    }

    async fn try_enroll(self: &Arc<Self>, request: Enroll) -> Result<Enrolled, RegistryError> {
        if request.csr_der.as_bytes().len() > MAX_CSR_BYTES
            || request.token.as_bytes().len() > MAX_TOKEN_BYTES
        {
            return Err(RegistryError::Invalid("enrollment field too large"));
        }
        let token_hash = hex_lower(&sha256(request.token.as_bytes()));
        let csr_hash = hex_lower(&sha256(request.csr_der.as_bytes()));
        let registry = Arc::clone(self);

        self.db
            .call(move |connection| {
                let now = registry.clock.now_ms();
                let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if let Some(replay) = replayed(&tx, &request, &token_hash, &csr_hash)? {
                    return Ok(replay);
                }
                let row = open_token(&tx, &token_hash, now)?;
                let peer_id = enrolling_peer(&tx, row.bound_peer.as_deref())?;
                let issued = registry.issue(&request, peer_id, &row, now)?;

                store_peer(&tx, &request, peer_id, &row.roles, &issued, now)?;
                tx.execute(
                    "UPDATE enrollment_tokens SET consumed_at_ms = ?2 WHERE token_hash = ?1",
                    params![token_hash, Unsigned(now)],
                )?;
                let enrolled = registry.enrolled(peer_id, row.roles.clone(), issued);
                store_result(&tx, &request, &token_hash, &csr_hash, &enrolled, &row, now)?;
                tx.commit()?;
                Ok(enrolled)
            })
            .await
    }

    fn issue(
        &self,
        request: &Enroll,
        peer_id: Uuid7,
        row: &TokenRow,
        now: u64,
    ) -> Result<Issued, RegistryError> {
        let spec = LeafSpec {
            peer_id,
            server_ip: row.server_ip,
            not_before: datetime_from_ms(now.saturating_sub(60_000)),
            lifetime: Duration::days(LEAF_LIFETIME_DAYS),
        };
        let certificate = self
            .ca
            .issue(request.csr_der.as_bytes(), &spec)
            .map_err(|_| RegistryError::Invalid("certificate request rejected"))?;
        let internal = |e: ilmek_identity::PkiError| RegistryError::Internal(e.to_string());
        Ok(Issued {
            serial: certificate_serial_hex(&certificate).map_err(internal)?,
            not_after_ms: certificate_not_after_ms(&certificate).map_err(internal)?,
            certificate,
        })
    }

    fn enrolled(&self, peer_id: Uuid7, roles: Vec<PeerRole>, issued: Issued) -> Enrolled {
        Enrolled {
            peer_id,
            certificate_der: Token::new(issued.certificate),
            chain_der: vec![Token::new(self.ca.certificate_der().to_vec())],
            roles,
            registry_endpoint: Endpoint {
                ip: *self.profile.registry_control.ip(),
                port: self.profile.registry_control.port(),
            },
            registry_id: self.profile.registry_id,
            expires_at_ms: issued.not_after_ms,
        }
    }
}

/// A repeated request with identical token and CSR gets the original answer back.
fn replayed(
    tx: &Transaction<'_>,
    request: &Enroll,
    token_hash: &str,
    csr_hash: &str,
) -> Result<Option<Enrolled>, RegistryError> {
    let previous: Option<(String, String, String)> = tx
        .query_row(
            "SELECT token_hash, csr_hash, result_json FROM enrollments WHERE operation_id = ?1",
            params![request.operation_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((stored_token, stored_csr, result)) = previous else {
        return Ok(None);
    };
    if stored_token != token_hash || stored_csr != csr_hash {
        return Err(RegistryError::Unauthorized);
    }
    serde_json::from_str(&result)
        .map(Some)
        .map_err(|_| RegistryError::Internal("stored enrollment".into()))
}

fn open_token(tx: &Transaction<'_>, token_hash: &str, now: u64) -> Result<TokenRow, RegistryError> {
    type Columns = (
        String,
        Option<String>,
        Option<String>,
        Unsigned,
        Option<Unsigned>,
    );
    let columns: Option<Columns> = tx
        .query_row(
            "SELECT roles, bound_peer_id, server_ip, expires_at_ms, consumed_at_ms
             FROM enrollment_tokens WHERE token_hash = ?1",
            params![token_hash],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((roles, bound_peer, server_ip, Unsigned(expires_at_ms), None)) = columns else {
        return Err(RegistryError::Unauthorized);
    };
    if now >= expires_at_ms {
        return Err(RegistryError::Unauthorized);
    }
    Ok(TokenRow {
        roles: serde_json::from_str(&roles)
            .map_err(|_| RegistryError::Internal("token roles".into()))?,
        bound_peer,
        server_ip: server_ip
            .map(|ip| ip.parse())
            .transpose()
            .map_err(|_| RegistryError::Internal("token server ip".into()))?,
        expires_at_ms,
    })
}

/// A renewal keeps its bound identity unless that peer was revoked.
fn enrolling_peer(tx: &Transaction<'_>, bound: Option<&str>) -> Result<Uuid7, RegistryError> {
    let Some(bound) = bound else {
        return Ok(Uuid7::generate());
    };
    let revoked: Option<Option<i64>> = tx
        .query_row(
            "SELECT revoked_at_ms FROM peers WHERE peer_id = ?1",
            params![bound],
            |row| row.get(0),
        )
        .optional()?;
    if revoked.is_none_or(|at| at.is_some()) {
        return Err(RegistryError::Unauthorized);
    }
    bound
        .parse()
        .map_err(|_| RegistryError::Internal("bound peer".into()))
}

fn store_peer(
    tx: &Transaction<'_>,
    request: &Enroll,
    peer_id: Uuid7,
    roles: &[PeerRole],
    issued: &Issued,
    now: u64,
) -> Result<(), RegistryError> {
    let roles_json =
        serde_json::to_string(roles).map_err(|_| RegistryError::Internal("roles".into()))?;
    tx.execute(
        "INSERT INTO peers
             (peer_id, display_name, roles, cert_serial, cert_not_after_ms, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(peer_id) DO UPDATE SET
             display_name = excluded.display_name,
             roles = excluded.roles,
             cert_serial = excluded.cert_serial,
             cert_not_after_ms = excluded.cert_not_after_ms",
        params![
            peer_id.to_string(),
            request.display_name.as_str(),
            roles_json,
            issued.serial,
            Unsigned(issued.not_after_ms),
            Unsigned(now)
        ],
    )?;
    Ok(())
}

fn store_result(
    tx: &Transaction<'_>,
    request: &Enroll,
    token_hash: &str,
    csr_hash: &str,
    enrolled: &Enrolled,
    row: &TokenRow,
    now: u64,
) -> Result<(), RegistryError> {
    let result = serde_json::to_string(enrolled)
        .map_err(|_| RegistryError::Internal("enrollment result".into()))?;
    tx.execute(
        "INSERT INTO enrollments
             (operation_id, token_hash, csr_hash, result_json, retain_until_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            request.operation_id.to_string(),
            token_hash,
            csr_hash,
            result,
            Unsigned(row.expires_at_ms.max(now) + RESULT_RETENTION_MS)
        ],
    )?;
    Ok(())
}
