use std::sync::Arc;

use ilmek_core::clock::SystemClock;
use ilmek_identity::PublisherKey;
use ilmek_protocol::wire::PeerRole;
use ilmek_registry::{
    InitOptions, InviteRequest, Registry, Settings, initialize, reissue_identity,
};
use time::OffsetDateTime;

use super::uuid;
use crate::cli::Community;
use crate::error::{CliError, CliResult, Context};
use crate::output::{line, note};

fn open(config: &std::path::Path) -> CliResult<Arc<Registry>> {
    let settings = Settings::load(config).context("registry configuration")?;
    Registry::open(settings, Arc::new(SystemClock))
        .context("cannot open the registry (run `community init` first?)")
}

fn role(text: &str) -> CliResult<PeerRole> {
    match text {
        "member" => Ok(PeerRole::Member),
        "publisher" => Ok(PeerRole::Publisher),
        "host" => Ok(PeerRole::Host),
        "admin" => Ok(PeerRole::Admin),
        other => Err(CliError::new(format!(
            "unknown role `{other}` (member, publisher, host, admin)"
        ))),
    }
}

pub(crate) fn run(command: Community) -> CliResult {
    match command {
        Community::Init {
            registry,
            advertise_ip,
            identity_days,
        } => init(&registry.config, advertise_ip, identity_days),
        Community::Invite {
            registry,
            role: roles,
            server_ip,
            bound_peer,
            ttl_seconds,
        } => {
            let request = InviteRequest {
                roles: roles.iter().map(|r| role(r)).collect::<CliResult<_>>()?,
                bound_peer: bound_peer
                    .as_deref()
                    .map(|p| uuid(p, "bound peer"))
                    .transpose()?,
                server_ip,
                ttl_seconds,
            };
            invite(&registry.config, &request)
        }
        Community::Peers { registry } => peers(&registry.config),
        Community::RevokePeer { registry, peer } => {
            open(&registry.config)?
                .revoke_peer(uuid(&peer, "peer")?)
                .context("cannot revoke")?;
            note(
                "Revoked. Hosts and clients pick this up on their next trust refresh (at most a minute online).",
            );
            Ok(())
        }
        Community::AddPublisher { registry, name } => {
            line(
                &open(&registry.config)?
                    .add_publisher(&name)
                    .context("cannot add the publisher")?
                    .to_string(),
            );
            Ok(())
        }
        Community::AddPublisherDevice {
            registry,
            publisher,
            peer,
        } => open(&registry.config)?
            .add_publisher_device(uuid(&publisher, "publisher")?, uuid(&peer, "peer")?)
            .context("cannot link the device"),
        Community::AddPublisherKey {
            registry,
            publisher,
            key,
        } => {
            let key = PublisherKey::from_base64url(&key).context("not a valid public key")?;
            line(
                &open(&registry.config)?
                    .add_publisher_key(uuid(&publisher, "publisher")?, &key)
                    .context("cannot add the key")?,
            );
            Ok(())
        }
        Community::RevokePublisherKey { registry, key_id } => open(&registry.config)?
            .revoke_publisher_key(&key_id)
            .context("cannot revoke the key"),
        Community::GrantNamespace {
            registry,
            publisher,
            namespace,
        } => open(&registry.config)?
            .grant_namespace(
                &namespace.parse().context("namespace")?,
                uuid(&publisher, "publisher")?,
            )
            .context("cannot grant the namespace"),
        Community::GrantHost {
            registry,
            publisher,
            host,
        } => open(&registry.config)?
            .grant_host(uuid(&publisher, "publisher")?, uuid(&host, "host")?)
            .context("cannot grant the host"),
        Community::ReissueIdentity {
            registry,
            advertise_ip,
        } => reissue(&registry.config, advertise_ip),
    }
}

fn init(
    config: &std::path::Path,
    advertise_ip: std::net::Ipv4Addr,
    identity_days: i64,
) -> CliResult {
    let settings = Settings::load(config).context("registry configuration")?;
    let options = InitOptions {
        advertised_ip: advertise_ip,
        identity_lifetime_days: Some(identity_days),
    };
    let profile = initialize(&settings, &options, OffsetDateTime::now_utc())
        .context("initialization failed")?;
    note(&format!(
        "Community created. Profile: {}/community.json",
        settings.data_dir.display()
    ));
    note(
        "Give the profile file to new devices, and tell them this CA fingerprint over a trusted channel:",
    );
    line(&profile.ca_fingerprint());
    Ok(())
}

fn invite(config: &std::path::Path, request: &InviteRequest) -> CliResult {
    let token = open(config)?
        .create_invite(request)
        .context("cannot create the invitation")?;
    note(
        "Single-use token (valid 15 minutes unless --ttl-seconds is set). It is not stored; keep it secret:",
    );
    line(&token);
    Ok(())
}

fn peers(config: &std::path::Path) -> CliResult {
    for peer in open(config)?.list_peers().context("cannot list peers")? {
        let roles: Vec<String> = peer
            .roles
            .iter()
            .map(|r| format!("{r:?}").to_lowercase())
            .collect();
        line(&format!(
            "{} {} [{}]{}",
            peer.peer_id,
            peer.display_name,
            roles.join(","),
            if peer.revoked { " REVOKED" } else { "" }
        ));
    }
    Ok(())
}

fn reissue(config: &std::path::Path, advertise_ip: std::net::Ipv4Addr) -> CliResult {
    let settings = Settings::load(config).context("registry configuration")?;
    let options = InitOptions {
        advertised_ip: advertise_ip,
        identity_lifetime_days: None,
    };
    reissue_identity(&settings, &options, OffsetDateTime::now_utc()).context("cannot reissue")?;
    note("New registry certificate written. Restart `ilmek registry serve`.");
    Ok(())
}
