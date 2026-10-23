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
        .register_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .setup(|app| {
            let root = state::data_root(app.handle())?;
            app.manage(state::Shell::new(root));
            windows::wipe_web_data(app.handle());
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
            commands::shell_close_app,
            commands::ilmek_bridge,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(%error, "the application failed to start");
        std::process::exit(1);
    }
}

/// `ilmek-client ilmek://publisher/app ...` opens those applications right after start-up.
fn open_startup_addresses(app: &tauri::AppHandle) {
    for (index, address) in std::env::args()
        .skip(1)
        .filter(|arg| arg.starts_with("ilmek://"))
        .enumerate()
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let shell = app.state::<state::Shell>();
            let transfer = format!("startup-{index}");
            if let Err(error) = commands::open_address(&app, &shell, &address, &transfer).await {
                tracing::error!(code = error.code, "could not open a start-up address");
            }
        });
    }
}
