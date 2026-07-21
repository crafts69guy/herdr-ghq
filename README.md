# herdr-ghq

![herdr 0.7.4+](https://img.shields.io/badge/herdr-0.7.4%2B-lightgrey)
![ghq required](https://img.shields.io/badge/ghq-required-green)
![platform macOS | Linux](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
![license MIT](https://img.shields.io/badge/license-MIT-green)

A [herdr](https://herdr.dev) plugin that puts your running **agents**, open **workspaces**,
every [`ghq`](https://github.com/x-motemen/ghq) **repository**, and its linked Git
**worktrees** in one fuzzy switcher — and makes `enter` do the right thing for whatever
you land on.

Where `ghq list | fzf | cd` can only change a directory, this uses herdr as a multiplexer:
jump to a live agent, switch workspaces, or open a repo/worktree exactly where you want it —
a new workspace, tab, split, or the current pane. It is a Rust TUI (ratatui + nucleo); no
fzf required.

![The herdr-ghq switcher: a fuzzy list with live preview and the `?` keybindings popup open](docs/switcher.png)

## Requirements

|                                                                          |                                                     |
| ------------------------------------------------------------------------ | --------------------------------------------------- |
| **herdr** ≥ 0.7.4                                                        | the host multiplexer                                |
| **[`ghq`](https://github.com/x-motemen/ghq)**                            | repository source                                   |
| **[Rust / `cargo`](https://rustup.rs)**                                  | the TUI builds on demand the first time you open it |
| _optional_ **[`hunk`](https://github.com/modem-dev/hunk)**               | the git menu's review pane (`brew install hunk`)    |
| _optional_ **[`lazygit`](https://github.com/jesseduffield/lazygit)**     | staging/commit from the git menu                    |
| _optional_ **[`eza`](https://github.com/eza-community/eza)**             | richer preview tree                                 |

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

The picker works like a Telescope/LazyVim picker: it opens **typing** (Insert mode), and
`esc` drops to **Normal** mode for Vim motions. A `NORMAL` / `INSERT` tag on the search box
says which mode owns the keys, `?` shows the live cheatsheet for that mode, and the
command bar re-labels itself per mode. Press `i` or `/` in Normal to type again.

**Accept** (`enter`) is kind-aware in either mode:

| Highlighted   | `enter`                                                                   |
| ------------- | ------------------------------------------------------------------------- |
| **agent**     | jump to it (`herdr agent focus`)                                          |
| **workspace** | switch to it (`herdr workspace focus`)                                    |
| **repo**      | open it in `default_target` — a new workspace unless configured otherwise |
| **worktree**  | open its linked checkout in `default_target`                              |

**Insert mode** (type to filter; `esc` → Normal, `^c` closes):

| Key                | Does                                                                     |
| ------------------ | ------------------------------------------------------------------------ |
| `↵` · `⌥↵`         | open · switch to the **clone** flow                                      |
| `^j`/`^n` · `^k`/`^p` | down · up                                                             |
| `^t` · `^v` · `^o` | open in a new **tab** · **split** · the **current pane** (`cd`)          |
| `⌥w` · `^g` · `^r` · `^x` | to a **workspace** · the **git menu** · `ghq get -u` · **remove** |
| `tab` / `⇧tab`     | cycle groups (All → Agents → Workspaces → Repos → Worktrees)              |
| `⌥p` · `⌥s` · `⌥j`/`⌥k` | toggle preview · cycle sort · scroll the preview                    |
| `^u` · `^w` · `⌫`  | clear the query · delete a word · delete a char (readline)               |
| `⌥,` · `⌥c` · `⌥u` · `?` | settings · changelog · update the plugin itself · this cheatsheet  |

**Normal mode** (`esc` from Insert; `i` or `/` returns): bare `h`/`j`/`k`/`l` motion is Vim's —
`j`/`k` move, `g`/`G` top/bottom, `^d`/`^u` page, `H`/`L` prev/next group. Frequent opens sit
on unshifted keys — `t` tab, `v` split, `o` cd, `w` workspace, `p` toggle preview — and the
**`␣` leader** groups the rest: `␣g` git, `␣u` update repo, `␣x` remove, `␣c` clone, `␣s` sort,
`␣l` changelog, `␣,` settings. `q` or `esc` closes. (`?` always shows the exact, current bindings.)

**Anywhere:** the **wheel** scrolls the pane under the pointer (card over the preview, list
elsewhere); a **click** selects an entry, filters on a group tab, or runs a command-bar pill.

Sorting defaults to `recent`, so repos you opened last float to the top; opens are recorded
in `${XDG_STATE_HOME:-~/.local/state}/herdr-ghq/recent.tsv`. While you type, fuzzy score
orders the list — sort only applies to the resting, no-query list.

**Remapping.** Every binding is a `chord → action` entry you can change in `config.toml`:

```toml
keys.tab = "ctrl-y"              # cycle groups on ^y instead of Tab
keys.split = "ctrl-x"            # split on ^x instead of ^v
keys.down = "ctrl-j,ctrl-n"     # one action, several chords
```

A chord is a key with optional `ctrl-` / `alt-` / `shift-` prefixes. The full list of action
names is in [`examples/config.toml`](examples/config.toml), and the footer + `?` cheatsheet
re-render from your bindings, so they always show what you actually set.

**Start mode.** `keymode = "normal"` opens the picker in Normal mode (Vim-first) instead of
Insert. Normal mode is always one `esc` away either way.

## Actions

Bind any of these the same way as `ghq.menu`:

| Action                                                   | Does                                                           |
| -------------------------------------------------------- | -------------------------------------------------------------- |
| `ghq.menu`                                               | the switcher                                                   |
| `ghq.git`                                                | the git menu for the current repo (bind to `prefix+g`)         |
| `ghq.get`                                                | the clone flow                                                 |
| `ghq.changelog`                                          | what changed, with your installed version marked               |
| `ghq.update-plugin`                                      | install a newer version (refuses to touch a `link`ed checkout) |
| `ghq.open-workspace` · `ghq.open-tab` · `ghq.open-split` | the switcher with `enter`'s repo target forced                 |

## Git menu

`^g` (Insert) or `␣g` (Normal) opens a git menu **overlay** over the switcher — the floating-card
shape of `⌥c`/`⌥,` — acting on the highlighted repo or linked worktree (or, via the `ghq.git`
action on `prefix+g`, the pane you launched from). Walk it with `↑`/`↓`, `enter` runs the row,
a mnemonic letter runs it directly, `esc` closes. Worktrees deliberately omit the repo-only
update and remove actions.

| Row                   | Runs                                                                 |
| --------------------- | ------------------------------------------------------------------- |
| review **worktree**   | `hunk diff`                                                          |
| review **staged**     | `hunk diff --staged`                                                 |
| review **branch**     | `hunk diff <base>` — base auto-detected, or pinned via `base_branch` |
| review **history**    | a `git log` list → `hunk show <commit>`                             |
| **resolve conflicts** | review the unmerged diff in `hunk`, then open `$EDITOR` on the files |
| **lazygit**           | stage / commit / push (shown only when `lazygit` is installed)       |

Reviews open in [`hunk`](https://github.com/modem-dev/hunk) (`brew install hunk`), themed from your
herdr `[theme.custom]`. Add your own rows in `menu.conf` (`key|icon|label|shell command`) beside
`config.toml`.

## Configuration

Settings live in a flat `config.toml` in the plugin's config dir (`herdr plugin config-dir ghq`).
Edit it directly, copy [`examples/config.toml`](examples/config.toml), or press `⌥,` in the
switcher — a floating form you walk with `↑`/`↓`, where `enter` cycles each value. Edits are
drafts: a `●` marks each changed row, `a` applies them all to `config.toml`, and `esc` discards
them. Applying takes effect in the running switcher — the list re-sorts, sources and preview
reload, colours and key rebinds update on the spot; no relaunch or server reload needed.

Every key is documented in `examples/config.toml`. The ones you're most likely to want:

| Key                                     | Values                                                                      |
| --------------------------------------- | --------------------------------------------------------------------------- |
| `default_target`                        | `workspace` (default) · `tab` · `split` · `pane`                            |
| `default_tab`                           | `all` (default) · `agents` · `workspaces` · `repos` · `worktrees`            |
| `include_agents` / `include_workspaces` | blend agents/workspaces into the list                                       |
| `include_worktrees`                     | list linked Git worktrees (`true` by default)                               |
| `sort`                                  | `recent` (default) · `name` · `kind`                                        |
| `keymode`                               | start mode: `insert` (default) · `normal` (Vim-first)                       |
| `keys.<action>`                         | rebind a key, e.g. `keys.tab = "ctrl-y"` (see below)                        |
| `label`                                 | workspace/tab label: `repo` · `owner-repo` · `path`                         |
| `preview` / `preview_readme`            | the preview pane                                                            |
| `clone_source`                          | seed the clone prompt from the `clipboard` (default) or start blank         |
| `base_branch`                           | base for the git menu's branch review (blank = auto-detect)                 |
| `split_direction` / `split_ratio`       | geometry for split targets                                                  |
| `update_check`                          | ask GitHub once a day whether a newer version is tagged (`true` by default) |
| `notifications` / `notification_position` | herdr toasts, and which corner they land in                               |
| `notification_sound`                    | `auto` (per-event, default) · `none` · `done` · `request`                   |

The switcher is themed from herdr's `[theme.custom]`, and previews each kind as a card —
a header with the entry's state as a pill, aligned `label value` rows, then bodies under
captioned rules. Repos and worktrees show branch · clean/dirty · last commit, a file tree,
and a README excerpt rendered as markdown; agents show what they are doing and their recent
output, in the agent's own colours; workspaces list their tabs, each with its live status.
Long cards scroll with `alt-j` / `alt-k`.

`update_check` only ever shows `↑ v0.6.0` in the command bar — it never installs anything.
Set it to `false` and the plugin makes no outbound requests at all.

## How it works

Each action starts in `bin/action.sh`, which captures the origin pane id and cwd before the
new pane steals focus, then opens that pane — an overlay for the picker and clone flow, a
popup for the changelog.

The picker itself is the Rust TUI in `src/`, built to `target/release/herdr-ghq-switcher` by
`bin/picker.sh` on first run. It reads `herdr agent list`, `herdr workspace list`, `ghq list`,
and Git's stable `worktree list --porcelain -z` output, fuzzy-filters with nucleo, and previews
the selection as a card drawn in your herdr theme colours — `bin/preview.sh` supplies only the
repo/worktree file tree. On
accept it maps the key to a herdr CLI verb — `agent focus`, `workspace focus`,
`workspace create`, `tab create`, `pane split`, `pane send-text` — always targeting the
captured origin pane or a real id from herdr, never a guessed one.

The changelog viewer is the same binary in `--changelog` mode; the settings form is a
floating overlay inside the switcher itself (`⌥,`), not a separate pane. Only the clone flow
is still bash (`bin/get.sh`).

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
