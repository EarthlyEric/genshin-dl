use std::sync::mpsc;

use anime_launcher_sdk::anime_game_core::sophon::prettify_bytes;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::api;
use crate::cli::Command;
use crate::download::{self, Event};
use crate::edition::Edition;

pub fn run(command: Command, edition: Edition) -> anyhow::Result<()> {
    match command {
        Command::List => list(edition),
        Command::Download {
            dest,
            threads,
            temp,
            voice,
            no_free_space_check,
        } => with_progress(move |tx| {
            download::download(edition, dest, threads, temp, voice, no_free_space_check, tx)
        }),
        Command::Update {
            dest,
            threads,
            temp,
            voice,
            no_free_space_check,
        } => with_progress(move |tx| {
            download::update(edition, dest, threads, temp, voice, no_free_space_check, tx)
        }),
        Command::Predownload {
            dest,
            threads,
            temp,
            voice,
            no_free_space_check,
        } => with_progress(move |tx| {
            download::pre_download(edition, dest, threads, temp, voice, no_free_space_check, tx)
        }),
        Command::Repair {
            dest,
            threads,
            temp,
            voice,
            no_free_space_check,
        } => with_progress(move |tx| {
            download::repair(edition, dest, threads, temp, voice, no_free_space_check, tx)
        }),
        Command::Tui => unreachable!("TUI is handled before cmd::run"),
    }
}

fn list(edition: Edition) -> anyhow::Result<()> {
    let mut output = Vec::new();
    list_text(edition, &mut output)?;
    for line in output {
        println!("{line}");
    }
    Ok(())
}

/// Build the human readable version/download manifest listing, shared between
/// the CLI and the TUI.
pub fn list_text(edition: Edition, out: &mut Vec<String>) -> anyhow::Result<()> {
    let client = api::new_client();

    out.push(format!("== Edition: {} ==", edition));
    let branches = api::get_branches(&client, edition)?;

    out.push(String::new());
    out.push("Available versions:".into());
    for branch in &branches.game_branches {
        let main = branch.main.as_ref().map(|p| p.tag.as_str()).unwrap_or("-");
        let pre = branch
            .pre_download
            .as_ref()
            .map(|p| p.tag.as_str())
            .unwrap_or("-");
        out.push(format!(
            "  version {main:<12} pre-download {pre:<12} id={} biz={}",
            branch.game.id, branch.game.biz
        ));
    }

    let package = api::get_latest_package(&branches, edition, false)?;
    let downloads = api::get_downloads(&client, &package, edition)?;

    out.push(String::new());
    out.push(format!("Download manifests for version {}:", package.tag));
    for manifest in &downloads.manifests {
        let compressed = manifest.stats.compressed_size.parse::<u64>().unwrap_or(0);
        let uncompressed = manifest.stats.uncompressed_size.parse::<u64>().unwrap_or(0);
        out.push(format!(
            "  {:<10} compressed {:>10}  uncompressed {:>10}  files {}  chunks {}",
            manifest.matching_field,
            prettify_bytes(compressed),
            prettify_bytes(uncompressed),
            manifest.stats.file_count,
            manifest.stats.chunk_count,
        ));
    }

    Ok(())
}

/// Run a download worker on a background thread and render progress with
/// `indicatif` on the main thread.
fn with_progress<F>(work: F) -> anyhow::Result<()>
where
    F: FnOnce(mpsc::Sender<Event>) -> anyhow::Result<()> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let result = work(tx.clone());
        let _ = tx.send(Event::Finished(
            result.as_ref().map(|_| ()).map_err(|e| e.to_string()),
        ));
        result
    });

    let mp = MultiProgress::new();

    let bytes_bar = mp.add(ProgressBar::new(0));
    bytes_bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} bytes [{bar:40.cyan/blue}] {pos}/{len} ({percent}%)",
        )
        .unwrap(),
    );

    let files_bar = mp.add(ProgressBar::new(0));
    files_bar.set_style(
        ProgressStyle::with_template("{spinner:.green} files [{bar:40.green/white}] {pos}/{len}")
            .unwrap(),
    );

    let log_bar = mp.add(ProgressBar::new_spinner());
    log_bar.set_style(ProgressStyle::with_template("{spinner:.yellow} {msg}").unwrap());
    log_bar.enable_steady_tick(std::time::Duration::from_millis(80));

    let mut phase = String::new();

    for event in &rx {
        match event {
            Event::Phase(p) => {
                phase = p.clone();
                log_bar.set_message(format!("{p}..."));
            }
            Event::ProgressBytes { downloaded, total } => {
                bytes_bar.set_length(total);
                bytes_bar.set_position(downloaded);
            }
            Event::ProgressFiles { downloaded, total } => {
                files_bar.set_length(total);
                files_bar.set_position(downloaded);
            }
            Event::Message(msg) => log_bar.set_message(msg),
            Event::Error(err) => {
                tracing::error!("{err}");
                log_bar.set_message(format!("ERROR: {err}"));
            }
            Event::Finished(result) => {
                bytes_bar.finish();
                files_bar.finish();
                log_bar.finish_with_message(format!(
                    "{phase} - {}",
                    if result.is_ok() { "ok" } else { "failed" }
                ));
                if let Err(err) = result {
                    anyhow::bail!("{err}");
                }
            }
        }
    }

    drop(rx);

    handle
        .join()
        .map_err(|_| anyhow::anyhow!("download worker panicked"))??;

    Ok(())
}
