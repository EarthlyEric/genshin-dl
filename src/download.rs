use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use anime_launcher_sdk::anime_game_core::sophon;
use anime_launcher_sdk::anime_game_core::sophon::api::schemas::sophon_diff::SophonDiff;
use anime_launcher_sdk::anime_game_core::sophon::api::schemas::sophon_manifests::{
    SophonDownloadInfo, SophonDownloads,
};
use anime_launcher_sdk::anime_game_core::sophon::installer::{
    SophonInstaller, Update as InstallerUpdate,
};
use anime_launcher_sdk::anime_game_core::sophon::reqwest::blocking::Client;
use anime_launcher_sdk::anime_game_core::sophon::updater::{
    SophonPatcher, Update as PatcherUpdate,
};
use anime_launcher_sdk::anime_game_core::sophon::utils::version::Version;
use anyhow::{bail, Context};

use crate::api;
use crate::edition::Edition;
use crate::voice;

/// Unified progress/status events emitted by the download engine.
#[derive(Debug, Clone)]
pub enum Event {
    Phase(String),
    ProgressBytes { downloaded: u64, total: u64 },
    ProgressFiles { downloaded: u64, total: u64 },
    Message(String),
    Error(String),
    Finished(Result<(), String>),
}

fn send(tx: &Sender<Event>, event: Event) {
    let _ = tx.send(event);
}

/// Download and install the full game, optionally with voiceover packages.
pub fn download(
    edition: Edition,
    game_dir: PathBuf,
    threads: usize,
    temp: Option<PathBuf>,
    voices: Vec<String>,
    no_free_space_check: bool,
    tx: Sender<Event>,
) -> anyhow::Result<()> {
    send(&tx, Event::Phase("Fetching game info".into()));

    let client = api::new_client();
    let branches = api::get_branches(&client, edition)?;
    let package = api::get_latest_package(&branches, edition, false)?;
    let downloads = api::get_downloads(&client, &package, edition)?;

    let game_info = downloads
        .get_manifests_for("game")
        .context("no game download manifest found")?
        .clone();

    let available = voice::voice_fields(
        downloads
            .manifests
            .iter()
            .map(|m| m.matching_field.as_str()),
    );
    let fields = voice::resolve(&voices, &available, false)?;

    std::fs::create_dir_all(&game_dir).context("failed to create game directory")?;
    let temp = temp.unwrap_or_else(std::env::temp_dir);

    install_download(
        &client,
        &game_info,
        &game_dir,
        &temp,
        threads,
        no_free_space_check,
        &tx,
    )?;

    for field in fields {
        let Some(info) = downloads.get_manifests_for(&field) else {
            continue;
        };
        install_download(
            &client,
            info,
            &game_dir,
            &temp,
            threads,
            no_free_space_check,
            &tx,
        )?;
    }

    write_version(&game_dir, &package.tag)?;

    send(&tx, Event::Message("Download complete".into()));
    Ok(())
}

/// Update an existing installation using diff patches, falling back to a full
/// download when the installed version is too old to patch.
pub fn update(
    edition: Edition,
    game_dir: PathBuf,
    threads: usize,
    temp: Option<PathBuf>,
    voices: Vec<String>,
    no_free_space_check: bool,
    tx: Sender<Event>,
) -> anyhow::Result<()> {
    send(&tx, Event::Phase("Fetching game info".into()));

    let client = api::new_client();
    let branches = api::get_branches(&client, edition)?;
    let package = api::get_latest_package(&branches, edition, false)?;
    let latest = package
        .version()
        .context("failed to parse latest version tag")?;
    let current = read_version(&game_dir).context("failed to read current version")?;

    if current == Some(latest) {
        send(&tx, Event::Phase("Already up to date".into()));
        send(
            &tx,
            Event::Message(format!("Already at the latest version {}", latest)),
        );
        return Ok(());
    }

    let temp = temp.unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&game_dir).context("failed to create game directory")?;

    let diffs = api::get_diffs(&client, &package, edition)?;
    let can_patch = current
        .as_ref()
        .map(|c| package.diff_tags.iter().any(|t| t == &c.to_string()))
        .unwrap_or(false);

    let game_diff = diffs.iter().find(|d| d.matching_field == "game");

    match (game_diff, current) {
        (Some(diff), Some(from)) if can_patch => {
            patch_diff(
                &client,
                diff,
                from,
                &game_dir,
                &temp,
                threads,
                no_free_space_check,
                &tx,
            )?;
        }
        _ => {
            let downloads = api::get_downloads(&client, &package, edition)?;
            let game_info = downloads
                .get_manifests_for("game")
                .context("no game download manifest found")?
                .clone();
            install_download(
                &client,
                &game_info,
                &game_dir,
                &temp,
                threads,
                no_free_space_check,
                &tx,
            )?;
        }
    }

    update_voices(
        &client,
        &diffs,
        &package,
        edition,
        &game_dir,
        &temp,
        threads,
        &voices,
        no_free_space_check,
        current,
        &tx,
    )?;

    write_version(&game_dir, &package.tag)?;

    send(&tx, Event::Message(format!("Updated to {}", latest)));
    Ok(())
}

