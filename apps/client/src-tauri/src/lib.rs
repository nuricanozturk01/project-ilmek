//! Desktop shell. All policy lives in `ilmek-runtime`; this crate only creates windows,
//! registers the content scheme and forwards commands.

mod commands;
mod error;
mod protocol;
mod sink;
mod state;
mod windows;

use tauri::Manager;

pub fn run() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).with_ansi(false).init();
    let result = tauri::Builder::default()
        .register_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .setup(|app| {
            let root = state::data_root(app.handle())?;
            app.manage(state::Shell::new(root));
            open_startup_address(app.handle().clone());
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
            commands::shell_close_app,
            commands::ilmek_bridge,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(%error, "the application failed to start");
        std::process::exit(1);
    }
}

/// `ilmek-client ilmek://publisher/app` opens that application right after start-up.
fn open_startup_address(app: tauri::AppHandle) {
    let Some(address) = std::env::args().nth(1).filter(|arg| arg.starts_with("ilmek://")) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let shell = app.state::<state::Shell>();
        if let Err(error) = commands::open_address(&app, &shell, &address, "startup").await {
            tracing::error!(code = error.code, "could not open the start-up address");
        }
    });
}
