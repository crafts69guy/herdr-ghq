#!/usr/bin/env bash
# Picker entrypoint: launch the herdr-ghq-switcher TUI (Rust). The binary is
# built on demand the first time — a herdr overlay pane hosts the whole thing,
# so agents/workspaces/repos are read live and the accept key opens/focuses.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bin/lib.sh
source "$SCRIPT_DIR/lib.sh"

PLUGIN_ROOT="${HERDR_PLUGIN_ROOT:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
export HERDR_PLUGIN_ROOT="$PLUGIN_ROOT"

BIN="$PLUGIN_ROOT/target/release/herdr-ghq-switcher"

# herdr's server env may lack the user's PATH additions; make sure common
# toolchain locations are reachable for the on-demand build.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

if [[ ! -x "$BIN" ]]; then
  command -v cargo >/dev/null 2>&1 ||
    die "Rust (cargo) is required to build the switcher. Install: brew install rust." "cargo not found on PATH"
  printf '\033[1mBuilding herdr-ghq switcher…\033[0m (first run only)\n\n'
  if ! cargo build --release --manifest-path "$PLUGIN_ROOT/Cargo.toml"; then
    die "Ghq could not build the switcher. Check the pane for cargo errors." "cargo build failed"
  fi
fi

exec "$BIN"
