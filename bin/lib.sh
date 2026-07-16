#!/usr/bin/env bash

# Shared helpers for the herdr-ghq plugin. Callers enable strict mode
# (set -euo pipefail) before sourcing this file. The reusable pieces here —
# config parsing, pane-context resolution, theme colors, notifications — follow
# the same contract as the sibling git-hub plugin so the two feel like one
# toolkit.

NOTIFICATIONS_ENABLED="true"
NOTIFICATION_POSITION="top-right"

log() {
  printf 'herdr-ghq: %s\n' "$*" >&2
}

die() {
  local message="$1"
  local detail="${2:-$message}"

  log "$detail"
  notify "$message"
  exit 1
}

herdr_bin() {
  printf '%s\n' "${HERDR_BIN_PATH:-herdr}"
}

# Read a scalar from the plugin's intentionally flat config.toml. Quoted strings,
# booleans, and bare values are supported; nested TOML is deliberately out of scope.
toml_get() {
  local key="$1"
  local file="$2"
  local default_value="${3-}"
  local value=""

  if [[ -f "$file" ]]; then
    value="$(awk -v wanted="$key" '
      /^[[:space:]]*#/ || /^[[:space:]]*$/ || /^[[:space:]]*\[/ { next }
      {
        line = $0
        sub(/^[[:space:]]*/, "", line)
        split(line, parts, "=")
        name = parts[1]
        sub(/[[:space:]]*$/, "", name)
        if (name != wanted) next

        sub(/^[^=]*=[[:space:]]*/, "", line)
        if (line ~ /^"/) {
          sub(/^"/, "", line)
          sub(/"[[:space:]]*(#.*)?$/, "", line)
        } else {
          sub(/[[:space:]]*#.*$/, "", line)
          sub(/[[:space:]]*$/, "", line)
        }
        print line
        exit
      }
    ' "$file")"
  fi

  if [[ -n "$value" ]]; then
    printf '%s\n' "$value"
  else
    printf '%s\n' "$default_value"
  fi
}

# Load the notification policy once a caller knows its plugin config path.
# Keep defaults active until both values validate so malformed notification
# config can still be reported to the user.
configure_notifications() {
  local file="$1"
  local allow_invalid="${2:-false}"
  local enabled position

  enabled="$(toml_get notifications "$file" true)"
  position="$(toml_get notification_position "$file" top-right)"

  case "$enabled" in
    true | false) ;;
    *)
      if [[ "$allow_invalid" == "true" ]]; then
        log "invalid notifications '$enabled' in $file; using true until Settings repairs it"
        enabled=true
      else
        die "Invalid notifications setting. Use true or false." "invalid notifications '$enabled' in $file"
      fi
      ;;
  esac

  case "$position" in
    '' | top-left | top-right | bottom-left | bottom-right) ;;
    *)
      if [[ "$allow_invalid" == "true" ]]; then
        log "invalid notification_position '$position' in $file; using top-right until Settings repairs it"
        position=top-right
      else
        die \
          "Invalid notification_position setting. Check the plugin config." \
          "invalid notification_position '$position' in $file"
      fi
      ;;
  esac

  NOTIFICATIONS_ENABLED="$enabled"
  NOTIFICATION_POSITION="$position"
}

