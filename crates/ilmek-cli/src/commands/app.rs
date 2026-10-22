use std::sync::Arc;

use ilmek_client::Device;
use ilmek_core::clock::SystemClock;
use ilmek_host::{
    BackendRow, Desired, Installer, Settings, Store, publish, sample_stats, unpublish,
};
use ilmek_package::{AppSpec, init_app, pack_spec};
use ilmek_protocol::Uuid7;
use ilmek_protocol::address::Address;
use ilmek_protocol::wire::Version;

use super::{connect_device, publisher::load_signer, uuid};
use crate::cli::{App, HostConfig};
use crate::error::{CliError, CliResult, Context};
use crate::output::{line, note};

struct HostHandle {
    settings: Settings,
    store: Store,
    installer: Installer,
}

fn open(config: &HostConfig) -> CliResult<HostHandle> {
    let settings = Settings::load(&config.config).context("host configuration")?;
    std::fs::create_dir_all(&settings.data_dir).context("cannot create the data directory")?;
    let store = Store::open(&settings.database_path()).context("cannot open the host database")?;
    let installer = Installer::new(
        settings.data_dir.clone(),
        store.clone(),
        settings.max_package_bytes,
        Arc::new(SystemClock),
    );
    Ok(HostHandle {
        settings,
        store,
        installer,
    })
}

/// Accepts an application id, `ilmek://ns/slug` or `ns/slug`.
async fn find(handle: &HostHandle, text: &str) -> CliResult<Uuid7> {
    if let Ok(id) = text.parse::<Uuid7>() {
        return Ok(id);
    }
    let address: Address = if text.starts_with("ilmek://") {
        text.parse()
    } else {
        format!("ilmek://{text}").parse()
    }
    .context("not an application id or address")?;
    let apps = handle
        .store
        .apps()
        .await
        .context("cannot read applications")?;
    apps.iter()
        .find(|a| a.namespace == address.namespace && a.slug == address.slug)
        .map(|a| a.app_id)
        .ok_or_else(|| CliError::new(format!("no installed application at {address}")))
}

pub(crate) async fn run(command: App) -> CliResult {
    match command {
        App::Init {
            dir,
            namespace,
            slug,
            publisher_id,
        } => scaffold(&dir, &namespace, &slug, &publisher_id),
        App::Pack {
            source,
            key,
            out,
            publisher_id,
        } => package(&source, &key, &out, publisher_id.as_deref()),
        App::Install { host, package } => install(&host, &package).await,
        App::Update { host, package } => update(&host, &package).await,
        App::Rollback { host, app, version } => {
            let handle = open(&host)?;
            let id = find(&handle, &app).await?;
            let version: Version = version.parse().context("version")?;
            handle
                .installer
                .rollback(id, version)
                .await
                .context("rollback failed")
        }
        App::Remove {
            host,
            app,
            purge_data,
        } => remove(&host, &app, purge_data).await,
        App::Start { host, app } => desire(&host, &app, Desired::Running, false).await,
        App::Stop { host, app } => desire(&host, &app, Desired::Stopped, false).await,
        App::Restart { host, app } => desire(&host, &app, Desired::Running, true).await,
        App::List { host } => list(&host).await,
        App::SetBackend {
            host,
            app,
            exe,
            args,
            accept_versions,
        } => set_backend(&host, &app, &exe, args, &accept_versions).await,
        App::Publish { host, app, device } => publish_app(&host, &app, &device).await,
        App::Unpublish { host, app, device } => {
            let handle = open(&host)?;
            let id = find(&handle, &app).await?;
            let publisher = connect_device(&device).await?;
            unpublish(&publisher, &handle.store, id)
                .await
                .context("unpublish failed")
        }
    }
}

fn scaffold(dir: &std::path::Path, namespace: &str, slug: &str, publisher_id: &str) -> CliResult {
    let spec = init_app(
        dir,
        namespace.parse().context("namespace")?,
        slug.parse().context("slug")?,
        uuid(publisher_id, "publisher id")?,
    )
    .context("cannot create the project")?;
    note(&format!(
        "Created {} (app id {})",
        dir.display(),
        spec.app_id
    ));
    Ok(())
}

