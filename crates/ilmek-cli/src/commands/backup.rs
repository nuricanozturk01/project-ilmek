//! `ilmek backup`: consistent backups of registry and host data (REQUIREMENTS §22).

use std::path::Path;

use ilmek_host::Settings as HostSettings;
use ilmek_registry::Settings as RegistrySettings;
use ilmek_store::{BackupReport, backup_tree, restore_tree, verify_backup};

use crate::cli::Backup;
use crate::error::{CliResult, Context};
use crate::output::{line, note};

pub(crate) fn run(command: &Backup) -> CliResult {
    match command {
        Backup::Registry {
            registry,
            out,
            include_keys,
        } => {
            let settings =
                RegistrySettings::load(&registry.config).context("registry configuration")?;
            let report = backup_tree(&settings.data_dir, out, *include_keys).context("backup")?;
            summarize(&report, out);
            Ok(())
        }
        Backup::Host { host, out } => {
            let settings = HostSettings::load(&host.config).context("host configuration")?;
            let report = backup_tree(&settings.data_dir, out, false).context("backup")?;
            summarize(&report, out);
            Ok(())
        }
        Backup::Verify { dir } => {
            let report = verify_backup(dir).context("verify")?;
            line(&format!(
                "ok: {} files ({} databases), {} bytes, keys {}",
                report.files,
                report.databases,
                report.bytes,
                if report.keys_included {
                    "included"
                } else {
                    "not included"
                }
            ));
            Ok(())
        }
        Backup::RestoreRegistry { registry, from } => {
            let settings =
                RegistrySettings::load(&registry.config).context("registry configuration")?;
            restore(from, &settings.data_dir, true)
        }
        Backup::RestoreHost { host, from } => {
            let settings = HostSettings::load(&host.config).context("host configuration")?;
            restore(from, &settings.data_dir, false)
        }
    }
}

fn summarize(report: &BackupReport, out: &Path) {
    line(&format!(
        "backup written to {}: {} files ({} databases), {} bytes",
        out.display(),
        report.files,
        report.databases,
        report.bytes
    ));
    if report.keys_included {
        note("this backup contains private keys: store it encrypted and apart from log archives");
    }
    for key in &report.withheld_keys {
        note(&format!(
            "private key not included: {key} (use --include-keys to keep it)"
        ));
    }
}

fn restore(from: &Path, data_dir: &Path, needs_registry_keys: bool) -> CliResult {
    let report = restore_tree(from, data_dir).context("restore")?;
    line(&format!(
        "restored {} files ({} databases) into {}",
        report.files,
        report.databases,
        data_dir.display()
    ));
    if needs_registry_keys && !report.keys_included {
        note(
            "the backup holds no private keys: copy them in separately before starting a registry",
        );
    }
    Ok(())
}
