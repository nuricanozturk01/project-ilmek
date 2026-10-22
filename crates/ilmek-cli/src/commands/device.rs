use std::sync::Arc;
use std::time::Instant;

use ilmek_client::{
    AppClient, Connector, Error, PackageCache, RegistryClient, Resolver, Source, fetch_package,
};
use ilmek_core::clock::SystemClock;
use ilmek_core::tls::TlsError;
use ilmek_protocol::address::Address;
use ilmek_protocol::wire::{AppRecord, Mode};
use tokio_util::sync::CancellationToken;

use super::connect_device;
use crate::cli::{Call, DeviceAddress, Fetch};
use crate::error::{CliError, CliResult, Context};
use crate::output::{line, note};

fn address(text: &str) -> CliResult<Address> {
    text.parse().context("invalid address")
}

async fn resolver(device: &std::path::Path) -> CliResult<(Arc<Connector>, Resolver)> {
    let connector = Arc::new(connect_device(device).await?);
    let resolver = Resolver::new(
        Arc::clone(&connector),
        Arc::new(SystemClock),
        Some(device.join("resolve-cache.json")),
    );
    Ok((connector, resolver))
}

fn print_record(record: &AppRecord, source: Source) {
    line(&format!(
        "address      ilmek://{}/{}",
        record.namespace, record.slug
    ));
    line(&format!("app id       {}", record.app_id));
    line(&format!(
        "host          {} at {}:{}",
        record.server_id, record.endpoint.ip, record.endpoint.port
    ));
    line(&format!(
        "version      {} (revision {})",
        record.version, record.revision
    ));
    line(&format!("manifest     {}", record.manifest_hash));
    line(&format!("availability {:?}", record.availability));
    if source == Source::StaleCache {
        line("source       STALE CACHE: the registry could not be reached");
    }
}

pub(crate) async fn resolve(args: &DeviceAddress) -> CliResult {
    let (_, resolver) = resolver(&args.device).await?;
    let resolution = resolver
        .resolve(&address(&args.address)?)
        .await
        .map_err(|e| CliError::new(explain(&e)))?;
    print_record(&resolution.record, resolution.source);
    Ok(())
}

/// A human reason for a failure, telling "no network" from "no such name" from "wrong peer".
fn explain(error: &Error) -> String {
    match error {
        Error::Tls(TlsError::Connect(cause)) => {
            format!(
                "TCP connection failed ({cause}): no route, the machine is down, or this is the wrong network"
            )
        }
        Error::Tls(TlsError::ConnectTimeout) => {
            "TCP connection timed out: no route, or a firewall is dropping the traffic".to_owned()
        }
        Error::Tls(TlsError::Certificate(reason)) => {
            format!(
                "the peer's certificate was rejected ({reason:?}); check this device's clock and enrollment"
            )
        }
        Error::Tls(TlsError::UnexpectedPeer { .. }) => {
            "a different machine answered than the registry names; refusing to talk to it"
                .to_owned()
        }
        Error::Tls(TlsError::AlpnMismatch | TlsError::Handshake(_)) => {
            "the endpoint did not complete an ILMEK/1 TLS handshake".to_owned()
        }
        Error::TrustStale => {
            "trust information is missing or older than 24 hours; reach the registry to refresh it"
                .to_owned()
        }
        Error::PeerRevoked => "the peer's certificate has been revoked".to_owned(),
        other => match other.remote() {
            Some(remote) => format!("the server refused: {:?} ({})", remote.code, remote.message),
            None => other.to_string(),
        },
    }
}

fn step_ok(label: &str, detail: &str, started: Instant) {
    line(&format!(
        "[ok]   {label}: {detail} ({} ms)",
        started.elapsed().as_millis()
    ));
}

fn step_failed(label: &str, reason: &str) -> CliError {
    line(&format!("[FAIL] {label}: {reason}"));
    CliError::new(format!("{label} failed"))
}