/// Pre-download chunks of the upcoming version without applying them.
pub fn pre_download(
    edition: Edition,
    game_dir: PathBuf,
    threads: usize,
    temp: Option<PathBuf>,
    voices: Vec<String>,
    no_free_space_check: bool,
    tx: Sender<Event>,
) -> anyhow::Result<()> {
    send(&tx, Event::Phase("Fetching game info".into()));

    let client = api::new_client();
    let branches = api::get_branches(&client, edition)?;
    let package = api::get_latest_package(&branches, edition, true)?;
    let temp = temp.unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&temp).context("failed to create temp directory")?;

    let current = read_version(&game_dir).unwrap_or(None);
    let can_patch = current
        .as_ref()
        .map(|c| package.diff_tags.iter().any(|t| t == &c.to_string()))
        .unwrap_or(false);

    let diffs = api::get_diffs(&client, &package, edition).unwrap_or_default();
    let downloads = match api::get_downloads(&client, &package, edition) {
        Ok(downloads) => downloads,
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to fetch download info, treating as unavailable"
            );
            SophonDownloads {
                build_id: String::new(),
                tag: String::new(),
                manifests: vec![],
            }
        }
    };

    // Game
    match (
        diffs.iter().find(|d| d.matching_field == "game"),
        current,
        can_patch,
    ) {
        (Some(diff), Some(from), true) => {
            pre_download_diff(
                &client,
                diff,
                from,
                &temp,
                threads,
                no_free_space_check,
                &tx,
            )?;
        }
        _ => {
            if let Some(info) = downloads.get_manifests_for("game") {
                pre_download_manifest(&client, info, &temp, threads, no_free_space_check, &tx)?;
            }
        }
    }

    // Voiceover packages
    let available = voice::voice_fields(
        downloads
            .manifests
            .iter()
            .map(|m| m.matching_field.as_str()),
    );
    let fields = voice::resolve(&voices, &available, true)?;

    for field in fields {
        match (
            diffs.iter().find(|d| d.matching_field == field),
            current,
            can_patch,
        ) {
            (Some(diff), Some(from), true) => {
                pre_download_diff(
                    &client,
                    diff,
                    from,
                    &temp,
                    threads,
                    no_free_space_check,
                    &tx,
                )?;
            }
            _ => {
                if let Some(info) = downloads.get_manifests_for(&field) {
                    pre_download_manifest(&client, info, &temp, threads, no_free_space_check, &tx)?;
                }
            }
        }
    }

    send(
        &tx,
        Event::Message(
            "Pre-download complete, install with the same --temp to reuse the chunks".to_owned(),
        ),
    );
    Ok(())
}

