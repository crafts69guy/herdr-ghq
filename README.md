# herdr-ghq

A [herdr](https://herdr.dev) plugin that turns one keypress into a **unified switcher**.
A single themed list blends three things you jump between all day — your running
coding **agents**, your open **workspaces**, and every [`ghq`](https://github.com/x-motemen/ghq)
repository — and the accept key does the right thing for whatever you land on.

Where a shell `ghq list | fzf | cd` can only change the current directory, this leans into
herdr as a multiplexer: jump to a live agent, switch workspaces, or open a repo **where you
want it** — a new workspace, tab, split, or the current pane.

![The herdr-ghq switcher: a colourful fuzzy list with live preview and the `?` keybindings popup open](docs/switcher.png)

## Features

- **One list, three sources.** Type-to-filter across agents (● colored by state),
  workspaces, and repos. Each row shows a kind icon, a bold primary name, and dim
  context — repos drop the repeated `host/owner/` prefix for a clean, scannable list.
- **Kind-aware accept:**

  | Highlighted   | `enter`                                                             |
  | ------------- | ------------------------------------------------------------------- |
  | **agent**     | jump to it (`herdr agent focus`)                                    |
  | **workspace** | switch to it (`herdr workspace focus`)                              |
  | **repo**      | open in the default target (a new **workspace**, unless overridden) |

- **For repos (and an agent's cwd), choose where it lands:**

  | Key         | Opens in…                                         |
  | ----------- | ------------------------------------------------- |
  | `ctrl-w`    | a new **workspace**                               |
  | `ctrl-t`    | a new **tab**                                     |
  | `ctrl-s`    | a **split** of the current pane                   |
  | `ctrl-o`    | the **current pane** (`cd`)                       |
  | `ctrl-g`    | a new tab, then hands off to the **git-hub** menu |
  | `ctrl-u`    | _(repo)_ `ghq get -u` on the highlighted repo     |
  | `ctrl-x`    | _(repo)_ remove it, behind a typed confirmation   |
  | `alt-enter` | switch to the **clone** flow (`ghq get`)          |

- **Browse controls** — shape the list without leaving your keyboard:

  | Key                 | Does                                                                                                                                     |
  | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
  | `tab` / `shift-tab` | cycle the **group** filter — All → Agents → Workspaces → Repos (empty groups are skipped); the active tab is shown in the Switcher title |
  | `alt-s`             | cycle the **sort**: `recent` (latest opened) → `name` → `kind`                                                                           |
  | `alt-p`             | **toggle** the preview pane on/off                                                                                                       |
  | `?`                 | toggle a **keybindings cheatsheet** popup (any key closes it)                                                                            |

  The **default sort is `recent`** — repos you opened most recently float to the top.
  Opens are remembered in `${XDG_STATE_HOME:-~/.local/state}/herdr-ghq/recent.tsv`.
  While you're typing, fuzzy match score orders the list; sort applies to the resting
  (no-query) list. Set the startup default with `sort = "recent" | "name" | "kind"`.

- **A real TUI** (Rust, ratatui + nucleo) — not an fzf wrapper — so the layout is
  keifu-grade: a Search box on top, the Switcher list, a Preview pane below, and a
  **full-width colourful command bar pinned to the very bottom**.
- **Themed** from herdr's `[theme.custom]` so it matches your terminal, with a kind-aware
  preview: repos show branch · dirty/clean · last commit + a file tree + README; agents
  show status and recent output; workspaces show their tabs and panes.
- **Clone flow** that seeds its prompt from a repo URL on your clipboard, then opens the
  fresh checkout with your default target.
- **Settings dashboard** — the `ghq.settings` action opens the switcher's TUI as a
  session-modal popup over the flat config: a form you walk with `↑`/`↓`, `enter` cycles
  the value. Changes apply on the next invocation, no server reload needed.

> Agents and workspaces need [`jq`](https://jqlang.github.io/jq/). Without it the switcher
> gracefully falls back to repos only.

## Requirements

- herdr ≥ 0.7.4
- [`ghq`](https://github.com/x-motemen/ghq)
- [Rust / `cargo`](https://rustup.rs) — the switcher TUI is built on demand the first
  time you open it (`brew install rust`)
- Optional: [`eza`](https://github.com/eza-community/eza) (richer preview tree),
  the [`git-hub`](https://github.com/crafts69guy/herdr-git-hub) plugin (`ctrl-g` handoff).

## Install

```sh
herdr plugin install crafts69guy/herdr-ghq
# or, for local development:
herdr plugin link /path/to/herdr-ghq
```

Then add a keybinding to `~/.config/herdr/config.toml` (see `examples/keybindings.toml`)
and reload:

```toml
[[keys.command]]
key = "prefix+space"
type = "plugin_action"
command = "ghq.menu"
description = "Project switcher (ghq)"
```

```sh
herdr server reload-config
```

Press `prefix+space` to open the picker. `ghq.get` opens the clone flow directly,
`ghq.settings` opens the settings dashboard, and `ghq.changelog` shows what changed.

## Configuration

Settings live in a flat `config.toml` in the plugin's config dir
(`herdr plugin config-dir ghq`). Edit it directly, copy `examples/config.toml`, or run
the `ghq.settings` action. See `examples/config.toml` for every key; highlights:

- `default_target` — `workspace` (default) · `tab` · `split` · `pane`
- `include_agents` / `include_workspaces` — blend agents/workspaces into the list (needs `jq`)
- `sort` — resting list order: `recent` (default) · `name` · `kind` (cycle live with `alt-s`)
- `label` — workspace/tab label: `repo` · `owner-repo` · `path`
- `preview` / `preview_readme` — the preview pane
- `clone_source` — seed the clone prompt from the `clipboard` (default) or start blank
- `split_direction` / `split_ratio` — geometry for split targets
- `update_check` — ask GitHub once a day whether a newer version is tagged (`true` by
  default). Shows `↑ v0.6.0` in the command bar; never installs anything. Set to `false`
  and the plugin makes no outbound requests.

## How it works

Each action (`bin/action.sh`) captures the origin pane id and cwd, then opens a pane —
an overlay for `picker` and `get`, a session-modal popup for `settings` and `changelog`.
The picker is a
Rust TUI (`src/`, built to
`target/release/herdr-ghq-switcher` by `bin/picker.sh` on first run) that reads
`herdr agent list`, `herdr workspace list`, and `ghq list`, fuzzy-filters with nucleo,
and previews the selection (reusing `bin/preview.sh`). On accept it maps the key to a
herdr CLI verb — `agent focus` / `workspace focus` / `workspace create` / `tab create` /
`pane split` / `pane send-text` — always targeting the captured origin pane or a real id
from herdr, never a guessed one. The settings dashboard and the changelog viewer are the
same binary in `--settings` / `--changelog` mode (their `bin/` scripts exec
`bin/picker.sh` with the flag, reusing its on-demand build); only the clone flow is still
bash (`bin/get.sh`). Removal (`ctrl-x`) is the only destructive path and always requires
typing the repo name to confirm.

## Changelog

Run the `ghq.changelog` action to read it in a popup, with the version you are running
marked — or see [`CHANGELOG.md`](CHANGELOG.md).

Releases are tagged `vX.Y.Z`; to update an installed copy, re-run the install command —
it re-fetches the ref:

```sh
herdr plugin install crafts69guy/herdr-ghq
herdr server reload-config
```

Watch the repository (Watch → Custom → Releases) to hear about new versions.

## License

MIT — see `LICENSE`.