pub(crate) async fn diagnose(args: &DeviceAddress) -> CliResult {
    let started = Instant::now();
    let wanted = address(&args.address).map_err(|e| step_failed("address", &e.to_string()))?;
    step_ok("address", &wanted.to_string(), started);

    let started = Instant::now();
    let device = ilmek_client::Device::load(&args.device)
        .map_err(|e| step_failed("device identity", &e.to_string()))?;
    let connector = Arc::new(
        Connector::new(device, Arc::new(SystemClock))
            .map_err(|e| step_failed("device identity", &e.to_string()))?,
    );
    step_ok(
        "device identity",
        &connector.device().peer_id.to_string(),
        started,
    );

    let started = Instant::now();
    let registry = RegistryClient::connect(&connector)
        .await
        .map_err(|e| step_failed("registry", &explain(&e)))?;
    step_ok(
        "registry",
        &connector.device().profile.registry_control.to_string(),
        started,
    );

    let started = Instant::now();
    registry
        .refresh_trust(connector.trust())
        .await
        .map_err(|e| step_failed("trust snapshot", &explain(&e)))?;
    step_ok("trust snapshot", "revocation list is current", started);

    let started = Instant::now();
    let record = registry
        .resolve(&wanted.namespace, &wanted.slug)
        .await
        .map_err(|e| step_failed("name", &explain(&e)))?;
    registry.close().await;
    step_ok(
        "name",
        &format!("{:?}, version {}", record.availability, record.version),
        started,
    );

    let started = Instant::now();
    let address = std::net::SocketAddr::new(record.endpoint.ip.into(), record.endpoint.port);
    let session = connector
        .connect_host(address, record.server_id, Mode::Control)
        .await
        .map_err(|e| step_failed("host", &explain(&e)))?;
    step_ok(
        "host",
        &format!("{address} answered as {}", record.server_id),
        started,
    );
    session.close().await;

    let started = Instant::now();
    let cache = PackageCache::new(
        std::env::temp_dir().join("ilmek-diagnose"),
        64 * 1024 * 1024,
    );
    fetch_package(
        &connector,
        &record,
        &cache,
        |_| {},
        &CancellationToken::new(),
    )
    .await
    .map_err(|e| step_failed("package", &explain(&e)))?;
    step_ok("package", "downloaded and verified", started);
    cache.evict(&record).context("cleaning up")?;
    Ok(())
}

pub(crate) async fn fetch(args: &Fetch) -> CliResult {
    let (connector, resolver) = resolver(&args.device).await?;
    let resolution = resolver
        .resolve(&address(&args.address)?)
        .await
        .map_err(|e| CliError::new(explain(&e)))?;
    let cache = PackageCache::new(args.cache.clone(), args.quota_mib * 1024 * 1024);
    let mut last_percent = 101;
    let package = fetch_package(
        &connector,
        &resolution.record,
        &cache,
        |progress| {
            let percent = progress.bytes_done * 100 / progress.bytes_total.max(1);
            if percent / 10 != last_percent / 10 {
                note(&format!(
                    "{percent}% ({} of {} bytes)",
                    progress.bytes_done, progress.bytes_total
                ));
                last_percent = percent;
            }
        },
        &CancellationToken::new(),
    )
    .await
    .map_err(|e| CliError::new(explain(&e)))?;
    line(&package.dir.display().to_string());
    Ok(())
}

pub(crate) async fn call(args: &Call) -> CliResult {
    let (connector, resolver) = resolver(&args.device).await?;
    let resolution = resolver
        .resolve(&address(&args.address)?)
        .await
        .map_err(|e| CliError::new(explain(&e)))?;
    serde_json::from_str::<serde_json::Value>(&args.body).context("--body is not JSON")?;
    let record = resolution.record;
    let version = record.version;
    let client = AppClient::new(connector, record, version);
    let result = if args.write {
        client.write(&args.method, args.body.as_bytes(), None).await
    } else {
        client.read(&args.method, args.body.as_bytes()).await
    }
    .map_err(|e| CliError::new(explain(&e)))?;
    line(&String::from_utf8_lossy(&result.body));
    Ok(())
}
