//! `ilmek`: community, identity, host and application management.

mod cli;
mod commands;
mod error;
mod output;

use std::process::ExitCode;

use clap::Parser;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let arguments = cli::Cli::parse();
    match commands::run(arguments.command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}
