#!/usr/bin/env bash
# Settings pane: the switcher binary in --settings mode, a themed TUI over the
# plugin's flat config.toml. Enter cycles the highlighted option (or edits it in
# place, for split_ratio); Esc closes. The values are read by this plugin at
# runtime, so no server reload is needed.
#
# picker.sh owns the on-demand cargo build and the PATH fixup, so this is a wrapper
# rather than a copy of them.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

exec bash "$SCRIPT_DIR/picker.sh" --settings
