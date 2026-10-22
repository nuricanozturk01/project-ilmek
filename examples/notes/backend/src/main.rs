//! Entry point started by `ilmek-host`; configuration arrives on stdin.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    match ilmek_app_sdk::run(
        ilmek_example_notes::MIGRATIONS,
        ilmek_example_notes::NotesBackend::new,
    )
    .await
    {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "notes backend stopped");
            std::process::ExitCode::FAILURE
        }
    }
}
