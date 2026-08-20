use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::edition::Edition;

#[derive(Parser)]
#[command(
    name = "genshin-dl",
    version,
    about = "Genshin Impact downloader built on anime-launcher-sdk (Sophon protocol)"
)]
pub struct Cli {
    #[arg(long, value_enum, default_value_t = EditionArg::Global, help = "Game edition")]
    pub edition: EditionArg,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List available versions and download manifests (game + voiceover packages)
    List,

    /// Download and install the full game. Already valid files are skipped
    Download {
        /// Game installation directory
        dest: PathBuf,

        #[arg(long, default_value_t = 8, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..), help = "Number of download threads")]
        threads: usize,

        #[arg(long, help = "Temporary download folder; reuse to resume downloads")]
        temp: Option<PathBuf>,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Voiceover locale codes to install (e.g. en-us,ja-jp) or 'all'"
        )]
        voice: Vec<String>,

        #[arg(long, help = "Skip the disk free space check")]
        no_free_space_check: bool,
    },

    /// Update an existing installation via diff patches (falls back to a full download)
    Update {
        /// Game installation directory
        dest: PathBuf,

        #[arg(long, default_value_t = 8, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..), help = "Number of download threads")]
        threads: usize,

        #[arg(long, help = "Temporary download folder; reuse to resume downloads")]
        temp: Option<PathBuf>,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Voiceover locale codes (default: detect installed ones)"
        )]
        voice: Vec<String>,

        #[arg(long, help = "Skip the disk free space check")]
        no_free_space_check: bool,
    },

    /// Pre-download chunks of the upcoming version without applying them
    Predownload {
        /// Game installation directory (used to detect the current version)
        dest: PathBuf,

        #[arg(long, default_value_t = 8, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..), help = "Number of download threads")]
        threads: usize,

        #[arg(long, help = "Temporary download folder; reuse the same on install")]
        temp: Option<PathBuf>,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Voiceover locale codes (default: all available)"
        )]
        voice: Vec<String>,

        #[arg(long, help = "Skip the disk free space check")]
        no_free_space_check: bool,
    },

    /// Verify and repair an existing installation, re-downloading broken regions only
    Repair {
        /// Game installation directory
        dest: PathBuf,

        #[arg(long, default_value_t = 8, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..), help = "Number of download threads")]
        threads: usize,

        #[arg(long, help = "Temporary download folder; reuse to resume downloads")]
        temp: Option<PathBuf>,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Voiceover locale codes (default: detect installed ones)"
        )]
        voice: Vec<String>,

        #[arg(long, help = "Skip the disk free space check")]
        no_free_space_check: bool,
    },

    /// Interactive terminal user interface
    Tui,
}

#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditionArg {
    #[default]
    Global,
    China,
}

impl From<EditionArg> for Edition {
    fn from(value: EditionArg) -> Self {
        match value {
            EditionArg::Global => Edition::Global,
            EditionArg::China => Edition::China,
        }
    }
}
