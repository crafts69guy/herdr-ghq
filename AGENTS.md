# Repository Guidelines

## Project Structure & Module Organization

The Rust TUI lives in `src/`: `main.rs` owns the event loop, while `data.rs`,
`ui.rs`, `preview.rs`, `action.rs`, and `history.rs` separate data loading,
rendering, previews, accepted actions, and recency state. Bash entrypoints in
`bin/` connect the TUI and fzf-based clone/settings flows to herdr. Plugin
metadata is defined in `herdr-plugin.toml`; sample user configuration belongs in
`examples/`, integration checks in `tests/`, and documentation images in `docs/`.
Do not commit generated `target/` artifacts.

## Build, Test, and Development Commands

- `cargo build` compiles a debug binary for quick iteration.
- `cargo build --release` produces the binary launched by `bin/picker.sh`.
- `cargo test` runs Rust unit tests, including filtering and history behavior.
- `bash tests/manifest_spec.sh` validates plugin actions, pane paths, and Bash
  syntax from a foreign working directory.
- `cargo fmt --check` verifies Rust formatting.
- `cargo clippy --all-targets -- -D warnings` treats lint findings as failures.
- `herdr plugin link /path/to/herdr-ghq` installs the checkout for manual testing;
  reload configuration with `herdr server reload-config`.

## Coding Style & Naming Conventions

Use rustfmt defaults (four-space indentation), `snake_case` for functions and
modules, and `PascalCase` for Rust types. Prefer typed errors with `anyhow::Result`
and avoid `unwrap()` in production paths. Bash scripts must use
`#!/usr/bin/env bash`, `set -euo pipefail`, quoted expansions, and shared helpers
from `bin/lib.sh`. Keep TOML keys and plugin action IDs snake_case and kebab-case,
respectively (for example, `default_target` and `open-workspace`).

## Testing Guidelines

Place focused Rust tests beside their module in `#[cfg(test)]` blocks and name
them after observable behavior. Extend `tests/manifest_spec.sh` when changing
the manifest or entrypoint contract. Before submitting, run both test commands,
formatting, and Clippy. Manually exercise the overlay for layout, keybinding, or
herdr CLI changes; attach a screenshot when visual output changes.

## Commit & Pull Request Guidelines

Recent commits use short, imperative summaries, often ending with a release tag
such as `(v0.4.0)`. Keep each commit focused. Pull requests should explain user
impact, list verification performed, link related issues, and call out required
herdr/ghq versions. For releases, keep versions in `Cargo.toml` and
`herdr-plugin.toml` synchronized.

## Safety & Configuration

Never hardcode credentials or user-specific paths. Verify real pane, workspace,
and agent IDs before issuing herdr commands. Preserve typed confirmation for
repository removal and test destructive flows against disposable repositories.
