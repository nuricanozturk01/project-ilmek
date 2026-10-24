//! `ilmek backup`: consistent backups of registry and host data (REQUIREMENTS §22).

use std::path::Path;

use ilmek_host::Settings as HostSettings;
use ilmek_registry::Settings as RegistrySettings;
use ilmek_store::{BackupReport, backup_tree, restore_tree, verify_backup};

use super::{host_config, registry_config};
use crate::cli::Backup;
use crate::error::{CliResult, Context};
use crate::output::line;

pub(crate) fn run(command: &Backup) -> CliResult {
    match command {
        Backup::Registry { registry, out } => {
            let settings = RegistrySettings::load(&registry_config(registry))
                .context("registry configuration")?;
            let report = backup_tree(&settings.data_dir, out, false).context("backup")?;
            summarize(&report, out);
            Ok(())
        }
        Backup::Host { host, out } => {
            let settings = HostSettings::load(&host_config(host)).context("host configuration")?;
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
            let settings = RegistrySettings::load(&registry_config(registry))
                .context("registry configuration")?;
            restore(from, &settings.data_dir)
        }
        Backup::RestoreHost { host, from } => {
            let settings = HostSettings::load(&host_config(host)).context("host configuration")?;
            restore(from, &settings.data_dir)
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
}

fn restore(from: &Path, data_dir: &Path) -> CliResult {
    let report = restore_tree(from, data_dir).context("restore")?;
    line(&format!(
        "restored {} files ({} databases) into {}",
        report.files,
        report.databases,
        data_dir.display()
    ));
    Ok(())
}
