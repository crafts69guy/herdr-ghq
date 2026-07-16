#!/usr/bin/env bash
# Picker pane entrypoint: a themed fzf list of ghq-managed repositories where
# the accept key decides *where* the repo opens. herdr keybindings accept only
# one key after the prefix (chords are rejected), so the popup's --expect keys
# stand in for the conceptual "prefix+p <target>" group.
#
#   enter    open in the configured default target (workspace unless forced)
#   ctrl-w   new workspace     ctrl-t   new tab
#   ctrl-s   split current     ctrl-o   cd in the current pane
#   ctrl-g   open in a new tab and hand off to the git-hub menu
#   ctrl-u   update (ghq get -u)   ctrl-x   remove (guarded)
#   alt-enter   clone a new repo (ghq get)
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bin/lib.sh
source "$SCRIPT_DIR/lib.sh"

PLUGIN_ROOT="${HERDR_PLUGIN_ROOT:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
CONFIG_DIR="${HERDR_PLUGIN_CONFIG_DIR:-$PLUGIN_ROOT/.config}"
CONFIG_FILE="$CONFIG_DIR/config.toml"

# Keep the picker usable when an option is malformed so the Settings repair path
# stays reachable; open_repo still validates targets strictly at use time.
configure_notifications "$CONFIG_FILE" true

command -v ghq >/dev/null 2>&1 || die "ghq is required — brew install ghq." "ghq not found on PATH"
command -v fzf >/dev/null 2>&1 || die "fzf is required — brew install fzf." "fzf not found on PATH"

default_target="$(toml_get default_target "$CONFIG_FILE" workspace)"
case "$default_target" in
  workspace | tab | split | pane) ;;
  *)
    log "invalid default_target '$default_target' in $CONFIG_FILE; using workspace until Settings repairs it"
    default_target="workspace"
    ;;
esac
# A hot-path action (open-tab, open-split, …) forces Enter's target.
enter_target="${GHQ_FORCE_TARGET:-$default_target}"

label_mode="$(toml_get label "$CONFIG_FILE" repo)"
preview_enabled="$(toml_get preview "$CONFIG_FILE" enabled)"
GHQ_SPLIT_DIRECTION="$(toml_get split_direction "$CONFIG_FILE" right)"
GHQ_SPLIT_RATIO="$(toml_get split_ratio "$CONFIG_FILE" 0.5)"
export GHQ_SPLIT_DIRECTION GHQ_SPLIT_RATIO
export GHQ_PREVIEW_README
GHQ_PREVIEW_README="$(toml_get preview_readme "$CONFIG_FILE" true)"

transparency="$(toml_get transparency "$CONFIG_FILE" auto)"
case "$transparency" in
  disabled) menu_transparent=false ;;
  *) menu_transparent=true ;;
esac

ROOT="$(ghq_root)"
[[ -n "$ROOT" ]] || die "ghq root is not configured." "ghq root returned empty"
export GHQ_ROOT="$ROOT"

repos="$(ghq list 2>/dev/null)" || die "Ghq could not list repositories." "ghq list failed"
if [[ -z "$repos" ]]; then
  notify "No repositories under $ROOT yet. Clone one with ghq get."
  # Jump straight to the clone flow so an empty root is still actionable.
  exec bash "$SCRIPT_DIR/get.sh"
fi

# The user's interactive fzf defaults (e.g. --height=40%, custom colors) would
# shrink and restyle the popup; this pane owns its full appearance.
export FZF_DEFAULT_OPTS=""

accent="$(theme_color accent)"
text="$(theme_color text)"
subtext="$(theme_color subtext0)"
surface="$(theme_color surface1)"
overlay="$(theme_color overlay0)"
panel="$(theme_color panel_bg)"

background=-1
if [[ "$menu_transparent" == "false" ]]; then
  background="${panel:-#15191B}"
  surface="${surface:-#23282A}"
fi

