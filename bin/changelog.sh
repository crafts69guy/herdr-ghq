#!/usr/bin/env bash
# Changelog pane: the switcher binary in --changelog mode, rendering the CHANGELOG.md
# that ships beside the installed plugin. No network.
#
# picker.sh owns prebuilt resolution, the Cargo fallback, and PATH fixup, so this is a wrapper
# rather than a copy of them.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

exec bash "$SCRIPT_DIR/picker.sh" --changelog
