use std::io;
use std::sync::Arc;

use ilmek_host::{ExitReport, Killer, LaunchSpec, Launcher, RunningBackend};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

type Factory = dyn Fn(LaunchSpec) -> JoinHandle<()> + Send + Sync;

/// Runs backends as tasks inside the test process; "killing" one aborts its task.
pub struct InProcessLauncher {
    factory: Arc<Factory>,
}

impl InProcessLauncher {
    pub fn new(factory: impl Fn(LaunchSpec) -> JoinHandle<()> + Send + Sync + 'static) -> Self {
        Self {
            factory: Arc::new(factory),
        }
    }
}

impl std::fmt::Debug for InProcessLauncher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InProcessLauncher")
    }
}

impl Launcher for InProcessLauncher {
    fn launch(&self, spec: LaunchSpec) -> io::Result<RunningBackend> {
        let task = (self.factory)(spec);
        let abort = task.abort_handle();
        let (exit_tx, exit_rx) = oneshot::channel();
        tokio::spawn(async move {
            let result = task.await;
            let _ = exit_tx.send(ExitReport {
                success: result.is_ok(),
                detail: result
                    .err()
                    .map_or_else(|| "finished".into(), |e| e.to_string()),
            });
        });
        Ok(RunningBackend {
            pid: None,
            exited: exit_rx,
            killer: Killer::new(move || abort.abort()),
        })
    }
}
