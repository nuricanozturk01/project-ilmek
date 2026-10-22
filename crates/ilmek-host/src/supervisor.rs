//! One supervisor task per application: starts, health-checks, restarts and stops its backend.

use std::sync::Arc;
use std::time::Duration;

use ilmek_app_sdk::BackendConfig;
use ilmek_core::clock::Clock;
use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::{Token, Version};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::{Instant, interval, timeout};
use tokio_util::sync::CancellationToken;

use crate::launcher::{ExitReport, Killer, LaunchSpec, Launcher, RunningBackend};
use crate::link::BackendLink;
use crate::settings::Settings;
use crate::slots::AppSlot;
use crate::store::{AppRow, AppState, Desired, StatusRow, Store};

const GRACEFUL_STOP: Duration = Duration::from_secs(5);
const GIVE_UP_AFTER: u32 = 5;
const HEALTHY_AFTER: Duration = Duration::from_mins(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

pub(crate) struct Env {
    pub(crate) store: Store,
    pub(crate) settings: Settings,
    pub(crate) launcher: Arc<dyn Launcher>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) community_id: Uuid7,
}

struct Instance {
    generation: u64,
    version: Version,
    link: Arc<BackendLink>,
    exited: oneshot::Receiver<ExitReport>,
    killer: Killer,
    pid: Option<u32>,
    started: Instant,
}

#[derive(Default)]
struct Failures {
    consecutive: u32,
    retry_at: Option<Instant>,
    gave_up_at_generation: Option<u64>,
}

enum Event {
    Cancelled,
    Exited(Option<ExitReport>),
    LinkClosed,
    Tick,
}

pub(crate) struct Supervisor {
    env: Arc<Env>,
    app_id: Uuid7,
    slot: Arc<AppSlot>,
    instance: Option<Instance>,
    failures: Failures,
    restarts: u64,
}

fn backoff(base: Duration, consecutive: u32) -> Duration {
    base.saturating_mul(2u32.saturating_pow(consecutive.saturating_sub(1)))
        .min(MAX_BACKOFF)
}

impl Supervisor {
    pub(crate) fn new(env: Arc<Env>, app_id: Uuid7, slot: Arc<AppSlot>) -> Self {
        Self {
            env,
            app_id,
            slot,
            instance: None,
            failures: Failures::default(),
            restarts: 0,
        }
    }

