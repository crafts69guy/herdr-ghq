#!/usr/bin/env bash
# fzf preview for a highlighted ghq repo. Resilient by design (no set -e): a
# preview must never abort mid-render. Receives the ghq list entry (relative
# "host/owner/repo") as $1; GHQ_ROOT and GHQ_PREVIEW_README come from the picker.
set -uo pipefail

rel="${1:-}"
[[ -n "$rel" ]] || exit 0

root="${GHQ_ROOT:-$(ghq root 2>/dev/null)}"
dir="$root/$rel"
[[ -d "$dir" ]] || {
  printf 'missing: %s\n' "$dir"
  exit 0
}

name="$(basename -- "$rel")"

branch="$(git -C "$dir" symbolic-ref --short HEAD 2>/dev/null)"
[[ -n "$branch" ]] || branch="$(git -C "$dir" rev-parse --short HEAD 2>/dev/null)"

if [[ -n "$(git -C "$dir" status --porcelain 2>/dev/null)" ]]; then
  state=$'\033[33m● dirty\033[0m'
else
  state=$'\033[32m✓ clean\033[0m'
fi
last="$(git -C "$dir" log -1 --format='%cr · %s' 2>/dev/null)"

printf '\033[1m %s\033[0m  \033[2m%s\033[0m\n' "$name" "$rel"
printf '   %s   %b\n' "${branch:-—}" "$state"
[[ -n "$last" ]] && printf '  \033[2m%s\033[0m\n' "$last"
printf '\n'

if command -v eza >/dev/null 2>&1; then
  eza --tree --level=2 --color=always --icons -- "$dir" 2>/dev/null | head -60
else
  # shellcheck disable=SC2012 # plain listing fallback; filenames here are benign
  ls -la -- "$dir" 2>/dev/null | head -40
fi

if [[ "${GHQ_PREVIEW_README:-true}" == "true" ]]; then
  readme="$(find "$dir" -maxdepth 1 -iname 'readme*' 2>/dev/null | head -1)"
  if [[ -n "$readme" ]]; then
    printf '\n\033[2m── %s ──\033[0m\n' "$(basename -- "$readme")"
    head -40 -- "$readme"
  fi
fi