/// Verify and repair an existing installation, re-downloading only broken
/// file regions.
pub fn repair(
    edition: Edition,
    game_dir: PathBuf,
    threads: usize,
    temp: Option<PathBuf>,
    voices: Vec<String>,
    no_free_space_check: bool,
    tx: Sender<Event>,
) -> anyhow::Result<()> {
    send(&tx, Event::Phase("Fetching game info".into()));

    let client = api::new_client();
    let branches = api::get_branches(&client, edition)?;
    let package = api::get_latest_package(&branches, edition, false)?;
    let downloads = api::get_downloads(&client, &package, edition)?;

    std::fs::create_dir_all(&game_dir).context("failed to create game directory")?;
    let temp = temp.unwrap_or_else(std::env::temp_dir);

    let mut manifests: Vec<SophonDownloadInfo> = vec![];
    if let Some(info) = downloads.get_manifests_for("game") {
        manifests.push(info.clone());
    }

    let installed = voice::detect_installed(&game_dir, edition);
    let fields = voice::resolve(&voices, &installed, true)?;
    for field in fields {
        if let Some(info) = downloads.get_manifests_for(&field) {
            manifests.push(info.clone());
        }
    }

    for info in manifests {
        repair_manifest(
            &client,
            &info,
            &game_dir,
            &temp,
            threads,
            no_free_space_check,
            &tx,
        )?;
    }

    send(&tx, Event::Message("Repair complete".into()));
    Ok(())
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn install_download(
    client: &Client,
    info: &SophonDownloadInfo,
    game_dir: &Path,
    temp: &Path,
    threads: usize,
    no_free_space_check: bool,
    tx: &Sender<Event>,
) -> anyhow::Result<()> {
    let mut installer = SophonInstaller::new(client.clone(), info, temp)
        .with_context(|| format!("failed to init installer for '{}'", info.matching_field))?;
    installer.chunks_in_mem = true;
    installer.chunks_queue_data_limit = Some(2 * 1024 * 1024 * 1024);
    installer.inplace = true;
    installer.check_free_space = !no_free_space_check;

    let failed = Arc::new(AtomicBool::new(false));
    let failed_clone = Arc::clone(&failed);

    let phase = format!("Downloading {}", info.matching_field);
    send(tx, Event::Phase(phase.clone()));
    installer.install(game_dir, threads, move |u| {
        send_installer_update(tx, u, &failed_clone, &phase);
    })?;

    if failed.load(Ordering::Acquire) {
        bail!("download failed for '{}'", info.matching_field);
    }

    Ok(())
}

fn pre_download_manifest(
    client: &Client,
    info: &SophonDownloadInfo,
    temp: &Path,
    threads: usize,
    no_free_space_check: bool,
    tx: &Sender<Event>,
) -> anyhow::Result<()> {
    let mut installer = SophonInstaller::new(client.clone(), info, temp)
        .with_context(|| format!("failed to init installer for '{}'", info.matching_field))?;
    installer.check_free_space = !no_free_space_check;

    let failed = Arc::new(AtomicBool::new(false));
    let failed_clone = Arc::clone(&failed);

    let phase = format!("Pre-downloading {}", info.matching_field);
    send(tx, Event::Phase(phase.clone()));
    installer.pre_download(threads, move |u| {
        send_installer_update(tx, u, &failed_clone, &phase)
    })?;

    if failed.load(Ordering::Acquire) {
        bail!("pre-download failed for '{}'", info.matching_field);
    }

    Ok(())
}

fn repair_manifest(
    client: &Client,
    info: &SophonDownloadInfo,
    game_dir: &Path,
    temp: &Path,
    threads: usize,
    no_free_space_check: bool,
    tx: &Sender<Event>,
) -> anyhow::Result<()> {
    let mut repairer = SophonInstaller::new(client.clone(), info, temp)
        .with_context(|| format!("failed to init repairer for '{}'", info.matching_field))?;
    repairer.mode_repair = true;
    repairer.inplace = true;
    repairer.chunks_in_mem = true;
    repairer.chunks_queue_data_limit = Some(2 * 1024 * 1024 * 1024);
    repairer.check_free_space = !no_free_space_check;

    let failed = Arc::new(AtomicBool::new(false));
    let failed_clone = Arc::clone(&failed);

    let phase = format!("Repairing {}", info.matching_field);
    send(tx, Event::Phase(phase.clone()));
    repairer.install(game_dir, threads, move |u| {
        send_installer_update(tx, u, &failed_clone, &phase);
    })?;

    let _ = std::fs::remove_dir_all(repairer.downloading_temp());

    if failed.load(Ordering::Acquire) {
        bail!("repair failed for '{}'", info.matching_field);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn patch_diff(
    client: &Client,
    diff: &SophonDiff,
    from: Version,
    game_dir: &Path,
    temp: &Path,
    threads: usize,
    no_free_space_check: bool,
    tx: &Sender<Event>,
) -> anyhow::Result<()> {
    let mut patcher = SophonPatcher::new(client.clone(), diff, temp, None)
        .with_context(|| format!("failed to init patcher for '{}'", diff.matching_field))?;
    patcher.check_free_space = !no_free_space_check;

    let failed = Arc::new(AtomicBool::new(false));
    let failed_clone = Arc::clone(&failed);

    send(
        tx,
        Event::Phase(format!("Patching {}", diff.matching_field)),
    );
    patcher.update(game_dir, from, threads, move |u| {
        send_patcher_update(tx, u, &failed_clone);
    })?;

    if failed.load(Ordering::Acquire) {
        bail!("patching failed for '{}'", diff.matching_field);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn pre_download_diff(
    client: &Client,
    diff: &SophonDiff,
    from: Version,
    temp: &Path,
    threads: usize,
    no_free_space_check: bool,
    tx: &Sender<Event>,
) -> anyhow::Result<()> {
    let mut patcher = SophonPatcher::new(client.clone(), diff, temp, None)
        .with_context(|| format!("failed to init patcher for '{}'", diff.matching_field))?;
    patcher.check_free_space = !no_free_space_check;

    let failed = Arc::new(AtomicBool::new(false));
    let failed_clone = Arc::clone(&failed);

    send(
        tx,
        Event::Phase(format!("Pre-downloading {}", diff.matching_field)),
    );
    patcher.pre_download(from, threads, move |u| {
        send_patcher_update(tx, u, &failed_clone);
    })?;

    if failed.load(Ordering::Acquire) {
        bail!("pre-download failed for '{}'", diff.matching_field);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn update_voices(
    client: &Client,
    diffs: &[SophonDiff],
    package: &sophon::api::schemas::game_branches::PackageInfo,
    edition: Edition,
    game_dir: &Path,
    temp: &Path,
    threads: usize,
    voices: &[String],
    no_free_space_check: bool,
    current: Option<Version>,
    tx: &Sender<Event>,
) -> anyhow::Result<()> {
    let installed = voice::detect_installed(game_dir, edition);
    let fields = voice::resolve(voices, &installed, true)?;
    let can_patch = current
        .as_ref()
        .map(|c| package.diff_tags.iter().any(|t| t == &c.to_string()))
        .unwrap_or(false);

    for field in fields {
        match (
            diffs.iter().find(|d| d.matching_field == field),
            current,
            can_patch,
        ) {
            (Some(diff), Some(from), true) => {
                patch_diff(
                    client,
                    diff,
                    from,
                    game_dir,
                    temp,
                    threads,
                    no_free_space_check,
                    tx,
                )?;
            }
            _ => {
                let downloads = api::get_downloads(client, package, edition)?;
                if let Some(info) = downloads.get_manifests_for(&field) {
                    install_download(
                        client,
                        info,
                        game_dir,
                        temp,
                        threads,
                        no_free_space_check,
                        tx,
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn send_installer_update(
    tx: &Sender<Event>,
    update: InstallerUpdate,
    failed: &Arc<AtomicBool>,
    phase: &str,
) {
    match update {
        InstallerUpdate::CheckingFreeSpace(_) => {
            send(tx, Event::Phase("Checking free space".into()))
        }
        InstallerUpdate::CheckingFiles { total_files } => {
            send(tx, Event::Phase("Checking files".into()));
            send(
                tx,
                Event::ProgressFiles {
                    downloaded: 0,
                    total: total_files,
                },
            );
        }
        InstallerUpdate::CheckingFilesProgress { passed, total } => send(
            tx,
            Event::ProgressFiles {
                downloaded: passed,
                total,
            },
        ),
        InstallerUpdate::DownloadingStarted {
            total_bytes,
            total_files,
            ..
        } => {
            send(tx, Event::Phase(phase.to_owned()));
            send(
                tx,
                Event::ProgressBytes {
                    downloaded: 0,
                    total: total_bytes,
                },
            );
            send(
                tx,
                Event::ProgressFiles {
                    downloaded: 0,
                    total: total_files,
                },
            );
        }
        InstallerUpdate::DownloadingProgressBytes {
            downloaded_bytes,
            total_bytes,
        } => send(
            tx,
            Event::ProgressBytes {
                downloaded: downloaded_bytes,
                total: total_bytes,
            },
        ),
        InstallerUpdate::DownloadingProgressFiles {
            downloaded_files,
            total_files,
        } => send(
            tx,
            Event::ProgressFiles {
                downloaded: downloaded_files,
                total: total_files,
            },
        ),
        InstallerUpdate::DownloadingFinished => send(tx, Event::Phase("Download finished".into())),
        InstallerUpdate::DownloadingError(err) => {
            failed.store(true, Ordering::Release);
            send(tx, Event::Error(err.to_string()));
        }
    }
}

fn send_patcher_update(tx: &Sender<Event>, update: PatcherUpdate, failed: &Arc<AtomicBool>) {
    match update {
        PatcherUpdate::CheckingFreeSpace(_) => send(tx, Event::Phase("Checking free space".into())),
        PatcherUpdate::CheckingFilesStarted => send(tx, Event::Phase("Checking files".into())),
        PatcherUpdate::DeletingStarted => send(tx, Event::Phase("Deleting unused files".into())),
        PatcherUpdate::DeletingProgress {
            deleted_files,
            total_unused,
        } => send(
            tx,
            Event::ProgressFiles {
                downloaded: deleted_files,
                total: total_unused,
            },
        ),
        PatcherUpdate::DeletingFinished => send(tx, Event::Phase("Deleting finished".into())),
        PatcherUpdate::DownloadingStarted(_) => send(tx, Event::Phase("Downloading update".into())),
        PatcherUpdate::DownloadingProgressBytes {
            downloaded_bytes,
            total_bytes,
        } => send(
            tx,
            Event::ProgressBytes {
                downloaded: downloaded_bytes,
                total: total_bytes,
            },
        ),
        PatcherUpdate::DownloadingFinished => send(tx, Event::Phase("Download finished".into())),
        PatcherUpdate::PatchingStarted => send(tx, Event::Phase("Patching".into())),
        PatcherUpdate::PatchingProgress {
            patched_files,
            total_files,
        } => send(
            tx,
            Event::ProgressFiles {
                downloaded: patched_files,
                total: total_files,
            },
        ),
        PatcherUpdate::PatchingFinished => send(tx, Event::Phase("Patching finished".into())),
        PatcherUpdate::DownloadingError(err) => {
            failed.store(true, Ordering::Release);
            send(tx, Event::Error(err.to_string()));
        }
        PatcherUpdate::PatchingError(err) => {
            failed.store(true, Ordering::Release);
            send(tx, Event::Error(err));
        }
        PatcherUpdate::FileHashCheckFailed(path) => {
            failed.store(true, Ordering::Release);
            send(
                tx,
                Event::Error(format!("file hash check failed: {}", path.display())),
            );
        }
    }
}

fn read_version(game_dir: &Path) -> anyhow::Result<Option<Version>> {
    let path = game_dir.join(".version");
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&path)?;
    let contents = contents.trim();
    if contents.is_empty() {
        return Ok(None);
    }

    Ok(Some(Version::from_str(contents).with_context(|| {
        format!("failed to parse version '{contents}'")
    })?))
}

fn write_version(game_dir: &Path, tag: &str) -> anyhow::Result<()> {
    std::fs::write(game_dir.join(".version"), tag)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phases_from(update: InstallerUpdate, phase: &str) -> Vec<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let failed = Arc::new(AtomicBool::new(false));
        send_installer_update(&tx, update, &failed, phase);
        drop(tx);
        rx.try_iter()
            .filter_map(|event| match event {
                Event::Phase(phase) => Some(phase),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn checking_files_advances_phase() {
        assert_eq!(
            phases_from(
                InstallerUpdate::CheckingFiles { total_files: 10 },
                "Repairing game"
            ),
            vec!["Checking files".to_owned()]
        );
    }

    #[test]
    fn downloading_started_restores_action_phase() {
        let phases = phases_from(
            InstallerUpdate::DownloadingStarted {
                location: PathBuf::from("."),
                total_bytes: 100,
                total_files: 2,
            },
            "Repairing game",
        );
        assert!(phases.contains(&"Repairing game".to_owned()));
    }

    #[test]
    fn checking_free_space_reports_phase() {
        assert_eq!(
            phases_from(
                InstallerUpdate::CheckingFreeSpace(PathBuf::from(".")),
                "Repairing game"
            ),
            vec!["Checking free space".to_owned()]
        );
    }
}
