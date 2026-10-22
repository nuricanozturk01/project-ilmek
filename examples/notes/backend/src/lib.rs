//! Private notes. A note belongs to the principal that created it; everyone else gets
//! `NOT_FOUND`, which does not even reveal that the note exists. Updates need the revision the
//! editor last saw, so two concurrent edits cannot silently overwrite each other.

use ilmek_app_sdk::{AppError, Backend, CallContext, MethodKind, Runtime};
use ilmek_store::Migration;
use ilmek_store::rusqlite::params;
use serde::Deserialize;
use serde_json::{Value, json};

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "notes",
    sql: "
CREATE TABLE notes (
    id INTEGER PRIMARY KEY,
    owner TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    revision INTEGER NOT NULL
) STRICT;
CREATE INDEX notes_by_owner ON notes (owner, id);
",
}];

const TITLE_MAX: usize = 200;
const BODY_MAX: usize = 20_000;

#[derive(Debug)]
pub struct NotesBackend {
    runtime: Runtime,
}

impl NotesBackend {
    #[must_use]
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    title: String,
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: i64,
    expected_revision: i64,
    title: String,
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Removal {
    id: i64,
    expected_revision: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteRef {
    id: i64,
}

fn parse<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, AppError> {
    serde_json::from_slice(body)
        .map_err(|_| AppError::invalid("request body does not match this method"))
}

fn checked(title: &str, body: &str) -> Result<(String, String), AppError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > TITLE_MAX {
        return Err(AppError::invalid(format!(
            "title must be 1..={TITLE_MAX} characters"
        )));
    }
    if body.chars().count() > BODY_MAX {
        return Err(AppError::invalid(format!(
            "body is longer than {BODY_MAX} characters"
        )));
    }
    Ok((title.to_owned(), body.to_owned()))
}

fn bytes(value: &Value) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec(value).map_err(|_| AppError::invalid("response could not be encoded"))
}

impl NotesBackend {
    async fn list(&self, owner: String) -> Result<Vec<u8>, AppError> {
        let notes = self
            .runtime
            .db
            .call(move |c| {
                let mut statement = c.prepare(
                    "SELECT id, title, revision FROM notes WHERE owner = ?1 ORDER BY id",
                )?;
                let rows = statement.query_map([&owner], |row| {
                    Ok(json!({
                        "id": row.get::<_, i64>(0)?,
                        "title": row.get::<_, String>(1)?,
                        "revision": row.get::<_, i64>(2)?,
                    }))
                })?;
                rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
            })
            .await?;
        bytes(&json!({ "notes": notes }))
    }

    async fn get(&self, owner: String, body: &[u8]) -> Result<Vec<u8>, AppError> {
        let NoteRef { id } = parse(body)?;
        let note = self
            .runtime
            .db
            .call(move |c| {
                c.query_row(
                    "SELECT id, title, body, revision FROM notes WHERE id = ?1 AND owner = ?2",
                    params![id, owner],
                    |row| {
                        Ok(json!({
                            "id": row.get::<_, i64>(0)?,
                            "title": row.get::<_, String>(1)?,
                            "body": row.get::<_, String>(2)?,
                            "revision": row.get::<_, i64>(3)?,
                        }))
                    },
                )
                .map_err(|_| AppError::not_found("no such note"))
            })
            .await?;
        bytes(&note)
    }

    async fn create(
        &self,
        context: &CallContext,
        owner: String,
        body: &[u8],
    ) -> Result<Vec<u8>, AppError> {
        let draft: Draft = parse(body)?;
        let (title, text) = checked(&draft.title, &draft.body)?;
        let (result, _) = self
            .runtime
            .operations
            .write(context, move |tx, _| {
                tx.execute(
                    "INSERT INTO notes (owner, title, body, revision) VALUES (?1, ?2, ?3, 1)",
                    [&owner, &title, &text],
                )?;
                bytes(&json!({ "id": tx.last_insert_rowid(), "revision": 1 }))
            })
            .await?;
        Ok(result)
    }

    async fn update(
        &self,
        context: &CallContext,
        owner: String,
        body: &[u8],
    ) -> Result<Vec<u8>, AppError> {
        let input: Update = parse(body)?;
        let (title, text) = checked(&input.title, &input.body)?;
        let (result, _) = self
            .runtime
            .operations
            .write(context, move |tx, _| {
                let changed = tx.execute(
                    "UPDATE notes SET title = ?1, body = ?2, revision = revision + 1
                     WHERE id = ?3 AND owner = ?4 AND revision = ?5",
                    params![title, text, input.id, owner, input.expected_revision],
                )?;
                if changed == 0 {
                    return Err(conflict_or_missing(tx, input.id, &owner)?);
                }
                bytes(&json!({ "id": input.id, "revision": input.expected_revision + 1 }))
            })
            .await?;
        Ok(result)
    }

    async fn delete(
        &self,
        context: &CallContext,
        owner: String,
        body: &[u8],
    ) -> Result<Vec<u8>, AppError> {
        let input: Removal = parse(body)?;
        let (result, _) = self
            .runtime
            .operations
            .write(context, move |tx, _| {
                let changed = tx.execute(
                    "DELETE FROM notes WHERE id = ?1 AND owner = ?2 AND revision = ?3",
                    params![input.id, owner, input.expected_revision],
                )?;
                if changed == 0 {
                    return Err(conflict_or_missing(tx, input.id, &owner)?);
                }
                bytes(&json!({ "deleted": input.id }))
            })
            .await?;
        Ok(result)
    }
}

/// Distinguishes "someone else changed it" from "it is not yours / does not exist".
fn conflict_or_missing(
    tx: &ilmek_store::rusqlite::Transaction<'_>,
    id: i64,
    owner: &str,
) -> Result<AppError, AppError> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM notes WHERE id = ?1 AND owner = ?2)",
        params![id, owner],
        |row| row.get(0),
    )?;
    Ok(if exists {
        AppError::revision_conflict("the note changed since you loaded it")
    } else {
        AppError::not_found("no such note")
    })
}

impl Backend for NotesBackend {
    fn methods(&self) -> &'static [(&'static str, MethodKind)] {
        &[
            ("notes.list", MethodKind::Read),
            ("notes.get", MethodKind::Read),
            ("notes.create", MethodKind::Write),
            ("notes.update", MethodKind::Write),
            ("notes.delete", MethodKind::Write),
        ]
    }

    async fn call(&self, context: CallContext, body: Vec<u8>) -> Result<Vec<u8>, AppError> {
        let owner = context.principal_id.to_string();
        match context.method.as_str() {
            "notes.list" => self.list(owner).await,
            "notes.get" => self.get(owner, &body).await,
            "notes.create" => self.create(&context, owner, &body).await,
            "notes.update" => self.update(&context, owner, &body).await,
            _ => self.delete(&context, owner, &body).await,
        }
    }
}
