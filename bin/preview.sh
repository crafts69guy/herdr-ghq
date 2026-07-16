#!/usr/bin/env bash
# fzf preview for the unified switcher. Resilient by design (no set -e): a
# preview must never abort mid-render.
#   preview.sh <kind> <id> <dir>
# repo → git header + file tree + README; agent → status + recent output;
# workspace → tabs/panes summary.
set -uo pipefail

kind="${1:-repo}"
id="${2:-}"
dir="${3:-}"

hr() { printf '\033[2m%s\033[0m\n' "────────────────────────────────────"; }

repo_body() {
  local d="$1" name branch state last
  [[ -d "$d" ]] || { printf 'missing: %s\n' "$d"; return; }
  name="$(basename -- "$d")"
  branch="$(git -C "$d" symbolic-ref --short HEAD 2>/dev/null)"
  [[ -n "$branch" ]] || branch="$(git -C "$d" rev-parse --short HEAD 2>/dev/null)"
  if [[ -n "$(git -C "$d" status --porcelain 2>/dev/null)" ]]; then
    state=$'\033[33m● dirty\033[0m'
  else
    state=$'\033[32m✓ clean\033[0m'
  fi
  last="$(git -C "$d" log -1 --format='%cr · %s' 2>/dev/null)"

  printf '\033[1m %s\033[0m\n' "$name"
  printf '  \033[2m%s\033[0m   %b\n' "${branch:-—}" "$state"
  [[ -n "$last" ]] && printf '  \033[2m%s\033[0m\n' "$last"
  printf '\n'
  if command -v eza >/dev/null 2>&1; then
    eza --tree --level=2 --color=always --icons -- "$d" 2>/dev/null | head -48
  else
    # shellcheck disable=SC2012 # plain listing fallback; filenames here are benign
    ls -la -- "$d" 2>/dev/null | head -40
  fi
  if [[ "${GHQ_PREVIEW_README:-true}" == "true" ]]; then
    local readme
    readme="$(find "$d" -maxdepth 1 -iname 'readme*' 2>/dev/null | head -1)"
    if [[ -n "$readme" ]]; then
      printf '\n'; hr
      printf '\033[2m%s\033[0m\n' "$(basename -- "$readme")"
      head -30 -- "$readme"
    fi
  fi
}

case "$kind" in
  agent)
    if command -v jq >/dev/null 2>&1; then
      info="$(herdr agent get "$id" 2>/dev/null)"
      agent="$(printf '%s' "$info" | jq -r '.result.agent // .agent // "agent"' 2>/dev/null)"
      status="$(printf '%s' "$info" | jq -r '.result.agent_status // .agent_status // "unknown"' 2>/dev/null)"
      title="$(printf '%s' "$info" | jq -r '.result.terminal_title_stripped // .terminal_title_stripped // ""' 2>/dev/null)"
      printf '\033[1m● %s\033[0m  \033[2m%s\033[0m\n' "${agent:-agent}" "$status"
      [[ -n "$title" ]] && printf '  \033[2m%s\033[0m\n' "$title"
    else
      printf '\033[1m● agent\033[0m\n'
    fi
    [[ -n "$dir" ]] && printf '  \033[2m%s\033[0m\n' "$dir"
    printf '\n'; hr
    printf '\033[2mrecent output\033[0m\n'
    herdr agent read "$id" --source recent --lines 24 2>/dev/null | tail -24 ||
      printf '(no output available)\n'
    ;;
  workspace)
    if command -v jq >/dev/null 2>&1; then
      info="$(herdr workspace get "$id" 2>/dev/null)"
      printf '\033[1m %s\033[0m\n\n' \
        "$(printf '%s' "$info" | jq -r '.result.label // .label // "workspace"' 2>/dev/null)"
      printf '%s' "$info" | jq -r '
        (.result // .) as $w
        | "  \(($w.tab_count // 0)) tabs · \(($w.pane_count // 0)) panes · \($w.agent_status // "unknown")",
          "",
          ( ($w.tabs // [])[]? | "   \(.label // .tab_id // "tab")" )
      ' 2>/dev/null
    else
      printf '\033[1m workspace\033[0m\n  %s\n' "$id"
    fi
    ;;
  repo | *)
    repo_body "$dir"
    ;;
esac
