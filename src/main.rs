mod api;
mod cli;
mod cmd;
mod download;
mod edition;
mod tui;
mod voice;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let edition = cli.edition.into();

    match cli.command {
        Command::Tui => tui::run(edition),
        command => cmd::run(command, edition),
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
