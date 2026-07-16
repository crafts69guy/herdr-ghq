# herdr-ghq

![herdr 0.7.4+](https://img.shields.io/badge/herdr-0.7.4%2B-lightgrey)
![ghq required](https://img.shields.io/badge/ghq-required-green)
![platform macOS | Linux](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
![license MIT](https://img.shields.io/badge/license-MIT-green)

A [herdr](https://herdr.dev) plugin that puts your running **agents**, open **workspaces**,
and every [`ghq`](https://github.com/x-motemen/ghq) **repository** in one fuzzy switcher —
and makes `enter` do the right thing for whatever you land on.

Where `ghq list | fzf | cd` can only change a directory, this uses herdr as a multiplexer:
jump to a live agent, switch workspaces, or open a repo exactly where you want it — a new
workspace, tab, split, or the current pane. It is a Rust TUI (ratatui + nucleo); no fzf
required.

![The herdr-ghq switcher: a fuzzy list with live preview and the `?` keybindings popup open](docs/switcher.png)

## Requirements

| | |
| --- | --- |
| **herdr** ≥ 0.7.4 | the host multiplexer |
| **[`ghq`](https://github.com/x-motemen/ghq)** | repository source |
| **[Rust / `cargo`](https://rustup.rs)** | the TUI builds on demand the first time you open it |
| _optional_ **[`eza`](https://github.com/eza-community/eza)** | richer preview tree |
| _optional_ **[`git-hub`](https://github.com/crafts69guy/herdr-git-hub)** | the `ctrl-g` handoff |

## Install

```sh
herdr plugin install crafts69guy/herdr-ghq
```

Bind a key in `~/.config/herdr/config.toml` (see [`examples/keybindings.toml`](examples/keybindings.toml)):

```toml
[[keys.command]]
key = "prefix+space"
type = "plugin_action"
command = "ghq.menu"
description = "Project switcher (ghq)"
```

Reload, then press `prefix+space`:

```sh
herdr server reload-config
```

## Keybindings

**Accept** (`enter`) is kind-aware:

| Highlighted | `enter` |
| --- | --- |
| **agent** | jump to it (`herdr agent focus`) |
| **workspace** | switch to it (`herdr workspace focus`) |
| **repo** | open it in `default_target` — a new workspace unless configured otherwise |

**Open a repo** (or an agent's cwd) somewhere specific:

| Key | Opens in… |
| --- | --- |
| `ctrl-w` | a new **workspace** |
| `ctrl-t` | a new **tab** |
| `ctrl-s` | a **split** of the current pane |
| `ctrl-o` | the **current pane** (`cd`) |
| `ctrl-g` | a new tab, handed off to the **git-hub** menu |

**Repo actions:**

| Key | Does |
| --- | --- |
| `ctrl-u` | `ghq get -u` on the highlighted repo |
| `ctrl-x` | remove it — requires typing the repo name to confirm |
| `alt-enter` | switch to the **clone** flow (`ghq get`) |

**Browse:**

| Key | Does |
| --- | --- |
| `tab` / `shift-tab` | cycle the group filter — All → Agents → Workspaces → Repos (empty groups skipped) |
| `alt-s` | cycle the sort: `recent` → `name` → `kind` |
| `alt-p` | toggle the preview pane |
| `alt-j` / `alt-k` | scroll the preview without moving the selection |
| `alt-c` | read the changelog over the list (`esc` returns to the same entry) |
| `alt-u` | update the plugin itself (vs `ctrl-u`, which updates the highlighted repo) |
| `?` | toggle the keybindings cheatsheet |

Sorting defaults to `recent`, so repos you opened last float to the top; opens are recorded
in `${XDG_STATE_HOME:-~/.local/state}/herdr-ghq/recent.tsv`. While you type, fuzzy score
orders the list — sort only applies to the resting, no-query list.

## Actions

Bind any of these the same way as `ghq.menu`:

| Action | Does |
| --- | --- |
| `ghq.menu` | the switcher |
| `ghq.get` | the clone flow |
| `ghq.settings` | the settings dashboard |
| `ghq.changelog` | what changed, with your installed version marked |
| `ghq.update-plugin` | install a newer version (refuses to touch a `link`ed checkout) |
| `ghq.open-workspace` · `ghq.open-tab` · `ghq.open-split` | the switcher with `enter`'s repo target forced |

## Configuration

Settings live in a flat `config.toml` in the plugin's config dir (`herdr plugin config-dir ghq`).
Edit it directly, copy [`examples/config.toml`](examples/config.toml), or run `ghq.settings` —
a form you walk with `↑`/`↓`, where `enter` cycles each value. Changes apply on the next
invocation; no server reload needed.

Every key is documented in `examples/config.toml`. The ones you're most likely to want:

| Key | Values |
| --- | --- |
| `default_target` | `workspace` (default) · `tab` · `split` · `pane` |
| `include_agents` / `include_workspaces` | blend agents/workspaces into the list |
| `sort` | `recent` (default) · `name` · `kind` |
| `label` | workspace/tab label: `repo` · `owner-repo` · `path` |
| `preview` / `preview_readme` | the preview pane |
| `clone_source` | seed the clone prompt from the `clipboard` (default) or start blank |
| `split_direction` / `split_ratio` | geometry for split targets |
| `update_check` | ask GitHub once a day whether a newer version is tagged (`true` by default) |

The switcher is themed from herdr's `[theme.custom]`, and previews each kind as a card —
a header with the entry's state as a pill, aligned `label value` rows, then bodies under
captioned rules. Repos show branch · clean/dirty · last commit, a file tree, and a README
excerpt rendered as markdown; agents show what they are doing and their recent output, in
the agent's own colours; workspaces list their tabs, each with its live status. Long cards
scroll with `alt-j` / `alt-k`.

`update_check` only ever shows `↑ v0.6.0` in the command bar — it never installs anything.
Set it to `false` and the plugin makes no outbound requests at all.

## How it works

Each action starts in `bin/action.sh`, which captures the origin pane id and cwd before the
new pane steals focus, then opens that pane — an overlay for the picker and clone flow, a
popup for settings and the changelog.

The picker itself is the Rust TUI in `src/`, built to `target/release/herdr-ghq-switcher` by
`bin/picker.sh` on first run. It reads `herdr agent list`, `herdr workspace list`, and
`ghq list`, fuzzy-filters with nucleo, and previews the selection as a card drawn in your
herdr theme colours — `bin/preview.sh` supplies only the repo's file tree. On
accept it maps the key to a herdr CLI verb — `agent focus`, `workspace focus`,
`workspace create`, `tab create`, `pane split`, `pane send-text` — always targeting the
captured origin pane or a real id from herdr, never a guessed one.

The settings dashboard and changelog viewer are the same binary in `--settings` /
`--changelog` mode. Only the clone flow is still bash (`bin/get.sh`).

## Contributing

Issues and pull requests are welcome. Start here:

```sh
git clone https://github.com/crafts69guy/herdr-ghq
cd herdr-ghq
herdr plugin link "$PWD"        # install this checkout
herdr server reload-config
```

Before you open a PR:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
bash tests/manifest_spec.sh      # manifest contract, version sync, bash syntax
bash tests/update_guard_spec.sh  # the update guard (herdr is stubbed, never called for real)
```

CI runs exactly those five. Two things are easy to miss:

- **Any user-visible change adds a line to `CHANGELOG.md`'s `[Unreleased]` section in the
  same commit** — `bin/release.sh` promotes that section verbatim into the GitHub release
  notes and nothing is generated from `git log`, so an entry not written there is lost.
- **Don't bump versions by hand.** `Cargo.toml` and `herdr-plugin.toml` must match, and
  `bin/release.sh` bumps both.

Layout, keybinding, and herdr CLI changes need manual exercise in a real herdr session —
there is no test runner for the overlay. Please attach a screenshot when visual output
changes, and test `ctrl-x` against disposable repos.

[`AGENTS.md`](AGENTS.md) has the full conventions: module layout, coding style, testing, and
the safety rules around herdr ids and destructive flows.

## Changelog

Run `ghq.changelog` to read it in a popup with your installed version marked, or see
[`CHANGELOG.md`](CHANGELOG.md). Releases are tagged `vX.Y.Z`; to update, re-run the install
command (it re-fetches the ref) or use the `ghq.update-plugin` action. Watch the repository
(Watch → Custom → Releases) to hear about new versions.

## License

MIT — see [`LICENSE`](LICENSE).
