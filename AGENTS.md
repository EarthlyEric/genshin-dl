# Agent Instructions

## Repository Shape

- Single Rust 2021 binary crate; `src/main.rs` routes CLI commands through `src/cmd.rs` and the TUI through `src/tui/`; `src/logging.rs` forwards tracing logs into the TUI event stream.
- `src/download.rs` is the shared Sophon installer/updater engine; `src/cmd.rs` and the TUI screens render its `download::Event` progress stream.
- API access is centralized in `src/api.rs`; edition mapping and voice-package selection/detection live in `src/edition.rs` and `src/voice.rs`.
- `anime-launcher-sdk` is a git dependency pinned to tag `1.35.10`; keep `Cargo.lock` consistent with intentional dependency changes.

## Tooling And Checks

- Use stable Rust and install the system protobuf compiler (`protoc`; Ubuntu package `protobuf-compiler`) before building.
- Match CI's check order: `cargo fmt --all --check`, then `cargo clippy --all-targets --locked -- -D warnings`, then `cargo test --locked`.
- Build the release artifact with `cargo build --release --locked`.
- Run one focused test with `cargo test --locked <test-name-or-filter>`.

## TUI (`src/tui/`)

- `src/tui/app.rs` owns the app state machine and key handling; per-screen rendering lives in `menu.rs`, `params.rs`, `progress.rs`, `result.rs`, shared helpers in `ui.rs`, and the voice picker in `voice_picker.rs`.
- Screens are only Menu, Params, Progress, Result — there are no separate picker screens. The file explorer and voice picker render inline in the Params middle zone; a picker is "open" when `dest_picker` / `voice_picker` is `Some`.
- Params navigation: ↑/↓/Tab cycle the four fields (dest, threads, voices, Start). `Enter` on the dest field opens the embedded `tui-file-explorer` picker, on voices opens the voice picker, and only on Start launches the worker (Enter elsewhere never starts it). `Esc` closes an open picker first, a second `Esc` returns to Menu.
- Explorer `space`/`n`/`r` keys are deliberately blocked (inline picker must not mutate the filesystem); `c` picks the current directory and closes the picker.
- Voice picker: `Space` toggles live and writes `app.voice` immediately; `none` and `all` are mutually exclusive with the concrete locale codes. `none` is meaningful only because `voice::resolve` returns empty for it regardless of `default_all`; an empty request keeps the action default (download: no voices; update/repair: detected installed; predownload: all).
- Rendering conventions: rounded borders throughout; key hints use `hint_line` (key names bold `Color::White`, descriptions `Color::DarkGray`); the global footer is its own rounded `DarkGray` block, so `ui()` reserves `Length(3)` for it.
- The explorer's own footer (status/disk info) is rendered by the `tui-file-explorer` crate; the key hints for it live in the app's global footer, not in the crate.

## Runtime Constraints

- Reuse the same `--temp` directory to resume partial downloads or apply a `predownload` with a later `update`; the default is the OS temporary directory.
- `<game_dir>/.version` drives update detection; a missing version forces a full download, while an invalid version makes `update` fail before downloading.
- Downloads assemble in place and verified chunks persist on disk; the SDK has no mid-run cancellation API, so rerunning is the recovery path.