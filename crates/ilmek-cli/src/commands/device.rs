use std::sync::Arc;
use std::time::Instant;

use ilmek_client::{
    AppClient, Connector, Device, Error, PackageCache, RegistryClient, Resolver, Source,
    fetch_package,
};
use ilmek_core::clock::SystemClock;
use ilmek_core::community::CommunityProfile;
use ilmek_core::net::ConnectError;
use ilmek_protocol::address::Address;
use ilmek_protocol::wire::{AppRecord, Mode};
use tokio_util::sync::CancellationToken;

use super::{connect_device, device_dir, find_profile, home};
use crate::cli::{Call, DeviceAddress, DeviceOpt, Fetch, Join};
use crate::error::{CliError, CliResult, Context};
use crate::output::{line, note};

fn address(text: &str) -> CliResult<Address> {
    text.parse().context("invalid address")
}

async fn resolver(device: &DeviceOpt) -> CliResult<(Arc<Connector>, Resolver)> {
    let connector = Arc::new(connect_device(device).await?);
    let resolver = Resolver::new(
        Arc::clone(&connector),
        Arc::new(SystemClock),
        Some(device_dir(device).join("resolve-cache.json")),
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

/// A human reason for a failure, telling "no network" from "no such name".
fn explain(error: &Error) -> String {
    match error {
        Error::Connect(ConnectError::Connect(cause)) => {
            format!(
                "TCP connection failed ({cause}): no route, the machine is down, or this is the wrong network"
            )
        }
        Error::Connect(ConnectError::Timeout) => {
            "TCP connection timed out: no route, or a firewall is dropping the traffic".to_owned()
        }
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
    let connector = Arc::new(
        connect_device(&args.device)
            .await
            .map_err(|e| step_failed("device", &e.to_string()))?,
    );
    step_ok("device", &connector.device().peer_id.to_string(), started);

    let started = Instant::now();
    let registry = RegistryClient::connect(&connector)
        .await
        .map_err(|e| step_failed("registry", &explain(&e)))?;
    step_ok(
        "registry",
        &connector.device().profile.registry.to_string(),
        started,
    );

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
    let address = connector.host_address(&record.endpoint);
    let session = connector
        .connect_host(address, Mode::Control)
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
    let cache_dir = args
        .cache
        .clone()
        .unwrap_or_else(|| home(None).join("cache"));
    let cache = PackageCache::new(cache_dir, args.quota_mib * 1024 * 1024);
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

pub(crate) async fn join(args: &Join) -> CliResult {
    let dir = device_dir(&args.device);
    let profile = match &args.profile {
        Some(path) => CommunityProfile::load(path).context("cannot read the profile")?,
        None => find_profile(args.device.registry).await?,
    };
    let joined = Device::join(&profile, &dir).context("cannot join")?;
    line(&format!("device {}", joined.peer_id));
    note(&format!("Joined. Device state is in {}.", dir.display()));
    Ok(())
}

pub(crate) fn whoami(opt: &DeviceOpt) -> CliResult {
    let device = Device::load(&device_dir(opt)).context("not joined yet (run `ilmek join`)")?;
    line(&format!("device    {}", device.peer_id));
    line(&format!("community {}", device.profile.community_id));
    line(&format!("registry  {}", device.profile.registry));
    Ok(())
}
