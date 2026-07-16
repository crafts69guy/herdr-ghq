# herdr-ghq

A [herdr](https://herdr.dev) plugin that turns one keypress into a **unified switcher**.
A single themed fzf list blends three things you jump between all day — your running
coding **agents**, your open **workspaces**, and every [`ghq`](https://github.com/x-motemen/ghq)
repository — and the accept key does the right thing for whatever you land on.

Where a shell `ghq list | fzf | cd` can only change the current directory, this leans into
herdr as a multiplexer: jump to a live agent, switch workspaces, or open a repo **where you
want it** — a new workspace, tab, split, or the current pane.

## Features

- **One list, three sources.** Type-to-filter across agents (● colored by state),
  workspaces, and repos. Each row shows a kind icon, a bold primary name, and dim
  context — repos drop the repeated `host/owner/` prefix for a clean, scannable list.
- **Kind-aware accept:**

  | Highlighted | `enter` |
  |-------------|---------|
  | **agent** | jump to it (`herdr agent focus`) |
  | **workspace** | switch to it (`herdr workspace focus`) |
  | **repo** | open in the default target (a new **workspace**, unless overridden) |

- **For repos (and an agent's cwd), choose where it lands:**

  | Key | Opens in… |
  |-----|-----------|
  | `ctrl-w` | a new **workspace** |
  | `ctrl-t` | a new **tab** |
  | `ctrl-s` | a **split** of the current pane |
  | `ctrl-o` | the **current pane** (`cd`) |
  | `ctrl-g` | a new tab, then hands off to the **git-hub** menu |
  | `ctrl-u` | *(repo)* `ghq get -u` on the highlighted repo |
  | `ctrl-x` | *(repo)* remove it, behind a typed confirmation |
  | `alt-enter` | switch to the **clone** flow (`ghq get`) |

- **Themed** from herdr's `[theme.custom]` so it matches your terminal, with a kind-aware
  preview: repos show branch · dirty/clean · last commit + a file tree + README; agents
  show status and recent output; workspaces show their tabs and panes.
- **Clone flow** that seeds its prompt from a repo URL on your clipboard, then opens the
  fresh checkout with your default target.
- **Settings dashboard** — the `ghq.settings` action opens a themed fzf toggler over the
  flat config; changes apply on the next invocation, no server reload needed.

> Agents and workspaces need [`jq`](https://jqlang.github.io/jq/). Without it the switcher
> gracefully falls back to repos only.

## Requirements

- herdr ≥ 0.7.4
- [`ghq`](https://github.com/x-motemen/ghq) and [`fzf`](https://github.com/junegunn/fzf)
- Optional: [`jq`](https://jqlang.github.io/jq/) (agents + workspaces in the list),
  [`eza`](https://github.com/eza-community/eza) (richer preview tree),
  the [`git-hub`](https://github.com/crafts69guy/herdr-git-hub) plugin (`ctrl-g` handoff)

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
key = "prefix+p"
type = "plugin_action"
command = "ghq.menu"
description = "Project switcher (ghq)"
```

```sh
herdr server reload-config
```

Press `prefix+p` to open the picker. `ghq.get` opens the clone flow directly, and
`ghq.settings` opens the settings dashboard.

## Configuration

Settings live in a flat `config.toml` in the plugin's config dir
(`herdr plugin config-dir ghq`). Edit it directly, copy `examples/config.toml`, or run
the `ghq.settings` action. See `examples/config.toml` for every key; highlights:

- `default_target` — `workspace` (default) · `tab` · `split` · `pane`
- `include_agents` / `include_workspaces` — blend agents/workspaces into the list (needs `jq`)
- `label` — workspace/tab label: `repo` · `owner-repo` · `path`
- `preview` / `preview_readme` — the preview pane
- `clone_source` — seed the clone prompt from the `clipboard` (default) or start blank
- `split_direction` / `split_ratio` — geometry for split targets

## How it works

Each action (`bin/action.sh`) captures the origin pane id and cwd, then opens an overlay
pane (`picker`, `get`, or `settings`). The picker builds one tab-delimited list from
`herdr agent list`, `herdr workspace list`, and `ghq list` (only the pretty column is
shown and searched; the kind/id/dir travel in hidden fields), and maps the accepted key
to a herdr CLI verb — `agent focus` / `workspace focus` / `workspace create` /
`tab create` / `pane split` / `pane send-text` — always targeting the captured origin
pane or a real id from herdr, never a guessed one. Removal (`ctrl-x`) is the only
destructive path and always requires typing the repo name to confirm.

## License

MIT — see `LICENSE`.
