#!/usr/bin/env bash
# The repo file tree for the switcher's preview card (shelled out to by
# src/preview.rs).
#
# This is all that is left in bash. eza already emits the ANSI the card passes
# through, so producing it here costs nothing; every other part of the preview
# is styled in Rust from the user's herdr theme, which bash cannot read.
#
# Resilient by design (no set -e): a preview must never abort mid-render.
#   preview.sh <dir>
set -uo pipefail

dir="${1:-}"
[[ -d "$dir" ]] || exit 0

if command -v eza >/dev/null 2>&1; then
  # Pruning the usual heavy dirs keeps the tree cheap on big repos; walking
  # them costs more than the 48 lines we keep. (--git-ignore is slower still:
  # it reads the ignore rules for every entry.)
  # `tail -n +2` drops eza's root line: it is the absolute path, which the
  # card's own `path` row already carries.
  eza --tree --level=2 --color=always --icons \
    -I 'node_modules|.git|target|dist|build|.next|vendor' -- "$dir" 2>/dev/null |
    tail -n +2 | head -48
else
  # shellcheck disable=SC2012 # plain listing fallback; filenames here are benign
  ls -la -- "$dir" 2>/dev/null | head -40
fi
