#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/herdr-plugin.toml"

fail() {
  printf 'manifest_spec: %s\n' "$*" >&2
  exit 1
}

# Every pane entrypoint must launch through HERDR_PLUGIN_ROOT so herdr can start
# it from the originating repo, not the plugin checkout.
assert_rooted_pane_command() {
  local script="$1"
  local expected
  expected="command = [\"bash\", \"-c\", \"exec bash \\\"\$HERDR_PLUGIN_ROOT/bin/$script\\\"\"]"

  grep -Fqx -- "$expected" "$MANIFEST" ||
    fail "$script must be launched through HERDR_PLUGIN_ROOT"
}

assert_rooted_pane_command picker.sh
assert_rooted_pane_command get.sh
assert_rooted_pane_command settings.sh

# Every declared action dispatches through bin/action.sh.
for action in menu get settings open-workspace open-tab open-split; do
  grep -Fq "id = \"$action\"" "$MANIFEST" || fail "action '$action' is not declared"
done

# The pane script must resolve from an unrelated working directory.
foreign_cwd="$(mktemp -d)"
trap 'rm -rf "$foreign_cwd"' EXIT
(
  cd "$foreign_cwd"
  HERDR_PLUGIN_ROOT="$ROOT" bash -c 'test -f "$HERDR_PLUGIN_ROOT/bin/picker.sh"'
) || fail "pane command could not resolve the plugin script from a foreign cwd"

# Every bin script must be syntactically valid bash.
for script in "$ROOT"/bin/*.sh; do
  bash -n "$script" || fail "syntax error in $script"
done

printf 'manifest_spec: ok\n'
