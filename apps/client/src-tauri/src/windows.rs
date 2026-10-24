//! Application tabs. The shell UI is the only webview; each open application is a sandboxed frame
//! inside it (see `ilmek-runtime` for the page policy). What lives here is the tab *session*
//! lifecycle: which tab is on screen, and giving up background tabs to save memory.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::Shell;

/// At most this many application tabs keep a live session and frame; showing one more discards
/// the one that has been in the background the longest.
pub(crate) const MAX_LIVE_TABS: usize = 4;
/// A background tab untouched for this long is discarded (its tab stays; reopening reloads it
/// from the verified package cache).
pub(crate) const IDLE_LIMIT: Duration = Duration::from_mins(10);

/// Records which application tab is on screen (`None`: a shell page is).
pub(crate) fn show_tab(app: &AppHandle, label: Option<&str>) {
    app.state::<Shell>().set_shown(label);
}

/// Ends an application tab's session. The UI removes the frame itself.
pub(crate) fn close_tab(app: &AppHandle, label: &str) {
    let shell = app.state::<Shell>();
    if let Some(session) = shell.remove_session(label) {
        session.close();
    }
    shell.forget_tab(label);
}

/// Gives a background tab's memory back; the UI keeps the tab and reloads it when it is chosen.
pub(crate) fn discard_tab(app: &AppHandle, label: &str) {
    close_tab(app, label);
    let _ = app.emit("tab-discarded", label);
}

/// Keeps at most [`MAX_LIVE_TABS`] live tabs, never touching the visible one or `keep`.
pub(crate) fn enforce_limit(app: &AppHandle, keep: &str) {
    let shell = app.state::<Shell>();
    while shell.live_tabs() > MAX_LIVE_TABS {
        let Some((oldest, _)) = shell
            .background_tabs()
            .into_iter()
            .find(|(label, _)| label != keep)
        else {
            return;
        };
        discard_tab(app, &oldest);
    }
}

/// Every half minute, discards background tabs that have not been looked at for [`IDLE_LIMIT`].
pub(crate) fn start_idle_sweeper(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            let idle: Vec<String> = app
                .state::<Shell>()
                .background_tabs()
                .into_iter()
                .filter(|(_, idle)| *idle >= IDLE_LIMIT)
                .map(|(label, _)| label)
                .collect();
            for label in idle {
                discard_tab(&app, &label);
            }
        }
    });
}
