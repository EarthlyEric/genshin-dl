# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.2.0] - 2026-08-19

### Added

- Embedded file explorer for selecting the game directory (inline in the TUI params screen)
- Voice pack picker with live selection (`none` / `all` / concrete locale codes)
- Log panel streaming tracing output into the TUI, with per-screen rendering split into dedicated modules
- Per-phase progress display and early state transition during file checks

### Fixed

- Download phase advancing during file check and download steps

## [v0.1.0] - 2026-08-19

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