fn package(
    source: &std::path::Path,
    key: &std::path::Path,
    out: &std::path::Path,
    publisher_id: Option<&str>,
) -> CliResult {
    let signer = load_signer(key)?;
    let mut spec = AppSpec::load(&source.join("app.toml")).context("cannot read app.toml")?;
    if let Some(id) = publisher_id {
        spec.publisher_id = uuid(id, "publisher id")?;
    }
    let packed = pack_spec(spec, source, &signer, out).context("packing failed")?;
    note(&format!(
        "Packed {} files into {}",
        packed.manifest.files.len(),
        out.display()
    ));
    line(&packed.manifest_hash.to_string());
    Ok(())
}

async fn install(host: &HostConfig, package: &std::path::Path) -> CliResult {
    let installed = open(host)?
        .installer
        .install(package)
        .await
        .context("install failed")?;
    line(&format!("{} {}", installed.app_id, installed.version));
    note("Installed but stopped. Configure a backend if needed, then `ilmek app start`.");
    Ok(())
}

async fn update(host: &HostConfig, package: &std::path::Path) -> CliResult {
    let installed = open(host)?
        .installer
        .update(package)
        .await
        .context("update failed")?;
    line(&format!(
        "{} {}{}",
        installed.app_id,
        installed.version,
        if installed.changed {
            ""
        } else {
            " (unchanged)"
        }
    ));
    Ok(())
}

async fn remove(host: &HostConfig, app: &str, purge_data: bool) -> CliResult {
    let handle = open(host)?;
    let id = find(&handle, app).await?;
    handle
        .installer
        .remove(id, purge_data)
        .await
        .context("remove failed")?;
    note(if purge_data {
        "Removed, data deleted."
    } else {
        "Removed. Data was kept; pass --purge-data to delete it."
    });
    Ok(())
}

async fn set_backend(
    host: &HostConfig,
    app: &str,
    exe: &std::path::Path,
    args: Vec<String>,
    accept_versions: &[String],
) -> CliResult {
    let handle = open(host)?;
    let id = find(&handle, app).await?;
    let executable = std::fs::canonicalize(exe).context("backend executable not found")?;
    if !executable.is_file() {
        return Err(CliError::new("backend executable is not a file"));
    }
    let accepted = accept_versions
        .iter()
        .map(|v| v.parse::<Version>())
        .collect::<Result<Vec<_>, _>>()
        .context("accept-version")?;
    handle
        .store
        .set_backend(
            id,
            BackendRow {
                executable,
                args,
                accepted_versions: accepted,
            },
        )
        .await
        .context("cannot save the backend")
}

async fn publish_app(host: &HostConfig, app: &str, device: &std::path::Path) -> CliResult {
    let handle = open(host)?;
    let id = find(&handle, app).await?;
    let host_device = Device::load(&handle.settings.device_dir).context("host device identity")?;
    let publisher = connect_device(device).await?;
    let record = publish(&publisher, &handle.store, id, host_device.peer_id)
        .await
        .context("publishing failed")?;
    line(&format!(
        "ilmek://{}/{} revision {} version {}",
        record.namespace, record.slug, record.revision, record.version
    ));
    Ok(())
}

async fn desire(host: &HostConfig, app: &str, desired: Desired, restart: bool) -> CliResult {
    let handle = open(host)?;
    let id = find(&handle, app).await?;
    handle
        .store
        .set_desired(id, desired, restart)
        .await
        .context("cannot change the desired state")?;
    note("Recorded. A running `ilmek host serve` applies it within a second.");
    Ok(())
}

fn bytes(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

async fn list(host: &HostConfig) -> CliResult {
    let handle = open(host)?;
    for app in handle
        .store
        .apps()
        .await
        .context("cannot read applications")?
    {
        let status = handle
            .store
            .status(app.app_id)
            .await
            .context("cannot read status")?;
        let (state, pid, restart_count, last_error) = status
            .map_or(("unknown".to_owned(), None, 0, None), |s| {
                (s.state.as_str().to_owned(), s.pid, s.restarts, s.last_error)
            });
        let usage = sample_stats(pid, &handle.installer.data_dir_of(app.app_id));
        line(&format!(
            "{} ilmek://{}/{} v{} desired={:?} state={} pid={} restarts={} rss={} cpu_ms={} data_bytes={}{}",
            app.app_id,
            app.namespace,
            app.slug,
            app.active_version,
            app.desired,
            state,
            pid.map_or_else(|| "-".to_owned(), |p| p.to_string()),
            restart_count,
            bytes(usage.rss_bytes),
            bytes(usage.cpu_ms),
            usage.data_bytes,
            last_error.map_or_else(String::new, |e| format!(" error=\"{e}\"")),
        ));
    }
    Ok(())
}
