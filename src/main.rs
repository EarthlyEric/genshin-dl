mod api;
mod cli;
mod cmd;
mod download;
mod edition;
mod logging;
mod tui;
mod voice;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let edition = cli.edition.into();

    match cli.command {
        Command::Tui => {
            let log_rx = logging::init_tui()?;
            tui::run(edition, log_rx)
        }
        command => {
            logging::init_cli()?;
            cmd::run(command, edition)
        }
    }
}
