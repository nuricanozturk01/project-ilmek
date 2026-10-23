use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid node configuration: {0}")]
    Invalid(String),
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("missing prerequisites: {}", .0.join(", "))]
    MissingPrerequisites(Vec<String>),
    #[error("applying changes needs root privileges")]
    NotRoot,
    #[error(
        "{0} carries this machine's default route or current management session; \
         refusing to reconfigure it (pass --allow-management-interface only with console access)"
    )]
    ManagementInterface(String),
    #[error("cannot read the passphrase file {path}: {reason}")]
    Passphrase { path: PathBuf, reason: String },
    #[error("snapshot {0} does not exist")]
    NoSnapshot(String),
    #[error("snapshot {0} was already rolled back")]
    AlreadyRolledBack(String),
    #[error("{what}: {source}")]
    Io {
        what: String,
        source: std::io::Error,
    },
    #[error("`{command}` failed: {reason}")]
    Command { command: String, reason: String },
    #[error("snapshot manifest is damaged: {0}")]
    Manifest(String),
    /// The change failed part-way. `rollback_problems` is empty when the undo succeeded.
    #[error("{cause}; automatic rollback of snapshot {snapshot} {outcome}", outcome = rollback_outcome(.rollback_problems))]
    Failed {
        cause: Box<ApplyError>,
        snapshot: String,
        rollback_problems: Vec<String>,
    },
    #[error("rollback finished with problems: {}", .0.join("; "))]
    RollbackIncomplete(Vec<String>),
}

fn rollback_outcome(problems: &[String]) -> String {
    if problems.is_empty() {
        "restored the previous state".to_owned()
    } else {
        format!("had problems: {}", problems.join("; "))
    }
}

impl ApplyError {
    pub(crate) fn io(what: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            what: what.into(),
            source,
        }
    }
}
