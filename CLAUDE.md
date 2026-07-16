# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A [herdr](https://herdr.dev) plugin providing a unified switcher over three sources — running
herdr **agents**, open herdr **workspaces**, and **ghq repos** — in one fuzzy picker. It is a
Rust TUI (ratatui + nucleo), not an fzf wrapper; the clone and settings flows are still bash + fzf.
See `README.md` for user-facing keybindings and configuration.

## Commands

```bash
cargo build                                  # debug binary
cargo build --release                        # what bin/picker.sh actually launches
cargo test                                   # unit tests (sorting, group filter, history parsing)
cargo test recent_sort_puts_latest_opened_first   # single test by name
cargo fmt --check
cargo clippy --all-targets -- -D warnings    # warnings are failures
bash tests/manifest_spec.sh                  # manifest/entrypoint contract, version sync, bash syntax
bash bin/release.sh 0.5.0                    # cut a release (gates, bump, changelog, tag, gh release)

herdr plugin link /path/to/herdr-ghq         # install this checkout for manual testing
herdr server reload-config                   # after touching keybindings/config
herdr plugin config-dir ghq                  # where the runtime config.toml lives
```

There is no test runner for the bash layer beyond `tests/manifest_spec.sh`. Changes to overlay
layout, keybindings, or herdr CLI calls need manual exercise in a real herdr session.

## Architecture

**Two layers, joined by environment variables.** Every action starts in bash and may end in Rust:

1. `bin/action.sh` is the single entrypoint for all six manifest actions. It maps the action id
   (via `HERDR_PLUGIN_ACTION_ID`) to an overlay pane id (`picker` / `get` / `settings`), captures
   the **origin pane id and cwd** before the overlay steals focus, and passes them forward as
   `GHQ_ORIGIN_PANE_ID` / `GHQ_ORIGIN_CWD` on `herdr plugin pane open`.
2. `bin/picker.sh` builds `target/release/herdr-ghq-switcher` on demand (first run only) and
   `exec`s it. It prepends common toolchain paths to `PATH` because herdr's server env lacks the
   user's shell additions.
3. The TUI (`src/`) loads entries, runs the event loop, and — **after `ratatui::restore()`** —
   dispatches the accepted action. Interactive accepts (clone prompt, remove confirmation, `ghq
   get -u` output) deliberately run on the torn-down terminal, not inside the TUI.

**Why the origin pane matters:** `split` and `pane` targets act on the captured `GHQ_ORIGIN_PANE_ID`.
The overlay pane is *not* the user's pane. Never guess or infer a pane/workspace/agent id — every id
must come from `herdr agent list`, `herdr workspace list`, or the captured origin.

**Module split (`src/`):**

- `main.rs` — `App` state, `handle_key` → `Flow` (Continue/Quit/Accept), `browse_order`
- `data.rs` — `Theme`, `Config`, `Entry`, and `load()` which shells out to `herdr agent list`,
  `herdr workspace list`, and `ghq list`
- `ui.rs` — three-row layout: Search (3) / body (list + optional preview) / full-width command bar (1)
- `preview.rs` — shells out to `bin/preview.sh` and converts its ANSI via `ansi-to-tui`
- `action.rs` — `Accept` enum → herdr CLI verbs
- `history.rs` — recency state at `$XDG_STATE_HOME/herdr-ghq/recent.tsv`, atomic write, cap 200

**Sort vs. search:** fuzzy score always wins while a query is present; `SortMode` (recent/name/kind)
only orders the resting, no-query list. Both paths honour the `GroupFilter`. Ties break on load
order so the list stays stable.

## Non-obvious constraints

- **The bash layer duplicates the Rust layer on purpose.** `bin/lib.sh` has its own `open_repo`,
  `focus_workspace`, `focus_agent` that mirror `src/action.rs`. The bash copies serve `get.sh`
  (clone flow); the Rust copies serve the picker. **Behaviour changes to open targets must land in
  both**, or the clone flow and the picker will diverge.
- **Config parsing is intentionally flat.** Both `Config::load` (`src/data.rs`) and `toml_get`
  (`bin/lib.sh`) are hand-rolled line parsers — one `key = value` per line, no sections, no nesting.
  Do not add a TOML crate or nested keys without changing both parsers and `bin/settings.sh`.
  Theme parsing (`[theme.custom]` from herdr's config) is a separate hand-rolled scanner.
- **`jq` is optional.** Agents and workspaces degrade to repos-only without it. `bin/lib.sh` uses
  awk-based `json_string_value` / `json_bool_value` precisely to avoid a hard jq dependency in the
  launcher path.
- **`GHQ_FORCE_TARGET` overrides `default_target` for Enter, repos only.** `bin/action.sh` exports it
  for the `open-workspace` / `open-tab` / `open-split` hot-path actions; `src/action.rs`
  (`forced_target` + `resolve_default_target`) resolves it once in `main` and passes it to
  `dispatch`. Enter on an **agent** or **workspace** still focuses that entry — forcing a target
  only changes where a *repo* lands, matching the manifest's "Pick a repo; Enter opens it in…".
  Invalid values on either the env var or the config degrade to `workspace` instead of erroring.
- **Version sync:** `Cargo.toml` and `herdr-plugin.toml` versions must match; `tests/manifest_spec.sh`
  enforces it. `bin/release.sh` bumps both, so bump through it rather than by hand.
- **The changelog is the release notes.** Every user-facing change adds a line to
  `CHANGELOG.md`'s `[Unreleased]` section *in the same commit*; `bin/release.sh` promotes that
  section to a dated one and feeds it verbatim to `gh release create`. Commits are not
  Conventional Commits and nothing is generated from `git log` — an empty `[Unreleased]` aborts
  the release.
- **`ctrl-x` (remove) is the only destructive path.** It requires typing the repo name to confirm.
  Preserve that; test against disposable repos.
- **Pane commands must launch through `$HERDR_PLUGIN_ROOT`** — `tests/manifest_spec.sh` asserts the
  exact manifest string, since herdr starts panes from the user's repo, not the plugin checkout.

## Conventions

Rustfmt defaults; `anyhow::Result` with typed errors; no `unwrap()` in production paths. Bash uses
`#!/usr/bin/env bash`, `set -euo pipefail`, quoted expansions, and helpers from `bin/lib.sh`.
TOML keys are snake_case; plugin action ids are kebab-case. Commits are short and imperative, often
ending with a release tag like `(v0.4.0)`. Never commit `target/`.
