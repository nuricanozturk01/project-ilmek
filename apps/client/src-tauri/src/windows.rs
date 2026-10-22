use std::sync::Arc;

use ilmek_runtime::AppSession;
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::UiError;
use crate::state::Shell;

/// Opens an application in its own window. The window has its own label, so the bridge and the
/// content scheme can tell it apart from every other window.
pub(crate) fn open_app_window(
    app: &AppHandle,
    label: &str,
    session: &Arc<AppSession>,
) -> Result<(), UiError> {
    let url = session.start_url().parse().map_err(|_| UiError::new("window_failed"))?;
    let guard = Arc::clone(session);
    let title = session.manifest().name.to_string();

    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::CustomProtocol(url))
        .title(title)
        .inner_size(960.0, 720.0)
        .initialization_script(session.bootstrap_script())
        .on_navigation(move |target| guard.allows_navigation(target.as_str()))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        // Web storage (localStorage, IndexedDB, cookies) would be keyed by origin alone, i.e. by
        // application id, and would survive across communities and identities. Applications use
        // the SDK's scoped storage instead, so the WebView keeps nothing.
        .incognito(true);

    let window = builder.build().map_err(|error| {
        tracing::error!(%error, "could not create the application window");
        UiError::new("window_failed")
    })?;

    let handle = app.clone();
    let closing = label.to_owned();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            if let Some(session) = handle.state::<Shell>().remove_session(&closing) {
                session.close();
            }
        }
    });
    Ok(())
}