    pub(crate) async fn run(mut self, cancel: CancellationToken) {
        let mut tick = interval(self.env.settings.reconcile_interval());
        loop {
            let event = {
                let closed_token = self.instance.as_ref().map(|i| i.link.closed().clone());
                let exit = async {
                    match self.instance.as_mut() {
                        Some(instance) => (&mut instance.exited).await.ok(),
                        None => std::future::pending().await,
                    }
                };
                let link_closed = async {
                    match &closed_token {
                        Some(token) => token.cancelled().await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    () = cancel.cancelled() => Event::Cancelled,
                    report = exit => Event::Exited(report),
                    () = link_closed => Event::LinkClosed,
                    _ = tick.tick() => Event::Tick,
                }
            };
            match event {
                Event::Cancelled => {
                    self.stop_instance().await;
                    return;
                }
                Event::Exited(report) => self.on_exit(report).await,
                Event::LinkClosed => {
                    self.stop_instance().await;
                    self.record_failure("backend closed its connection".into())
                        .await;
                }
                Event::Tick => {
                    if !self.reconcile().await {
                        return;
                    }
                }
            }
        }
    }

    /// Brings the backend in line with the desired state. `false` when the app is gone.
    async fn reconcile(&mut self) -> bool {
        let Ok(Some(app)) = self.env.store.app(self.app_id).await else {
            self.stop_instance().await;
            return false;
        };
        if app.desired == Desired::Stopped {
            self.stop_instance().await;
            self.failures = Failures::default();
            self.set_status(AppState::Stopped, None, None).await;
        } else if !app.backend_required {
            self.set_status(AppState::Running, None, None).await;
        } else {
            self.reconcile_backend(&app).await;
        }
        true
    }

    async fn reconcile_backend(&mut self, app: &AppRow) {
        if let Some(instance) = &self.instance {
            let changed = instance.generation != app.restart_generation
                || instance.version != app.active_version;
            if changed {
                self.set_status(AppState::Updating, instance.pid, None)
                    .await;
                self.stop_instance().await;
                self.failures = Failures::default();
                self.start(app).await;
            } else if instance.started.elapsed() >= HEALTHY_AFTER {
                self.failures.consecutive = 0;
            }
            return;
        }
        if self.failures.gave_up_at_generation == Some(app.restart_generation) {
            return;
        }
        if self.failures.gave_up_at_generation.is_some() {
            self.failures = Failures::default();
        }
        if self.failures.retry_at.is_none_or(|at| Instant::now() >= at) {
            self.start(app).await;
        }
    }

    async fn on_exit(&mut self, report: Option<ExitReport>) {
        let detail = report.map_or_else(
            || "backend vanished".to_owned(),
            |r| format!("exited: {}", r.detail),
        );
        if let Some(instance) = self.instance.take() {
            self.slot.set_link(None);
            instance.link.close();
        }
        self.record_failure(detail).await;
    }

    async fn record_failure(&mut self, detail: String) {
        self.restarts += 1;
        self.failures.consecutive += 1;
        let generation = self
            .env
            .store
            .app(self.app_id)
            .await
            .ok()
            .flatten()
            .map_or(0, |a| a.restart_generation);
        if self.failures.consecutive >= GIVE_UP_AFTER {
            self.failures.gave_up_at_generation = Some(generation);
            self.failures.retry_at = None;
            let message =
                format!("{detail}; gave up after {GIVE_UP_AFTER} failures, restart it manually");
            self.set_status(AppState::Failed, None, Some(message)).await;
        } else {
            self.failures.retry_at = Some(
                Instant::now()
                    + backoff(
                        Duration::from_millis(self.env.settings.restart_backoff_ms),
                        self.failures.consecutive,
                    ),
            );
            self.set_status(AppState::Failed, None, Some(detail)).await;
        }
    }

    async fn start(&mut self, app: &AppRow) {
        self.set_status(AppState::Starting, None, None).await;
        match self.try_start(app).await {
            Ok(instance) => {
                self.slot.set_link(Some(Arc::clone(&instance.link)));
                let pid = instance.pid;
                self.instance = Some(instance);
                self.set_status(AppState::Running, pid, None).await;
            }
            Err(message) => self.record_failure(message).await,
        }
    }

    async fn try_start(&mut self, app: &AppRow) -> Result<Instance, String> {
        let backend = self
            .env
            .store
            .backend(self.app_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no backend executable configured".to_owned())?;
        let paths = crate::paths::AppPaths::new(&self.env.settings.data_dir, self.app_id);
        paths.backup_before_version_change(app.active_version);

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(|_| "system randomness unavailable".to_owned())?;
        let token = Token::new(secret.to_vec());

        let spec = LaunchSpec {
            app_id: self.app_id,
            executable: backend.executable,
            args: backend.args,
            data_dir: paths.data(),
            logs_dir: paths.logs(),
            config: BackendConfig {
                token: token.clone(),
                ipc_port: port,
                community_id: self.env.community_id,
                app_id: self.app_id,
                data_dir: paths.data().display().to_string(),
            },
        };
        let RunningBackend {
            pid,
            mut exited,
            mut killer,
        } = self
            .env
            .launcher
            .launch(spec)
            .map_err(|e| format!("cannot start backend: {e}"))?;

        let accepted = tokio::select! {
            accepted = timeout(self.env.settings.start_deadline(), listener.accept()) => match accepted {
                Ok(Ok((stream, _))) => Ok(stream),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("backend did not connect in time".to_owned()),
            },
            report = &mut exited => Err(report.map_or_else(
                |_| "backend vanished while starting".to_owned(),
                |r| format!("backend exited while starting: {}", r.detail),
            )),
        };
        let stream = accepted.inspect_err(|_| killer.kill())?;
        let link = BackendLink::accept(stream, &token, self.app_id, self.env.community_id)
            .await
            .map_err(|e| {
                killer.kill();
                e.to_string()
            })?;
        paths.remember_version(app.active_version);
        Ok(Instance {
            generation: app.restart_generation,
            version: app.active_version,
            link,
            exited,
            killer,
            pid,
            started: Instant::now(),
        })
    }

    /// Orderly shutdown: refuse new work, close the connection, escalate to a kill.
    async fn stop_instance(&mut self) {
        let Some(mut instance) = self.instance.take() else {
            return;
        };
        self.slot.set_link(None);
        instance.link.close();
        if timeout(GRACEFUL_STOP, &mut instance.exited).await.is_err() {
            instance.killer.kill();
            let _ = timeout(Duration::from_secs(2), &mut instance.exited).await;
        }
    }

    async fn set_status(&self, state: AppState, pid: Option<u32>, last_error: Option<String>) {
        let status = StatusRow {
            state,
            pid,
            restarts: self.restarts,
            last_error,
            updated_at_ms: self.env.clock.now_ms(),
        };
        if let Err(error) = self.env.store.write_status(self.app_id, status).await {
            tracing::warn!(error = %error, "could not record application status");
        }
    }
}
