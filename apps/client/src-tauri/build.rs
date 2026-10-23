//! Every command is listed here so that capabilities decide who may call it: the main window
//! gets the `shell_*` commands, application windows get only `ilmek_bridge`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let commands = &[
        "shell_state",
        "shell_join",
        "shell_refresh",
        "shell_catalog",
        "shell_open",
        "shell_cancel",
        "shell_cached",
        "shell_remove",
        "shell_history",
        "shell_bookmark",
        "shell_diagnostics",
        "shell_close_app",
        "ilmek_bridge",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )?;
    Ok(())
}
