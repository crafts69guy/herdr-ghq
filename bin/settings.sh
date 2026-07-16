#!/usr/bin/env bash
# Settings pane: a themed fzf dashboard over the plugin's flat config.toml.
# Enter cycles the highlighted option (or prompts, for split_ratio); Esc closes.
# The values are read by this plugin at runtime, so no server reload is needed.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bin/lib.sh
source "$SCRIPT_DIR/lib.sh"

PLUGIN_ROOT="${HERDR_PLUGIN_ROOT:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
CONFIG_DIR="${HERDR_PLUGIN_CONFIG_DIR:-$PLUGIN_ROOT/.config}"
CONFIG_FILE="$CONFIG_DIR/config.toml"

command -v fzf >/dev/null 2>&1 || die "fzf is required — brew install fzf." "fzf not found on PATH"
export FZF_DEFAULT_OPTS=""

# key|default|hint
SETTINGS=(
  "default_target|workspace|where Enter opens a repo"
  "split_direction|right|split growth direction"
  "split_ratio|0.5|split size (0.1–0.9)"
  "label|repo|workspace/tab label style"
  "include_agents|true|list running agents in the switcher"
  "include_workspaces|true|list open workspaces in the switcher"
  "sort|recent|resting list order (recent/name/kind)"
  "title_color|peach|box title colour (theme slot or #hex)"
  "preview|enabled|show the preview pane"
  "preview_position|down|down = full-width footer; right = side-by-side"
  "preview_readme|true|include README in the preview"
  "clone_source|clipboard|seed clone input from clipboard"
  "open_after_clone|true|open a repo right after cloning"
  "transparency|auto|popup background transparency"
  "notifications|true|show herdr notifications"
  "notification_position|top-right|notification corner"
)

config_set() {
  local key="$1" val="$2" file="$3" tmp
  mkdir -p -- "$(dirname -- "$file")"
  touch -- "$file"
  if grep -qE "^[[:space:]]*${key}[[:space:]]*=" "$file"; then
    tmp="$(mktemp)"
    awk -v k="$key" -v v="$val" '
      !done && $0 ~ "^[[:space:]]*" k "[[:space:]]*=" { print k " = \"" v "\""; done=1; next }
      { print }
    ' "$file" >"$tmp"
    mv -- "$tmp" "$file"
  else
    printf '%s = "%s"\n' "$key" "$val" >>"$file"
  fi
}

cycle() {
  local key="$1" cur="$2"
  case "$key" in
    default_target)
      case "$cur" in workspace) echo tab ;; tab) echo split ;; split) echo pane ;; *) echo workspace ;; esac ;;
    split_direction) [[ "$cur" == right ]] && echo down || echo right ;;
    label)
      case "$cur" in repo) echo owner-repo ;; owner-repo) echo path ;; *) echo repo ;; esac ;;
    sort)
      case "$cur" in recent) echo name ;; name) echo kind ;; *) echo recent ;; esac ;;
    preview) [[ "$cur" == enabled ]] && echo disabled || echo enabled ;;
    preview_position) [[ "$cur" == down ]] && echo right || echo down ;;
    title_color)
      case "$cur" in peach) echo mauve ;; mauve) echo teal ;; teal) echo blue ;; blue) echo accent ;; *) echo peach ;; esac ;;
    preview_readme | open_after_clone | notifications | include_agents | include_workspaces) [[ "$cur" == true ]] && echo false || echo true ;;
    clone_source) [[ "$cur" == clipboard ]] && echo prompt || echo clipboard ;;
    transparency)
      case "$cur" in auto) echo enabled ;; enabled) echo disabled ;; *) echo auto ;; esac ;;
    notification_position)
      case "$cur" in top-right) echo top-left ;; top-left) echo bottom-left ;; bottom-left) echo bottom-right ;; *) echo top-right ;; esac ;;
    split_ratio) echo "__prompt__" ;;
  esac
}

theme_fzf_args() {
  local accent text subtext surface overlay
  accent="$(theme_color accent)"
  text="$(theme_color text)"
  subtext="$(theme_color subtext0)"
  surface="$(theme_color surface1)"
  overlay="$(theme_color overlay0)"
  THEME_ARGS=()
  if [[ -n "$accent" ]]; then
    THEME_ARGS=(--color "fg:${text:--1},hl:${accent},fg+:${text:--1},bg+:${surface:--1},hl+:${accent},prompt:${accent},pointer:${accent},border:${overlay:--1},label:${accent}:bold,header:${subtext:--1}")
  fi
}
theme_fzf_args

render() {
  local spec key default hint cur
  for spec in "${SETTINGS[@]}"; do
    IFS='|' read -r key default hint <<<"$spec"
    cur="$(toml_get "$key" "$CONFIG_FILE" "$default")"
    printf '%-22s \033[1m%-12s\033[0m \033[2m%s\033[0m\n' "$key" "$cur" "$hint"
  done
}

while true; do
  choice="$(render | fzf \
    --ansi --reverse --no-multi --no-sort \
    --prompt='  ' --pointer='▌' \
    --margin=1,2 --padding=1,2 \
    --border=rounded --border-label=' 󰒓 Ghq Settings ' --border-label-pos=3 \
    --header='enter change · esc done' --header-first \
    "${THEME_ARGS[@]}")" || break

  key="${choice%% *}"
  [[ -n "$key" ]] || continue
  cur="$(toml_get "$key" "$CONFIG_FILE" '')"
  next="$(cycle "$key" "$cur")"

  if [[ "$next" == "__prompt__" ]]; then
    printf '\033[1m%s\033[0m (current: %s)\n' "$key" "${cur:-unset}"
    if IFS= read -r -e -i "${cur:-0.5}" -p 'New value: ' next; then
      next="$(printf '%s' "$next" | tr -d '[:space:]')"
      [[ -n "$next" ]] || continue
    else
      continue
    fi
  fi

  config_set "$key" "$next" "$CONFIG_FILE"
done
