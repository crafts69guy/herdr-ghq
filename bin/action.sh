#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bin/lib.sh
source "$SCRIPT_DIR/lib.sh"

ACTION_ID="${HERDR_PLUGIN_ACTION_ID:-}"
[[ -n "$ACTION_ID" ]] || die "Ghq could not determine which action to run." "HERDR_PLUGIN_ACTION_ID is not set"

# Map each action to the overlay pane it opens and, for the hot-path actions,
# the Enter target the picker should force.
entrypoint=""
force_target=""
case "$ACTION_ID" in
  menu) entrypoint="picker" ;;
  open-workspace) entrypoint="picker"; force_target="workspace" ;;
  open-tab) entrypoint="picker"; force_target="tab" ;;
  open-split) entrypoint="picker"; force_target="split" ;;
  get) entrypoint="get" ;;
  settings) entrypoint="settings" ;;
  changelog) entrypoint="changelog" ;;
  *) die "Ghq received an unsupported action. Check plugin logs." "unknown plugin action '$ACTION_ID'" ;;
esac

command -v ghq >/dev/null 2>&1 || die "ghq is required — brew install ghq." "ghq not found on PATH"

pane_id="$(context_pane_id)"
cwd=""

# The settings dashboard is a fixed-size form, so it opens as a session-modal popup
# sized to its content: 17 settings + border + command bar = 20 rows, and a widest row
# of a 22-column key, a 14-column value, and a 45-column hint = 88. Cells rather than a
# percentage — the form does not get more readable on a 200-column monitor. Keep in step
# with SETTINGS in src/settings.rs; a smaller window scrolls rather than clipping.
# The picker stays a full overlay.
placement=(--placement overlay)
case "$entrypoint" in
  settings) placement=(--placement popup --width 88 --height 20) ;;
  # The changelog scrolls, so height is comfort rather than a fit.
  changelog) placement=(--placement popup --width 88 --height 28) ;;
esac

command=("$(herdr_bin)" plugin pane open --plugin ghq --entrypoint "$entrypoint" "${placement[@]}")
if cwd="$(active_cwd "$pane_id")"; then
  command+=(--cwd "$cwd" --env "GHQ_ORIGIN_CWD=$cwd")
fi
if [[ -n "$pane_id" ]]; then
  command+=(--env "GHQ_ORIGIN_PANE_ID=$pane_id")
fi
if [[ -n "$force_target" ]]; then
  command+=(--env "GHQ_FORCE_TARGET=$force_target")
fi

if ! "${command[@]}"; then
  die "Ghq could not open the $entrypoint pane. Check plugin logs." "herdr failed to open the ghq $entrypoint pane"
fi
