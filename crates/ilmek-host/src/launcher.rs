//! Starting backend executables. Production uses real processes; tests may substitute
//! in-process backends through the same trait.

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use ilmek_app_sdk::BackendConfig;
use ilmek_protocol::Uuid7;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

use crate::logs::RotatingLog;

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub app_id: Uuid7,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
    /// Delivered over stdin, never on the command line or in the environment.
    pub config: BackendConfig,
}

#[derive(Debug, Clone)]
pub struct ExitReport {
    pub success: bool,
    pub detail: String,
}

/// Forcibly ends a backend that ignored the orderly shutdown.
pub struct Killer(Option<Box<dyn FnOnce() + Send + Sync>>);

impl Killer {
    #[must_use]
    pub fn new(kill: impl FnOnce() + Send + Sync + 'static) -> Self {
        Self(Some(Box::new(kill)))
    }

    pub fn kill(&mut self) {
        if let Some(kill) = self.0.take() {
            kill();
        }
    }
}

impl std::fmt::Debug for Killer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Killer")
    }
}

#[derive(Debug)]
pub struct RunningBackend {
    pub pid: Option<u32>,
    pub exited: oneshot::Receiver<ExitReport>,
    pub killer: Killer,
}

pub trait Launcher: Send + Sync + 'static {
    fn launch(&self, spec: LaunchSpec) -> io::Result<RunningBackend>;
}

/// Runs backends as operating-system child processes.
///
/// A separate process is not a sandbox: backends are code the host operator trusts.
#[derive(Debug, Default)]
pub struct ProcessLauncher;

impl Launcher for ProcessLauncher {
    fn launch(&self, spec: LaunchSpec) -> io::Result<RunningBackend> {
        std::fs::create_dir_all(&spec.data_dir)?;
        let log = Arc::new(RotatingLog::open(&spec.logs_dir, "backend.log")?);

        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.args)
            .current_dir(&spec.data_dir)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for inherited in ["PATH", "SYSTEMROOT", "TEMP", "TMP", "TMPDIR"] {
            if let Some(value) = std::env::var_os(inherited) {
                command.env(inherited, value);
            }
        }
        let mut child = command.spawn()?;
        let pid = child.id();

        let config = serde_json::to_vec(&spec.config).map_err(io::Error::other)?;
        let stdin = child.stdin.take();
        tokio::spawn(async move {
            if let Some(mut stdin) = stdin {
                let _ = stdin.write_all(&config).await;
            }
        });
        if let Some(out) = child.stdout.take() {
            tokio::spawn(pump(out, Arc::clone(&log), "out"));
        }
        if let Some(err) = child.stderr.take() {
            tokio::spawn(pump(err, Arc::clone(&log), "err"));
        }

        let (exit_tx, exit_rx) = oneshot::channel();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let status = tokio::select! {
                status = child.wait() => status,
                _ = kill_rx => {
                    let _ = child.kill().await;
                    child.wait().await
                }
            };
            let report = match status {
                Ok(status) => ExitReport {
                    success: status.success(),
                    detail: status.to_string(),
                },
                Err(error) => ExitReport {
                    success: false,
                    detail: error.to_string(),
                },
            };
            let _ = exit_tx.send(report);
        });
        Ok(RunningBackend {
            pid,
            exited: exit_rx,
            killer: Killer::new(move || {
                let _ = kill_tx.send(());
            }),
        })
    }
}

async fn pump(stream: impl AsyncRead + Unpin, log: Arc<RotatingLog>, label: &'static str) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        log.line(&format!("[{label}] {line}"));
    }
}
