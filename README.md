# genshin-dl

CLI & TUI downloader for Genshin Impact, built on the
[anime-launcher-sdk](https://github.com/an-anime-team/anime-launcher-sdk). Supports Global and China editions.

## Commands

```
genshin-dl [--edition global|china] <COMMAND>

  list                      List available versions and download manifests
  download <dest>           Download and install the full game
  update   <dest>           Update via diff patches (falls back to a full download)
  predownload <dest>        Pre-download chunks of the upcoming version
  repair   <dest>           Verify and repair broken file regions
  tui                       Interactive terminal interface
```

Common options for download/update/predownload/repair:

- `--threads N` — number of download threads (default `8`)
- `--temp DIR` — temporary download folder; **reuse the same folder to resume**
  partial downloads / apply a pre-download
- `--voice en-us,ja-jp` — voiceover locale codes to install (`all` = everything,
  default: none for `download`, auto-detected installed voices for `update`/`repair`,
  all for `predownload`)
- `--no-free-space-check` — skip the disk free space check

## Usage examples

```sh
# Show available versions and packages
genshin-dl list
genshin-dl --edition china list

# Full install with two voiceovers (resume-safe)
genshin-dl download ~/Games/Genshin --threads 8 --temp ~/.cache/genshin-dl --voice en-us,ja-jp

# Update an existing installation
genshin-dl update ~/Games/Genshin

# Pre-download the next version now, apply it later
genshin-dl predownload ~/Games/Genshin --temp ~/.cache/genshin-dl
genshin-dl update ~/Games/Genshin --temp ~/.cache/genshin-dl

# Repair broken files
genshin-dl repair ~/Games/Genshin

# Interactive interface
genshin-dl tui
```

## How it works

The tool talks directly to HoYoverse's Sophon API (`getGameBranches`,
`getBuild`, `getPatchBuild`) using the `anime-launcher-sdk`. Files are split
into fixed-size chunks, each downloaded with its own HTTP request, md5-verified,
and assembled at the correct offset into the target files. Download and
assembly run on separate thread pools; chunks are kept in memory (up to 2 GiB)
and already-verified files are skipped, so re-running a command resumes cleanly.

Notes:

- `install`/`update` never report an error for a partial failure — they always
  report broken chunks through progress events. The tool surfaces these as
  errors and exits non-zero.
- The version is tracked in `<game_dir>/.version`. Update detection relies on
  it; if it's missing the tool falls back to a full download.
- The worker cannot be cancelled mid-run (the SDK has no cancel API), but
  because chunks are saved and verified on disk, the next run simply resumes.

## Build

```sh
cargo build --release
```
