//! `ilmek network`: plan, apply and roll back a routing node's Wi-Fi/IP setup.

use std::path::{Path, PathBuf};

use ilmek_network::{
    ApplyOptions, Environment, LinuxProbe, NodeConfig, Phase, ProcessRunner, apply, plan, rollback,
    snapshots,
};

use crate::cli::{Network, SnapshotDir};
use crate::error::{CliError, CliResult, Context};
use crate::output::{line, note};

const DEFAULT_SNAPSHOT_DIR: &str = "/var/lib/ilmek/network";

pub(crate) fn run(command: Network) -> CliResult {
    match command {
        Network::Plan { node } => {
            let config = NodeConfig::load(&node.config).context("node configuration")?;
            line(&plan(&config, &LinuxProbe).to_string());
            Ok(())
        }
        Network::Apply {
            node,
            snapshot_dir,
            yes,
            allow_management_interface,
        } => apply_plan(&node.config, snapshot_dir, yes, allow_management_interface),
        Network::Rollback {
            snapshots: dir,
            snapshot,
        } => rollback_snapshot(&resolve_dir(&dir)?, snapshot),
        Network::Status { snapshots: dir } => {
            status(&resolve_dir(&dir)?);
            Ok(())
        }
    }
}

fn resolve_dir(dir: &SnapshotDir) -> CliResult<PathBuf> {
    if let Some(explicit) = &dir.snapshot_dir {
        return Ok(explicit.clone());
    }
    match &dir.config {
        Some(path) => Ok(NodeConfig::load(path)
            .context("node configuration")?
            .snapshot_dir),
        None => Ok(PathBuf::from(DEFAULT_SNAPSHOT_DIR)),
    }
}

fn apply_plan(
    config: &Path,
    snapshot_dir: Option<PathBuf>,
    yes: bool,
    allow_management: bool,
) -> CliResult {
    let config = NodeConfig::load(config).context("node configuration")?;
    let snapshot_dir = snapshot_dir.unwrap_or_else(|| config.snapshot_dir.clone());
    let built = plan(&config, &LinuxProbe);
    line(&built.to_string());
    if !yes {
        return Err(CliError::new(
            "nothing was changed: review the plan above and repeat with --yes to apply it",
        ));
    }
    let env = Environment {
        root: Path::new("/"),
        probe: &LinuxProbe,
        runner: &ProcessRunner,
    };
    let options = ApplyOptions {
        allow_management_interface: allow_management,
    };
    let applied = apply(&built, &snapshot_dir, env, &options).context("apply")?;
    note(&format!(
        "applied. To undo: ilmek network rollback --snapshot {}",
        applied.snapshot_id
    ));
    line(&applied.snapshot_id);
    Ok(())
}

fn rollback_snapshot(snapshot_dir: &Path, snapshot: Option<String>) -> CliResult {
    let id = match snapshot {
        Some(id) => id,
        None => snapshots(snapshot_dir, Path::new("/"))
            .into_iter()
            .rev()
            .find(|s| s.phase != Phase::RolledBack)
            .map(|s| s.id)
            .ok_or_else(|| CliError::new("no snapshot to roll back"))?,
    };
    let env = Environment {
        root: Path::new("/"),
        probe: &LinuxProbe,
        runner: &ProcessRunner,
    };
    rollback(snapshot_dir, &id, env).context("rollback")?;
    note(&format!("rolled back snapshot {id}"));
    Ok(())
}

fn status(snapshot_dir: &Path) {
    let found = snapshots(snapshot_dir, Path::new("/"));
    if found.is_empty() {
        line("no snapshots: this machine has not been configured by ilmek");
    }
    for s in found {
        line(&format!(
            "{}\tnode={}\tcreated_unix={}\t{}",
            s.id, s.node, s.created_unix, s.phase
        ));
    }
}
