#!/usr/bin/env bash
# Picker pane: a unified herdr switcher. One themed fzf list blends three
# sources — running agents, open workspaces, and ghq repositories — and the
# accept key is kind-aware:
#
#   agent      enter → jump to it (herdr agent focus); open keys act on its cwd
#   workspace  enter → switch to it (herdr workspace focus)
#   repo       enter → open in the default target; ^w/^t/^s/^o pick where
#
#   ^t tab   ^s split   ^o cd current pane   ^w workspace
#   ^g git menu (repo/agent cwd)   ^u update repo   ^x remove repo
#   ⌥↵ clone a new repo
#
# herdr keybindings accept only one key after the prefix, so these popup keys
# stand in for the "prefix+p <target>" group. Agents/workspaces need jq; without
# it the picker gracefully falls back to repos only.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bin/lib.sh
source "$SCRIPT_DIR/lib.sh"

PLUGIN_ROOT="${HERDR_PLUGIN_ROOT:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
CONFIG_DIR="${HERDR_PLUGIN_CONFIG_DIR:-$PLUGIN_ROOT/.config}"
CONFIG_FILE="$CONFIG_DIR/config.toml"

configure_notifications "$CONFIG_FILE" true

command -v ghq >/dev/null 2>&1 || die "ghq is required — brew install ghq." "ghq not found on PATH"
command -v fzf >/dev/null 2>&1 || die "fzf is required — brew install fzf." "fzf not found on PATH"

default_target="$(toml_get default_target "$CONFIG_FILE" workspace)"
case "$default_target" in
  workspace | tab | split | pane) ;;
  *)
    log "invalid default_target '$default_target'; using workspace"
    default_target="workspace"
    ;;
esac
enter_target="${GHQ_FORCE_TARGET:-$default_target}"

label_mode="$(toml_get label "$CONFIG_FILE" repo)"
preview_enabled="$(toml_get preview "$CONFIG_FILE" enabled)"
include_agents="$(toml_get include_agents "$CONFIG_FILE" true)"
include_workspaces="$(toml_get include_workspaces "$CONFIG_FILE" true)"
GHQ_SPLIT_DIRECTION="$(toml_get split_direction "$CONFIG_FILE" right)"
GHQ_SPLIT_RATIO="$(toml_get split_ratio "$CONFIG_FILE" 0.5)"
export GHQ_SPLIT_DIRECTION GHQ_SPLIT_RATIO
export GHQ_PREVIEW_README
GHQ_PREVIEW_README="$(toml_get preview_readme "$CONFIG_FILE" true)"

# jq is required to parse herdr's JSON; hot-path repo actions stay repo-only.
if ! command -v jq >/dev/null 2>&1; then
  include_agents="false"
  include_workspaces="false"
fi
if [[ -n "${GHQ_FORCE_TARGET:-}" ]]; then
  include_agents="false"
  include_workspaces="false"
fi

transparency="$(toml_get transparency "$CONFIG_FILE" auto)"
[[ "$transparency" == "disabled" ]] && menu_transparent=false || menu_transparent=true

ROOT="$(ghq_root)"
[[ -n "$ROOT" ]] || die "ghq root is not configured." "ghq root returned empty"
export GHQ_ROOT="$ROOT"

# --- palette ---------------------------------------------------------------
R=$'\033[0m'
DIM=$'\033[2m'
sgrf() { # sgrf <hex> <ansi-fallback-code>
  if [[ -n "$1" ]]; then printf '\033[38;2;%sm' "$(hex_rgb "$1")"; else printf '\033[%sm' "$2"; fi
}
A="$(theme_color accent)"
TXT="$(theme_color text)"
SUB="$(theme_color subtext0)"
SURF="$(theme_color surface1)"
OVL="$(theme_color overlay0)"
PANEL="$(theme_color panel_bg)"
C_TXT="$(sgrf "$TXT" 39)"
C_GH="$(sgrf "$(theme_color mauve)" 35)"
C_BB="$(sgrf "$(theme_color blue)" 34)"
C_GL="$(sgrf "$(theme_color peach)" 33)"
C_GIT="$(sgrf "$SUB" 90)"
C_WS="$(sgrf "$A" 36)"
# Footer key colors (a colorful command bar).
C_A="$(sgrf "$A" 36)"
C_G="$(sgrf "$(theme_color green)" 32)"
C_Y="$(sgrf "$(theme_color yellow)" 33)"
C_B="$(sgrf "$(theme_color blue)" 34)"
C_M="$(sgrf "$(theme_color mauve)" 35)"
C_P="$(sgrf "$(theme_color peach)" 33)"
C_RD="$(sgrf "$(theme_color red)" 31)"

