use std::collections::HashMap;
use std::sync::Arc;

use ilmek_protocol::Uuid7;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use crate::slots::{AppSlot, Slots};
use crate::supervisor::{Env, Supervisor};

/// Keeps one supervisor running for every installed application.
pub struct Manager {
    slots: Arc<Slots>,
    shutdown: CancellationToken,
    task: JoinHandle<()>,
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager").finish_non_exhaustive()
    }
}

struct Running {
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl Manager {
    pub(crate) fn start(env: Arc<Env>) -> Self {
        let slots = Arc::new(Slots::new(
            env.settings.app_max_inflight,
            env.settings.app_queue_depth,
        ));
        let shutdown = CancellationToken::new();
        let task = tokio::spawn(reconcile_loop(env, Arc::clone(&slots), shutdown.clone()));
        Self {
            slots,
            shutdown,
            task,
        }
    }

    /// A shareable handle for services that only need to look slots up.
    pub(crate) fn handle(&self) -> Arc<Slots> {
        Arc::clone(&self.slots)
    }

    #[must_use]
    pub fn slot(&self, app_id: Uuid7) -> Option<Arc<AppSlot>> {
        self.slots.get(app_id)
    }

    /// Stops every backend (orderly first) and waits for the supervisors to finish.
    pub async fn shutdown(self) {
        self.shutdown.cancel();
        let _ = self.task.await;
    }
}

async fn reconcile_loop(env: Arc<Env>, slots: Arc<Slots>, shutdown: CancellationToken) {
    let mut running: HashMap<Uuid7, Running> = HashMap::new();
    let mut tick = interval(env.settings.reconcile_interval());
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = tick.tick() => {}
        }
        let Ok(apps) = env.store.apps().await else {
            continue;
        };
        for app in &apps {
            if !running.contains_key(&app.app_id) {
                let slot = slots.ensure(app.app_id);
                let cancel = CancellationToken::new();
                let supervisor = Supervisor::new(Arc::clone(&env), app.app_id, slot);
                let task = tokio::spawn(supervisor.run(cancel.clone()));
                running.insert(app.app_id, Running { cancel, task });
            }
        }
        let finished: Vec<Uuid7> = running
            .iter()
            .filter(|(id, r)| r.task.is_finished() && !apps.iter().any(|a| a.app_id == **id))
            .map(|(id, _)| *id)
            .collect();
        for id in finished {
            running.remove(&id);
            slots.remove(id);
        }
    }
    for entry in running.values() {
        entry.cancel.cancel();
    }
    for entry in running.into_values() {
        let _ = entry.task.await;
    }
}
