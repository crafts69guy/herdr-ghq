# herdr-ghq

A [herdr](https://herdr.dev) plugin that turns [`ghq`](https://github.com/x-motemen/ghq)
into a one-key project switcher. Fuzzy-pick any repository ghq manages and open it
**where you want it** — a new workspace, tab, split, or the current pane — plus clone,
update, remove, and a handoff to the [`git-hub`](https://github.com/crafts69guy/herdr-git-hub)
git menu.

Where a shell `ghq list | fzf | cd` can only change the current directory, this leans
into herdr as a multiplexer: the accept key decides the destination.

## Features

- **Themed fzf picker** over `ghq list`, colored from herdr's `[theme.custom]` so it
  matches your terminal theme, with a rich preview (branch · dirty/clean · last commit,
  a file tree, and a README excerpt).
- **Pick a repo, choose where it lands** — the popup's keys fan out to every target:

  | Key | Opens the repo in… |
  |-----|--------------------|
  | `enter` | the default target (a new **workspace**, unless overridden) |
  | `ctrl-w` | a new **workspace** |
  | `ctrl-t` | a new **tab** |
  | `ctrl-s` | a **split** of the current pane |
  | `ctrl-o` | the **current pane** (`cd`) |
  | `ctrl-g` | a new tab, then hands off to the **git-hub** menu |
  | `ctrl-u` | *(update)* runs `ghq get -u` on the highlighted repo |
  | `ctrl-x` | *(remove)* deletes it, behind a typed confirmation |
  | `alt-enter` | switches to the **clone** flow (`ghq get`) |

- **Clone flow** that seeds its prompt from a repo URL on your clipboard, then opens the
  fresh checkout with your default target.
- **Settings dashboard** — the `ghq.settings` action opens a themed fzf toggler over the
  flat config; changes apply on the next invocation, no server reload needed.

## Requirements

- herdr ≥ 0.7.4
- [`ghq`](https://github.com/x-motemen/ghq) and [`fzf`](https://github.com/junegunn/fzf)
- Optional: [`eza`](https://github.com/eza-community/eza) (richer preview tree),
  [`lazygit`](https://github.com/jesseduffield/lazygit) / the `git-hub` plugin (`ctrl-g`)

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
- `label` — workspace/tab label: `repo` · `owner-repo` · `path`
- `preview` / `preview_readme` — the repo preview pane
- `clone_source` — seed the clone prompt from the `clipboard` (default) or start blank
- `split_direction` / `split_ratio` — geometry for split targets

## How it works

Each action (`bin/action.sh`) captures the origin pane id and cwd, then opens an overlay
pane (`picker`, `get`, or `settings`). The picker runs fzf with `--expect` keys and maps
the accepted key to a herdr CLI verb — `workspace create` / `tab create` /
`pane split` / `pane send-text` — always targeting the captured origin pane, never a
guessed one. Removal (`ctrl-x`) is the only destructive path and always requires typing
the repo name to confirm.

## License

MIT — see `LICENSE`.
