#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects command arguments and scheme-handler parameters by value"
)]

use std::sync::Arc;

use ilmek_runtime::{CachedApp, CatalogPage, Client, Response, Status};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Webview};

use crate::error::UiError;
use crate::sink::WindowSink;
use crate::state::Shell;
use crate::windows;

type UiResult<T> = Result<T, UiError>;

fn client(shell: &Shell) -> UiResult<Arc<Client>> {
    shell.client().ok_or(UiError::new("not_joined"))
}

#[derive(Serialize)]
pub(crate) struct ShellState {
    joined: bool,
    status: Option<Status>,
}

#[tauri::command]
pub(crate) async fn shell_state(shell: State<'_, Shell>) -> UiResult<ShellState> {
    let Some(client) = shell.client() else {
        return Ok(ShellState {
            joined: false,
            status: None,
        });
    };
    Ok(ShellState {
        joined: true,
        status: Some(client.refresh().await),
    })
}

#[tauri::command]
pub(crate) async fn shell_join(
    shell: State<'_, Shell>,
    profile: Option<String>,
    registry: Option<String>,
) -> UiResult<()> {
    let profile = profile.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let registry = registry.as_deref().map(str::trim).filter(|r| !r.is_empty());
    if let Some(profile) = profile {
        Client::join(&shell.root, profile.as_bytes())?;
    } else {
        let registry = registry
            .map(str::parse::<std::net::SocketAddrV4>)
            .transpose()
            .map_err(|_| UiError::new("invalid_input"))?;
        Client::join_auto(&shell.root, registry).await?;
    }
    shell.reset_client();
    Ok(())
}

#[tauri::command]
pub(crate) async fn shell_refresh(shell: State<'_, Shell>) -> UiResult<Status> {
    Ok(client(&shell)?.refresh().await)
}

#[tauri::command]
pub(crate) async fn shell_catalog(
    shell: State<'_, Shell>,
    query: String,
    cursor: Option<String>,
) -> UiResult<CatalogPage> {
    Ok(client(&shell)?.catalog(&query, cursor).await?)
}

#[derive(Clone, Serialize)]
struct TransferProgress {
    id: String,
    bytes_done: u64,
    bytes_total: u64,
}

#[derive(Serialize)]
pub(crate) struct Opened {
    label: String,
    stale: bool,
}

#[tauri::command]
pub(crate) async fn shell_open(
    app: AppHandle,
    shell: State<'_, Shell>,
    address: String,
    transfer_id: String,
) -> UiResult<Opened> {
    open_address(&app, &shell, &address, &transfer_id).await
}

/// Resolves, downloads, verifies and opens `address` in a new window.
pub(crate) async fn open_address(
    app: &AppHandle,
    shell: &Shell,
    address: &str,
    transfer_id: &str,
) -> UiResult<Opened> {
    let client = client(shell)?;
    let label = shell.next_label();
    let sink = Arc::new(WindowSink {
        app: app.clone(),
        label: label.clone(),
    });
    let cancel = shell.begin_transfer(transfer_id);

    let progress_app = app.clone();
    let progress_id = transfer_id.to_owned();
    let opened = client
        .open_app(
            address,
            sink,
            move |progress| {
                let _ = progress_app.emit(
                    "transfer-progress",
                    TransferProgress {
                        id: progress_id.clone(),
                        bytes_done: progress.bytes_done,
                        bytes_total: progress.bytes_total,
                    },
                );
            },
            &cancel,
        )
        .await;
    shell.end_transfer(transfer_id);

    let (session, source) = opened?;
    shell.add_session(&label, Arc::clone(&session));
    windows::open_app_window(app, &label, &session)?;
    Ok(Opened {
        label,
        stale: source == ilmek_client::Source::StaleCache,
    })
}

#[tauri::command]
pub(crate) fn shell_cancel(shell: State<'_, Shell>, transfer_id: String) {
    shell.cancel_transfer(&transfer_id);
}

#[tauri::command]
pub(crate) fn shell_cached(shell: State<'_, Shell>) -> UiResult<Vec<CachedApp>> {
    Ok(client(&shell)?.cached_apps())
}

#[tauri::command]
pub(crate) async fn shell_remove(shell: State<'_, Shell>, address: String) -> UiResult<()> {
    Ok(client(&shell)?.remove_app(&address).await?)
}

#[derive(Serialize)]
pub(crate) struct HistoryView {
    visited: Vec<String>,
    bookmarks: Vec<String>,
}

#[tauri::command]
pub(crate) fn shell_history(shell: State<'_, Shell>) -> UiResult<HistoryView> {
    let client = client(&shell)?;
    Ok(HistoryView {
        visited: client.visited(),
        bookmarks: client.bookmarks(),
    })
}

#[tauri::command]
pub(crate) fn shell_bookmark(shell: State<'_, Shell>, address: String, on: bool) -> UiResult<()> {
    Ok(client(&shell)?.set_bookmark(&address, on)?)
}

#[tauri::command]
pub(crate) fn shell_diagnostics(shell: State<'_, Shell>) -> UiResult<String> {
    Ok(client(&shell)?.diagnostics())
}

#[tauri::command]
pub(crate) fn shell_close_app(app: AppHandle, label: String) {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
}

/// The only command application pages may call. The application is identified by the calling
/// window's label, which the page cannot choose or change.
#[tauri::command]
pub(crate) async fn ilmek_bridge(
    webview: Webview,
    shell: State<'_, Shell>,
    request: serde_json::Value,
) -> UiResult<Response> {
    let session = shell
        .session(webview.label())
        .ok_or(UiError::new("forbidden"))?;
    let operation = request
        .get("op")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("?")
        .to_owned();
    let response = session.handle(request).await;
    tracing::debug!(operation, "bridge request handled");
    Ok(response)
}
