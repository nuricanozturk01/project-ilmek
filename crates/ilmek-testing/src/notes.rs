//! A small private-notes backend used to exercise hosts, SDKs and clients end to end.

use std::sync::{Arc, Mutex};

use ilmek_app_sdk::{
    AppError, Backend, CallContext, MethodKind, Runtime, SubscribeContext, run_with_config,
};
use ilmek_host::LaunchSpec;
use ilmek_store::Migration;
use tokio::task::{AbortHandle, JoinHandle};

use crate::launcher::InProcessLauncher;

pub const NOTES_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "notes",
    sql: "CREATE TABLE notes (
              id INTEGER PRIMARY KEY,
              owner TEXT NOT NULL,
              text TEXT NOT NULL
          ) STRICT;",
}];

pub struct NotesBackend {
    runtime: Runtime,
}

impl NotesBackend {
    #[must_use]
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }
}

impl NotesBackend {
    async fn add(
        &self,
        context: &CallContext,
        body: &[u8],
        owner: String,
    ) -> Result<Vec<u8>, AppError> {
        let input: serde_json::Value =
            serde_json::from_slice(body).map_err(|_| AppError::invalid("body is not JSON"))?;
        let text = input["text"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AppError::invalid("text required"))?
            .to_owned();
        let (result, _) = self
            .runtime
            .operations
            .write(context, move |tx, events| {
                tx.execute(
                    "INSERT INTO notes (owner, text) VALUES (?1, ?2)",
                    [&owner, &text],
                )?;
                let id = tx.last_insert_rowid();
                events.append(tx, "notes.events", format!("{{\"id\":{id}}}").as_bytes())?;
                Ok(format!("{{\"id\":{id}}}").into_bytes())
            })
            .await?;
        Ok(result)
    }
}

impl Backend for NotesBackend {
    fn methods(&self) -> &'static [(&'static str, MethodKind)] {
        &[
            ("notes.add", MethodKind::Write),
            ("notes.list", MethodKind::Read),
        ]
    }

    async fn call(&self, context: CallContext, body: Vec<u8>) -> Result<Vec<u8>, AppError> {
        let owner = context.principal_id.to_string();
        if context.method.as_str() == "notes.add" {
            return self.add(&context, &body, owner).await;
        }
        let notes = self
            .runtime
            .db
            .call(move |connection| {
                let mut statement = connection
                    .prepare("SELECT id, text FROM notes WHERE owner = ?1 ORDER BY id")?;
                let rows = statement.query_map([&owner], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "text": row.get::<_, String>(1)?
                    }))
                })?;
                rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
            })
            .await?;
        serde_json::to_vec(&serde_json::json!({ "notes": notes }))
            .map_err(|_| AppError::invalid("response"))
    }

    async fn authorize_subscribe(&self, context: &SubscribeContext) -> Result<(), AppError> {
        if context.topic.as_str() == "notes.events" {
            Ok(())
        } else {
            Err(AppError::forbidden("unknown topic"))
        }
    }
}

/// Handles to the running in-process backends, so tests can crash them.
#[derive(Debug, Clone, Default)]
pub struct BackendHandles(Arc<Mutex<Vec<AbortHandle>>>);

impl BackendHandles {
    /// Aborts every backend started so far, as if the processes had been killed.
    pub fn crash_all(&self) {
        for handle in self.0.lock().unwrap().drain(..) {
            handle.abort();
        }
    }
}

/// A launcher that runs a backend built by `build` inside the test process.
#[must_use]
pub fn backend_launcher<B: Backend>(
    migrations: &'static [Migration],
    build: fn(Runtime) -> B,
) -> (InProcessLauncher, BackendHandles) {
    let handles = BackendHandles::default();
    let shared = handles.clone();
    let launcher = InProcessLauncher::new(move |spec: LaunchSpec| -> JoinHandle<()> {
        let task = tokio::spawn(async move {
            let _ = run_with_config(spec.config, migrations, build).await;
        });
        shared.0.lock().unwrap().push(task.abort_handle());
        task
    });
    (launcher, handles)
}

/// A launcher that runs [`NotesBackend`] inside the test process.
#[must_use]
pub fn notes_launcher() -> (InProcessLauncher, BackendHandles) {
    backend_launcher(NOTES_MIGRATIONS, NotesBackend::new)
}