# Herdr read commands emit JSON. Paths and IDs are plain strings in practice;
# this small extractor avoids adding jq as a launcher dependency.
json_string_value() {
  local key="$1"
  awk -v wanted="$key" '
    {
      marker = "\"" wanted "\""
      pos = index($0, marker)
      if (!pos) next
      value = substr($0, pos + length(marker))
      sub(/^[[:space:]]*:[[:space:]]*"/, "", value)
      if (value == $0) next
      sub(/".*/, "", value)
      gsub(/\\\//, "/", value)
      gsub(/\\\\/, "\\", value)
      print value
      exit
    }
  '
}

json_bool_value() {
  local key="$1"
  awk -v wanted="$key" '
    {
      marker = "\"" wanted "\""
      pos = index($0, marker)
      if (!pos) next
      value = substr($0, pos + length(marker))
      sub(/^[[:space:]]*:[[:space:]]*/, "", value)
      if (value ~ /^true/) print "true"
      if (value ~ /^false/) print "false"
      exit
    }
  '
}

context_value() {
  local key="$1"
  if [[ -n "${HERDR_PLUGIN_CONTEXT_JSON:-}" ]]; then
    printf '%s\n' "$HERDR_PLUGIN_CONTEXT_JSON" | json_string_value "$key"
  fi
}

context_pane_id() {
  local pane_id="${HERDR_PANE_ID:-${HERDR_ACTIVE_PANE_ID:-}}"
  if [[ -z "$pane_id" ]]; then
    pane_id="$(context_value pane_id)"
  fi
  printf '%s\n' "$pane_id"
}

pane_details() {
  local pane_id="$1"
  local herdr
  herdr="$(herdr_bin)"

  "$herdr" pane get "$pane_id" 2>/dev/null ||
    "$herdr" pane process-info --pane "$pane_id" 2>/dev/null
}

current_pane_details() {
  "$(herdr_bin)" pane current 2>/dev/null
}

pane_cwd() {
  local pane_id="$1"
  local details cwd
  details="$(pane_details "$pane_id")" || return 1
  cwd="$(printf '%s\n' "$details" | json_string_value foreground_cwd)"
  if [[ -z "$cwd" ]]; then
    cwd="$(printf '%s\n' "$details" | json_string_value cwd)"
  fi
  [[ -n "$cwd" ]] || return 1
  printf '%s\n' "$cwd"
}

active_cwd() {
  local pane_id="$1"
  local cwd="${HERDR_ACTIVE_PANE_CWD:-}"
  local details

  if [[ -z "$cwd" ]]; then
    cwd="$(context_value foreground_cwd)"
  fi
  if [[ -z "$cwd" ]]; then
    cwd="$(context_value cwd)"
  fi
  if [[ -z "$cwd" && -n "$pane_id" ]]; then
    cwd="$(pane_cwd "$pane_id")" || true
  fi
  # CLI-invoked plugin actions do not receive HERDR_PLUGIN_CONTEXT_JSON. This is
  # the path used from inside a popup pane: read the currently focused pane while
  # the popup is still alive and retain its cwd before the pane closes.
  if [[ -z "$cwd" && "${HERDR_ENV:-}" == "1" ]]; then
    details="$(current_pane_details)" || true
    cwd="$(printf '%s\n' "$details" | json_string_value foreground_cwd)"
    if [[ -z "$cwd" ]]; then
      cwd="$(printf '%s\n' "$details" | json_string_value cwd)"
    fi
  fi
  if [[ -z "$cwd" && "${HERDR_ENV:-}" != "1" ]]; then
    cwd="$PWD"
  fi

  [[ -n "$cwd" ]] || return 1
  printf '%s\n' "$cwd"
}

# Read one color slot from herdr's [theme.custom] (kept in sync with the user's
# terminal theme by theme plugins such as hue-theme). Empty when the slot or
# section is absent — callers fall back to default colors.
theme_color() {
  local key="$1"
  awk -v key="$key" '
    /^\[theme\.custom\]/ { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && $1 == key {
      if (match($0, /"#[0-9a-fA-F]{6}"/)) {
        print substr($0, RSTART + 1, RLENGTH - 2)
        exit
      }
    }
  ' "${HERDR_CONFIG_PATH:-$HOME/.config/herdr/config.toml}" 2>/dev/null
}

# "#rrggbb" -> "r;g;b" for ANSI truecolor escapes.
hex_rgb() {
  local h="${1#\#}"
  printf '%d;%d;%d' "0x${h:0:2}" "0x${h:2:2}" "0x${h:4:2}"
}

notify() {
  local body="$1"
  local command
  local response shown reason attempt

  [[ "$NOTIFICATIONS_ENABLED" == "true" ]] || return 0

  command=("$(herdr_bin)" notification show "Ghq" --body "$body")
  if [[ -n "$NOTIFICATION_POSITION" ]]; then
    command+=(--position "$NOTIFICATION_POSITION")
  fi

  # A successful API request can still return shown=false. Retry once during
  # transient popup teardown/focus transitions, then preserve the reason in
  # plugin logs without changing the original action's exit status.
  for attempt in 1 2; do
    if ! response="$("${command[@]}" 2>/dev/null)"; then
      log "notification unavailable: $body"
      return 0
    fi

    shown="$(printf '%s\n' "$response" | json_bool_value shown)"
    reason="$(printf '%s\n' "$response" | json_string_value reason)"
    if [[ "$shown" != "false" ]]; then
      return 0
    fi
    if [[ "$attempt" -eq 1 && ("$reason" == "busy" || "$reason" == "no_foreground_client") ]]; then
      sleep 0.1
      continue
    fi

    log "notification not shown (${reason:-unknown}): $body"
    return 0
  done
}

# --- ghq helpers ------------------------------------------------------------

ghq_root() {
  ghq root 2>/dev/null
}

# Turn a ghq list entry (relative "host/owner/repo") into a workspace/tab label.
repo_label() {
  local rel="$1"
  local mode="${2:-repo}"
  case "$mode" in
    owner-repo)
      printf '%s\n' "$rel" | awk -F/ '{ if (NF>=2) printf "%s/%s\n", $(NF-1), $NF; else print $NF }'
      ;;
    path)
      printf '%s\n' "$rel"
      ;;
    repo | *)
      basename -- "$rel"
      ;;
  esac
}

# Switch to an existing herdr workspace / agent (the unified switcher's
# non-repo entries). Targets come straight from `herdr workspace list` /
# `herdr agent list`, so they are trusted ids, not guessed ones.
focus_workspace() {
  "$(herdr_bin)" workspace focus "$1" >/dev/null ||
    die "Ghq could not switch to that workspace." "herdr workspace focus failed for '$1'"
}

focus_agent() {
  "$(herdr_bin)" agent focus "$1" >/dev/null ||
    die "Ghq could not jump to that agent." "herdr agent focus failed for '$1'"
}

# Open an absolute repo path at the requested herdr target. Split and pane
# targets act on the captured origin pane id — never a guessed one.
#   open_repo <workspace|tab|split|pane> <abs_path> <origin_pane_id> <label>
open_repo() {
  local target="$1"
  local path="$2"
  local origin_pane="$3"
  local label="$4"
  local herdr
  herdr="$(herdr_bin)"

  [[ -d "$path" ]] || die "Repository path no longer exists: $path" "open_repo: '$path' is not a directory"

  case "$target" in
    workspace)
      "$herdr" workspace create --cwd "$path" --label "$label" --focus >/dev/null ||
        die "Ghq could not open a workspace for $label." "herdr workspace create failed for $path"
      ;;
    tab)
      "$herdr" tab create --cwd "$path" --label "$label" --focus >/dev/null ||
        die "Ghq could not open a tab for $label." "herdr tab create failed for $path"
      ;;
    split)
      local dir="${GHQ_SPLIT_DIRECTION:-right}"
      local ratio="${GHQ_SPLIT_RATIO:-0.5}"
      local args=(pane split)
      [[ -n "$origin_pane" ]] && args+=("$origin_pane")
      args+=(--direction "$dir" --ratio "$ratio" --cwd "$path" --focus)
      "$herdr" "${args[@]}" >/dev/null ||
        die "Ghq could not split the pane for $label." "herdr pane split failed for $path"
      ;;
    pane)
      [[ -n "$origin_pane" ]] ||
        die "Ghq could not find the origin pane to cd into." "no origin pane id for 'pane' target"
      "$herdr" pane send-text "$origin_pane" "cd '$path'" >/dev/null ||
        die "Ghq could not send cd to the current pane." "herdr pane send-text failed for $origin_pane"
      "$herdr" pane send-keys "$origin_pane" enter >/dev/null ||
        die "Ghq could not submit cd in the current pane." "herdr pane send-keys enter failed for $origin_pane"
      ;;
    *)
      die "Ghq received an unknown open target '$target'." "unknown open target '$target'"
      ;;
  esac
}
