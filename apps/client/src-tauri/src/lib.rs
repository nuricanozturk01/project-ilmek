//! Desktop shell. All policy lives in `ilmek-runtime`; this crate only creates windows,
//! registers the content scheme and forwards commands.

mod commands;
mod error;
mod protocol;
mod sink;
mod state;
mod windows;

use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

pub fn run() {
    let level = std::env::var("ILMEK_LOG")
        .ok()
        .and_then(|text| text.parse::<tracing::Level>().ok())
        .unwrap_or(tracing::Level::WARN);
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let result = tauri::Builder::default()
        // Must be the first plugin. A second start (the browser handing over an `ilmek://` link)
        // is forwarded to the running instance and exits; with the `deep-link` feature the link
        // reaches `on_open_url` below, so here we only bring the window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .register_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .setup(|app| {
            let root = state::data_root(app.handle())?;
            app.manage(state::Shell::new(root));
            windows::start_idle_sweeper(app.handle());
            // `ilmek://…` links from a browser or the OS: macOS delivers them as events, Linux and
            // Windows as arguments of a new process (handled by the single-instance plugin).
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    open_link(&handle, url.as_str());
                }
            });
            // Linux and Windows have no installer step in `tauri dev` or a plain binary: register
            // the scheme for this user when the program starts. (A packaged build registers it
            // while installing; macOS only ever registers through the installed `.app`.)
            #[cfg(any(target_os = "linux", windows))]
            if let Err(error) = app.deep_link().register_all() {
                tracing::warn!(%error, "could not register the ilmek:// scheme for this user");
            }
            open_startup_addresses(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::shell_state,
            commands::shell_join,
            commands::shell_refresh,
            commands::shell_catalog,
            commands::shell_open,
            commands::shell_cancel,
            commands::shell_cached,
            commands::shell_remove,
            commands::shell_history,
            commands::shell_bookmark,
            commands::shell_diagnostics,
            commands::shell_show_tab,
            commands::shell_close_tab,
            commands::shell_pending,
            commands::shell_bridge,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(%error, "the application failed to start");
        std::process::exit(1);
    }
}

/// An `ilmek://` link the operating system handed to this program: the UI opens it in a tab.
fn open_link(app: &tauri::AppHandle, address: &str) {
    if !address.starts_with("ilmek://") {
        return;
    }
    app.state::<state::Shell>()
        .queue_address(address.to_owned());
    let _ = app.emit("open-pending", ());
}

/// `ilmek-client ilmek://publisher/app ...` opens those applications right after start-up.
fn open_startup_addresses(app: &tauri::AppHandle) {
    for address in std::env::args()
        .skip(1)
        .filter(|arg| arg.starts_with("ilmek://"))
    {
        open_link(app, &address);
    }
}
