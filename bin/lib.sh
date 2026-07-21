#!/usr/bin/env bash

# Shared helpers for the herdr-ghq plugin. Callers enable strict mode
# (set -euo pipefail) before sourcing this file. The reusable pieces here —
# config parsing, pane-context resolution, theme colors, notifications — follow
# the same contract as the sibling git-hub plugin so the two feel like one
# toolkit.

NOTIFICATIONS_ENABLED="true"
NOTIFICATION_POSITION="top-right"
# auto = honour each call's per-event sound; none/done/request force one for every toast.
NOTIFICATION_SOUND="auto"

log() {
  printf 'herdr-ghq: %s\n' "$*" >&2
}

die() {
  local message="$1"
  local detail="${2:-$message}"

  log "$detail"
  notify "$message" request
  exit 1
}

herdr_bin() {
  printf '%s\n' "${HERDR_BIN_PATH:-herdr}"
}

# Build the release switcher on demand (first run only) and echo its path on
# stdout; build progress goes to stderr so a caller can `bin="$(ensure_built)"`.
# herdr's server env may lack the user's PATH additions, so prepend common
# toolchain locations for the build. The picker, clone flow, and the `open` /
# `config` delegations all route through this one binary.
ensure_built() {
  local root="${HERDR_PLUGIN_ROOT:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
  local bin="$root/target/release/herdr-ghq-switcher"
  export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
  if [[ ! -x "$bin" ]]; then
    command -v cargo >/dev/null 2>&1 ||
      die "Rust (cargo) is required to build the switcher. Install: brew install rust." "cargo not found on PATH"
    printf '\033[1mBuilding herdr-ghq switcher…\033[0m (first run only)\n\n' >&2
    if ! cargo build --release --manifest-path "$root/Cargo.toml" >&2; then
      die "Ghq could not build the switcher. Check the pane for cargo errors." "cargo build failed"
    fi
  fi
  printf '%s\n' "$bin"
}

# Guard the review viewer, then refresh its themed chrome. hunk is the read-only
# review TUI bin/review.sh launches; it is a separate (Node) dependency, so fail
# with a clear install hint rather than a bare "command not found". The theme
# regeneration is best-effort — hunk falls back to its own theme.
ensure_hunk() {
  command -v hunk >/dev/null 2>&1 ||
    die "hunk is required for review — brew install hunk (or npm i -g hunkdiff)." "hunk not found on PATH"
  "$(ensure_built)" hunk-theme >/dev/null 2>&1 || true
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
  local enabled position sound

  enabled="$(toml_get notifications "$file" true)"
  position="$(toml_get notification_position "$file" top-right)"
  sound="$(toml_get notification_sound "$file" auto)"

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

  case "$sound" in
    auto | none | done | request) ;;
    *)
      if [[ "$allow_invalid" == "true" ]]; then
        log "invalid notification_sound '$sound' in $file; using auto until Settings repairs it"
        sound=auto
      else
        die \
          "Invalid notification_sound setting. Use auto, none, done, or request." \
          "invalid notification_sound '$sound' in $file"
      fi
      ;;
  esac

  NOTIFICATIONS_ENABLED="$enabled"
  NOTIFICATION_POSITION="$position"
  NOTIFICATION_SOUND="$sound"
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

notify() {
  local body="$1"
  # Per-event sound: done for completions, request for attention/errors, none for
  # neutral. A caller omits it for a plain toast. `notification_sound = auto` honours
  # this; any other config value forces one sound for every toast.
  local event_sound="${2:-none}"
  local command sound
  local response shown reason attempt

  [[ "$NOTIFICATIONS_ENABLED" == "true" ]] || return 0

  if [[ "$NOTIFICATION_SOUND" == "auto" ]]; then
    sound="$event_sound"
  else
    sound="$NOTIFICATION_SOUND"
  fi

  command=("$(herdr_bin)" notification show "Ghq" --body "$body" --sound "$sound")
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

# Focusing a workspace/agent and opening a repo at a target now live only in the
# Rust switcher (src/action.rs). The picker calls them directly; the clone flow
# reaches them through `herdr-ghq-switcher open` (see ensure_built), so the herdr
# verbs are no longer mirrored here.
