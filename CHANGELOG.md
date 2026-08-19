# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-08-19

### Added

- CLI with `list`, `download`, `update`, `predownload`, `repair` and `tui` subcommands
- Interactive TUI (ratatui) with menu, parameters, progress gauges and log view
- Global and China edition support (`--edition global|china`)
- Chunked download of game and voiceover packages via the Sophon protocol (anime-launcher-sdk)
- Multi-threaded downloads with per-chunk md5 verification
- Resume support: partial chunks are saved and verified on disk, re-running skips valid files
- Diff-patch based updates with fallback to a full download
- Disk free space check (`--no-free-space-check` to skip)
- Version display via `--version` and in the TUI title bar