# Agent Instructions

## Repository Shape

- This is a single Rust 2021 binary crate; `src/main.rs` routes CLI commands through `src/cmd.rs` and the TUI through `src/tui/`.
- `src/download.rs` is the shared Sophon installer/updater engine; `src/cmd.rs` and `src/tui/app.rs` render its `download::Event` progress stream.
- API access is centralized in `src/api.rs`; edition mapping and voice-package selection/detection live in `src/edition.rs` and `src/voice.rs`.
- `anime-launcher-sdk` is a git dependency pinned to tag `1.35.10`; keep `Cargo.lock` consistent with intentional dependency changes.

## Tooling And Checks

- Use stable Rust and install the system protobuf compiler (`protoc`; Ubuntu package `protobuf-compiler`) before building.
- Match CI's check order: `cargo fmt --all --check`, then `cargo clippy --all-targets --locked -- -D warnings`, then `cargo test --locked`.
- Build the release artifact with `cargo build --release --locked`.
- Run one focused test with `cargo test --locked <test-name-or-filter>`.

## Runtime Constraints

- Reuse the same `--temp` directory to resume partial downloads or apply a `predownload` with a later `update`; the default is the OS temporary directory.
- `<game_dir>/.version` drives update detection; a missing version forces a full download, while an invalid version makes `update` fail before downloading.
- Downloads assemble in place and verified chunks persist on disk; the SDK has no mid-run cancellation API, so rerunning is the recovery path.