fzf_args=(
  --ansi --reverse --no-multi --cycle
  "--expect=ctrl-w,ctrl-t,ctrl-s,ctrl-o,ctrl-g,ctrl-u,ctrl-x,alt-enter"
  --prompt='  ' --pointer='▌'
  "--margin=1,2" "--padding=1,2"
  --border=rounded --border-label=" 󰊢 Projects · ${enter_target} " --border-label-pos=3
  --header='enter open · ^w ws · ^t tab · ^s split · ^o cd · ^g git · ^u update · ^x remove · ⌥↵ clone'
  --header-first
)

if [[ "$preview_enabled" != "disabled" ]]; then
  fzf_args+=(
    --preview "bash '$SCRIPT_DIR/preview.sh' {}"
    --preview-window 'right:55%:border-rounded:wrap'
    --preview-label ' 󰙅 Repo '
  )
fi

if [[ -n "$accent" ]]; then
  fzf_args+=(--color "fg:${text:--1},bg:${background},gutter:${background},hl:${accent},fg+:${text:--1},bg+:${surface:--1},hl+:${accent},prompt:${accent},pointer:${accent},border:${overlay:--1},label:${accent}:bold,header:${subtext:--1},preview-border:${overlay:--1}")
elif [[ "$menu_transparent" == "false" ]]; then
  fzf_args+=(--color "bg:${background},gutter:${background},bg+:${surface}")
fi

out="$(printf '%s\n' "$repos" | fzf "${fzf_args[@]}")" || exit 0

key="$(printf '%s\n' "$out" | sed -n '1p')"
rel="$(printf '%s\n' "$out" | sed -n '2p')"

# alt-enter clones a new repo regardless of the current selection.
if [[ "$key" == "alt-enter" ]]; then
  exec bash "$SCRIPT_DIR/get.sh"
fi

[[ -n "$rel" ]] || exit 0
abs="$ROOT/$rel"
label="$(repo_label "$rel" "$label_mode")"
origin_pane="${GHQ_ORIGIN_PANE_ID:-}"

case "$key" in
  '') open_repo "$enter_target" "$abs" "$origin_pane" "$label" ;;
  ctrl-w) open_repo workspace "$abs" "$origin_pane" "$label" ;;
  ctrl-t) open_repo tab "$abs" "$origin_pane" "$label" ;;
  ctrl-s) open_repo split "$abs" "$origin_pane" "$label" ;;
  ctrl-o) open_repo pane "$abs" "$origin_pane" "$label" ;;
  ctrl-g)
    # Land the repo in a focused tab, then hand off to the git-hub menu, which
    # reads the now-active pane. Falls back to just opening the tab when git-hub
    # is not installed.
    open_repo tab "$abs" "$origin_pane" "$label"
    if "$(herdr_bin)" plugin list 2>/dev/null | grep -q '^- git-hub '; then
      "$(herdr_bin)" plugin action invoke menu --plugin git-hub >/dev/null 2>&1 ||
        log "git-hub menu handoff failed for $abs"
    else
      notify "git-hub is not installed — opened $label in a new tab."
    fi
    ;;
  ctrl-u)
    printf '\033[1mUpdating\033[0m %s\n\n' "$rel"
    if ghq get -u -- "$rel"; then
      notify "Updated $label."
    else
      notify "Update failed for $label — check the pane."
    fi
    printf '\n\033[2mpress any key to close\033[0m'
    read -rsn1 _ || true
    ;;
  ctrl-x)
    printf '\033[1;31mRemove repository\033[0m\n  %s\n\n' "$abs"
    printf 'This deletes the directory permanently.\n'
    printf "Type the repo name (\033[1m%s\033[0m) to confirm: " "$label"
    read -r reply || true
    if [[ "$reply" == "$label" ]]; then
      if rm -rf -- "$abs"; then
        notify "Removed $label."
      else
        die "Ghq could not remove $label." "rm -rf failed for $abs"
      fi
    else
      printf '\nAborted.\n'
      notify "Removal of $label aborted."
      sleep 0.6
    fi
    ;;
  *)
    die "Ghq received an unexpected key '$key'." "unexpected fzf key '$key'"
    ;;
esac