state_color() {
  case "$1" in
    idle) sgrf "$(theme_color green)" 32 ;;
    working) sgrf "$(theme_color yellow)" 33 ;;
    blocked) sgrf "$(theme_color red)" 31 ;;
    *) sgrf "$SUB" 90 ;;
  esac
}

# Fields: kind \t id \t dir \t display. Only the display is shown/searched;
# fzf returns the full line on accept so kind/id/dir survive the round-trip.
row() { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4"; }

collect_entries() {
  if [[ "$include_agents" == "true" ]]; then
    herdr agent list 2>/dev/null |
      jq -r '.result.agents[]? | [.terminal_id, .agent, .agent_status, (.foreground_cwd // .cwd // "")] | @tsv' |
      while IFS=$'\t' read -r tid agent status cwd; do
        [[ -n "$tid" ]] || continue
        local base sc prim disp
        base="$(basename -- "${cwd:-agent}")"
        sc="$(state_color "$status")"
        prim="$(printf '%-38s' "$base · $agent")"
        disp="$(printf '%s●%s %s%s%s %s%s%s' "$sc" "$R" "$C_TXT" "$prim" "$R" "$DIM" "$status" "$R")"
        row agent "$tid" "$cwd" "$disp"
      done
  fi

  if [[ "$include_workspaces" == "true" ]]; then
    herdr workspace list 2>/dev/null |
      jq -r '.result.workspaces[]? | [.workspace_id, .label, (.number|tostring), (.pane_count|tostring), (.focused|tostring)] | @tsv' |
      while IFS=$'\t' read -r wid label num panes focused; do
        [[ -n "$wid" ]] || continue
        local prim sec disp
        prim="$(printf '%-38s' "$label")"
        sec="#$num · ${panes}p"
        [[ "$focused" == "true" ]] && sec="$sec · current"
        disp="$(printf '%s%s%s %s%s%s %s%s%s' "$C_WS" "" "$R" "$C_TXT" "$prim" "$R" "$DIM" "$sec" "$R")"
        row workspace "$wid" "" "$disp"
      done
  fi

  local rel host rest icon ic prim disp
  while IFS= read -r rel; do
    [[ -n "$rel" ]] || continue
    host="${rel%%/*}"
    rest="${rel#*/}"
    case "$host" in
      github.com) icon=""; ic="$C_GH" ;;
      bitbucket.org) icon=""; ic="$C_BB" ;;
      gitlab.com) icon=""; ic="$C_GL" ;;
      *) icon=""; ic="$C_GIT" ;;
    esac
    prim="$(printf '%-38s' "$rest")"
    disp="$(printf '%s%s%s %s%s%s %s%s%s' "$ic" "$icon" "$R" "$C_TXT" "$prim" "$R" "$DIM" "${host%%.*}" "$R")"
    row repo "$rel" "$ROOT/$rel" "$disp"
  done < <(ghq list 2>/dev/null)
}

entries="$(collect_entries)"
if [[ -z "$entries" ]]; then
  notify "Nothing to switch to yet. Clone a repo with ghq get."
  exec bash "$SCRIPT_DIR/get.sh"
fi

# The user's interactive fzf defaults would shrink/restyle the popup; own it.
export FZF_DEFAULT_OPTS=""

background=-1
if [[ "$menu_transparent" == "false" ]]; then
  background="${PANEL:-#15191B}"
  SURF="${SURF:-#23282A}"
fi

# Full-width colorful command bar (fzf footer). Each key gets its own hue and a
# dim separator between groups.
fk() { printf '%s%s%s %s%s%s' "$1" "$2" "$R" "$C_TXT" "$3" "$R"; }
SEP="  ${C_GIT}│${R}  "
footer=" $(fk "$C_A" '↵' 'open')${SEP}$(fk "$C_G" '^t' 'tab')${SEP}$(fk "$C_Y" '^s' 'split')${SEP}$(fk "$C_B" '^o' 'cd')${SEP}$(fk "$C_M" '^w' 'workspace')${SEP}$(fk "$C_P" '^g' 'git')${SEP}$(fk "$C_A" '^u' 'update')${SEP}$(fk "$C_RD" '^x' 'remove')${SEP}$(fk "$C_M" '⌥↵' 'clone') "

# Layout mirrors codediff.nvim: a Search input box on top, a Results list box
# below, and a Preview box on the right — no outer wrapper border.
fzf_args=(
  --ansi --layout=reverse --no-multi --cycle
  --delimiter='\t' --with-nth=4 --nth=4
  --info=inline
  "--expect=ctrl-w,ctrl-t,ctrl-s,ctrl-o,ctrl-g,ctrl-u,ctrl-x,alt-enter"
  --prompt='  ' --pointer='▌'
  "--margin=1,1" "--padding=0"
  --input-border=rounded --input-label=' Search ' --input-label-pos=3
  --list-border=rounded --list-label=' Switcher ' --list-label-pos=3
  --footer="$footer" --footer-border=line
)

