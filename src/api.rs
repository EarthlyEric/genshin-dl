use anime_launcher_sdk::anime_game_core::sophon::api;
use anime_launcher_sdk::anime_game_core::sophon::api::schemas::game_branches::{
    GameBranches, PackageInfo,
};
use anime_launcher_sdk::anime_game_core::sophon::api::schemas::sophon_diff::SophonDiff;
use anime_launcher_sdk::anime_game_core::sophon::api::schemas::sophon_manifests::SophonDownloads;
use anime_launcher_sdk::anime_game_core::sophon::reqwest::blocking::Client;
use anyhow::Context;

use crate::edition::Edition;

pub fn new_client() -> Client {
    Client::new()
}

pub fn get_branches(client: &Client, edition: Edition) -> anyhow::Result<GameBranches> {
    api::get_game_branches_info(client, &edition.sophon())
        .context("failed to fetch game branches info")
}

pub fn get_latest_package(
    branches: &GameBranches,
    edition: Edition,
    preload: bool,
) -> anyhow::Result<PackageInfo> {
    branches
        .get_package_by_id_or_biz_latest(edition.game_id(), preload)
        .cloned()
        .context(if preload {
            "no pre-download package available"
        } else {
            "failed to find latest game package"
        })
}

pub fn get_downloads(
    client: &Client,
    package: &PackageInfo,
    edition: Edition,
) -> anyhow::Result<SophonDownloads> {
    api::get_game_download_sophon_info(client, package, &edition.sophon())
        .context("failed to fetch game download info")
}

pub fn get_diffs(
    client: &Client,
    package: &PackageInfo,
    edition: Edition,
) -> anyhow::Result<Vec<SophonDiff>> {
    Ok(
        api::get_game_diffs_sophon_info(client, package, &edition.sophon())
            .context("failed to fetch game diffs info")?
            .manifests,
    )
}
