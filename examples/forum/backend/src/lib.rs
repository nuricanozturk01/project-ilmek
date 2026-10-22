//! Forum backend. Every member may read and post; authorship is the host-authenticated principal.

use ilmek_app_sdk::{AppError, Backend, CallContext, MethodKind, Runtime, SubscribeContext};
use ilmek_store::Migration;
use serde::Deserialize;
use serde_json::{Value, json};

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "forum",
    sql: "
CREATE TABLE topics (
    id INTEGER PRIMARY KEY,
    author TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    comment_count INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE TABLE comments (
    id INTEGER PRIMARY KEY,
    topic_id INTEGER NOT NULL REFERENCES topics(id),
    author TEXT NOT NULL,
    body TEXT NOT NULL
) STRICT;
CREATE INDEX comments_by_topic ON comments (topic_id, id);
",
}];

const TITLE_MAX: usize = 200;
const BODY_MAX: usize = 5_000;
const PAGE_MAX: i64 = 50;
const EVENT_TOPIC: &str = "forum.events";

#[derive(Debug)]
pub struct ForumBackend {
    runtime: Runtime,
}

impl ForumBackend {
    #[must_use]
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewTopic {
    title: String,
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewComment {
    topic_id: i64,
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TopicRef {
    topic_id: i64,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    before_id: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

fn parse<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, AppError> {
    serde_json::from_slice(body)
        .map_err(|_| AppError::invalid("request body does not match this method"))
}

/// Text must be non-blank and within a character limit (not bytes, so Turkish text counts fairly).
fn checked_text(value: &str, field: &str, max: usize) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid(format!("{field} must not be empty")));
    }
    if trimmed.chars().count() > max {
        return Err(AppError::invalid(format!(
            "{field} is longer than {max} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

fn bytes(value: &Value) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec(value).map_err(|_| AppError::invalid("response could not be encoded"))
}

impl ForumBackend {
    async fn list_topics(&self, body: &[u8]) -> Result<Vec<u8>, AppError> {
        let page: Page = if body == b"{}" {
            Page::default()
        } else {
            parse(body)?
        };
        let limit = page.limit.unwrap_or(20).clamp(1, PAGE_MAX);
        let before = page.before_id.unwrap_or(i64::MAX);
        let topics = self
            .runtime
            .db
            .call(move |c| {
                let mut statement = c.prepare(
                    "SELECT id, author, title, comment_count FROM topics WHERE id < ?1
                     ORDER BY id DESC LIMIT ?2",
                )?;
                let rows = statement.query_map([before, limit], |row| {
                    Ok(json!({
                        "id": row.get::<_, i64>(0)?,
                        "author": row.get::<_, String>(1)?,
                        "title": row.get::<_, String>(2)?,
                        "comment_count": row.get::<_, i64>(3)?,
                    }))
                })?;
                rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
            })
            .await?;
        bytes(&json!({ "topics": topics }))
    }

    async fn get_topic(&self, body: &[u8]) -> Result<Vec<u8>, AppError> {
        let TopicRef { topic_id } = parse(body)?;
        let found = self
            .runtime
            .db
            .call(move |c| {
                let topic = c
                    .query_row(
                        "SELECT id, author, title, body FROM topics WHERE id = ?1",
                        [topic_id],
                        |row| {
                            Ok(json!({
                                "id": row.get::<_, i64>(0)?,
                                "author": row.get::<_, String>(1)?,
                                "title": row.get::<_, String>(2)?,
                                "body": row.get::<_, String>(3)?,
                            }))
                        },
                    )
                    .map_err(|_| AppError::not_found("no such topic"))?;
                let mut statement = c.prepare(
                    "SELECT id, author, body FROM comments WHERE topic_id = ?1 ORDER BY id",
                )?;
                let rows = statement.query_map([topic_id], |row| {
                    Ok(json!({
                        "id": row.get::<_, i64>(0)?,
                        "author": row.get::<_, String>(1)?,
                        "body": row.get::<_, String>(2)?,
                    }))
                })?;
                let comments = rows.collect::<Result<Vec<_>, _>>()?;
                Ok::<_, AppError>(json!({ "topic": topic, "comments": comments }))
            })
            .await?;
        bytes(&found)
    }

    async fn create_topic(&self, context: &CallContext, body: &[u8]) -> Result<Vec<u8>, AppError> {
        let input: NewTopic = parse(body)?;
        let title = checked_text(&input.title, "title", TITLE_MAX)?;
        let text = checked_text(&input.body, "body", BODY_MAX)?;
        let author = context.principal_id.to_string();
        let (result, _) = self
            .runtime
            .operations
            .write(context, move |tx, events| {
                tx.execute(
                    "INSERT INTO topics (author, title, body) VALUES (?1, ?2, ?3)",
                    [&author, &title, &text],
                )?;
                let id = tx.last_insert_rowid();
                events.append(
                    tx,
                    EVENT_TOPIC,
                    &bytes(&json!({"kind": "topic", "topic_id": id}))?,
                )?;
                bytes(&json!({ "topic_id": id }))
            })
            .await?;
        Ok(result)
    }

    async fn add_comment(&self, context: &CallContext, body: &[u8]) -> Result<Vec<u8>, AppError> {
        let input: NewComment = parse(body)?;
        let text = checked_text(&input.body, "body", BODY_MAX)?;
        let author = context.principal_id.to_string();
        let topic_id = input.topic_id;
        let (result, _) = self
            .runtime
            .operations
            .write(context, move |tx, events| {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM topics WHERE id = ?1)",
                    [topic_id],
                    |row| row.get(0),
                )?;
                if !exists {
                    return Err(AppError::not_found("no such topic"));
                }
                tx.execute(
                    "INSERT INTO comments (topic_id, author, body) VALUES (?1, ?2, ?3)",
                    ilmek_store::rusqlite::params![topic_id, author, text],
                )?;
                let id = tx.last_insert_rowid();
                tx.execute(
                    "UPDATE topics SET comment_count = comment_count + 1 WHERE id = ?1",
                    [topic_id],
                )?;
                events.append(
                    tx,
                    EVENT_TOPIC,
                    &bytes(&json!({"kind": "comment", "topic_id": topic_id, "comment_id": id}))?,
                )?;
                bytes(&json!({ "comment_id": id }))
            })
            .await?;
        Ok(result)
    }
}

impl Backend for ForumBackend {
    fn methods(&self) -> &'static [(&'static str, MethodKind)] {
        &[
            ("forum.topics.list", MethodKind::Read),
            ("forum.topic.get", MethodKind::Read),
            ("forum.topics.create", MethodKind::Write),
            ("forum.comments.add", MethodKind::Write),
        ]
    }

    async fn call(&self, context: CallContext, body: Vec<u8>) -> Result<Vec<u8>, AppError> {
        match context.method.as_str() {
            "forum.topics.list" => self.list_topics(&body).await,
            "forum.topic.get" => self.get_topic(&body).await,
            "forum.topics.create" => self.create_topic(&context, &body).await,
            _ => self.add_comment(&context, &body).await,
        }
    }

    async fn authorize_subscribe(&self, context: &SubscribeContext) -> Result<(), AppError> {
        if context.topic.as_str() == EVENT_TOPIC {
            Ok(())
        } else {
            Err(AppError::forbidden("unknown topic"))
        }
    }
}