if [[ "$preview_enabled" != "disabled" ]]; then
  fzf_args+=(
    --preview "bash '$SCRIPT_DIR/preview.sh' {1} {2} {3}"
    --preview-window 'right:52%:wrap'
    --preview-border=rounded --preview-label=' 󰈈 Preview ' --preview-label-pos=3
  )
fi

if [[ -n "$A" ]]; then
  fzf_args+=(--color "fg:${TXT:--1},bg:${background},gutter:${background},hl:${A},fg+:${TXT:--1},bg+:${SURF:--1},hl+:${A},prompt:${A},pointer:${A},info:${SUB:--1},input-border:${OVL:--1},input-label:${A}:bold,list-border:${OVL:--1},list-label:${A}:bold,preview-border:${OVL:--1},preview-label:${SUB:--1},footer:${SUB:--1},footer-border:${OVL:--1}")
elif [[ "$menu_transparent" == "false" ]]; then
  fzf_args+=(--color "bg:${background},gutter:${background},bg+:${SURF}")
fi

out="$(printf '%s\n' "$entries" | fzf "${fzf_args[@]}")" || exit 0

key="$(printf '%s\n' "$out" | sed -n '1p')"
sel="$(printf '%s\n' "$out" | sed -n '2p')"

if [[ "$key" == "alt-enter" ]]; then
  exec bash "$SCRIPT_DIR/get.sh"
fi
[[ -n "$sel" ]] || exit 0

IFS=$'\t' read -r kind id dir _ <<<"$sel"
origin_pane="${GHQ_ORIGIN_PANE_ID:-}"

# --- accept ----------------------------------------------------------------
case "$kind" in
  agent)
    label="$(basename -- "${dir:-$id}")"
    # Open keys act on the agent's cwd; without one, fall back to jumping to it.
    open_kind=""
    case "$key" in
      ctrl-w) open_kind="workspace" ;;
      ctrl-t) open_kind="tab" ;;
      ctrl-s) open_kind="split" ;;
      ctrl-o) open_kind="pane" ;;
    esac
    if [[ -n "$open_kind" && -n "$dir" ]]; then
      open_repo "$open_kind" "$dir" "$origin_pane" "$label"
    else
      focus_agent "$id"
    fi
    ;;
  workspace)
    focus_workspace "$id"
    ;;
  repo)
    abs="$dir"
    label="$(repo_label "$id" "$label_mode")"
    case "$key" in
      '') open_repo "$enter_target" "$abs" "$origin_pane" "$label" ;;
      ctrl-w) open_repo workspace "$abs" "$origin_pane" "$label" ;;
      ctrl-t) open_repo tab "$abs" "$origin_pane" "$label" ;;
      ctrl-s) open_repo split "$abs" "$origin_pane" "$label" ;;
      ctrl-o) open_repo pane "$abs" "$origin_pane" "$label" ;;
      ctrl-g)
        open_repo tab "$abs" "$origin_pane" "$label"
        if "$(herdr_bin)" plugin list 2>/dev/null | grep -q '^- git-hub '; then
          "$(herdr_bin)" plugin action invoke menu --plugin git-hub >/dev/null 2>&1 ||
            log "git-hub menu handoff failed for $abs"
        else
          notify "git-hub is not installed — opened $label in a new tab."
        fi
        ;;
      ctrl-u)
        printf '\033[1mUpdating\033[0m %s\n\n' "$id"
        if ghq get -u -- "$id"; then notify "Updated $label."; else notify "Update failed for $label — check the pane."; fi
        printf '\n\033[2mpress any key to close\033[0m'
        read -rsn1 _ || true
        ;;
      ctrl-x)
        printf '\033[1;31mRemove repository\033[0m\n  %s\n\n' "$abs"
        printf 'This deletes the directory permanently.\n'
        printf "Type the repo name (\033[1m%s\033[0m) to confirm: " "$label"
        read -r reply || true
        if [[ "$reply" == "$label" ]]; then
          if rm -rf -- "$abs"; then notify "Removed $label."; else die "Ghq could not remove $label." "rm -rf failed for $abs"; fi
        else
          printf '\nAborted.\n'; notify "Removal of $label aborted."; sleep 0.6
        fi
        ;;
    esac
    ;;
  *)
    # ctrl-g on a repo path via handoff already handled; unknown key for
    # agent/workspace is a no-op (focus already applied above).
    log "no action for kind='$kind' key='$key'"
    ;;
esac
