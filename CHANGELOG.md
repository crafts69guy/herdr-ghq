# Changelog

All notable changes to this plugin are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Keys are remappable, and there is an optional Vim mode.** Every picker binding is now
  a `chord → action` entry you can rebind in `config.toml` with `keys.<action> = "chord"`
  lines (for example `keys.tab = "ctrl-y"` or `keys.clear_query = "ctrl-u"`), so a vimmer
  can reclaim `^u`/`^w` for readline editing or move any verb to a key that suits them.
  Set `keymode = modal` for a Vim-style **Normal mode**: bare `hjkl`/`gg`/`G` navigate,
  `i` or `/` drop into the type-to-filter Insert mode, and the accept verbs sit on
  unshifted keys — no modifier held. A `NORMAL` / `INSERT` tag on the search box says
  which mode owns the keys. The default (`keymode = insert`) is exactly the old behaviour,
  so nothing changes unless you ask for it. See `examples/config.toml` for the full list
  of `keys.*` action names.

- Both plugin actions are now reachable from the switcher itself: `⌥c` reads the
  changelog and `⌥u` updates the plugin, alongside `^u` which updates the highlighted
  _repo_. Both are listed under `?`.

  `⌥c` draws over the list rather than replacing it, so reading what changed does not
  cost you your place — `esc` puts you back on the same entry. It shares the parser and
  renderer with the `ghq.changelog` pane, so the two cannot drift apart.

- **The switcher takes the mouse.** Click an entry to select it, a group tab to filter,
  and a pill on the command bar to run that command on the selection — the pills were
  always the list of what the keys do, so they are now the buttons for it too. A click
  dismisses a popup the way any key does. Nothing needs the mouse: every action still has
  its key, and the pills still say which.
- **The mouse wheel scrolls the pane under the pointer** — the card over the preview,
  the list anywhere else. The switcher asks for wheel and button reporting only, not the
  pointer motion crossterm's mouse capture would also turn on, so drags stay herdr's.
- **The preview scrolls**, with `⌥j` / `⌥k` — the `⌥` echo of the `^j` / `^k` that move
  the list, so the two panes move under the same fingers. The pane says `⌥jk 24/64` while
  there is anything below the fold, and stays quiet when the card fits. A card is 60-odd
  rows once an agent's output is in it, so most of it used to be simply unreachable: the
  scroll offset existed in the code but nothing was ever bound to it.

### Changed

- **An agent's output keeps the agent's colours.** herdr can hand back the escape
  sequences from the agent's own screen, so its diffs, syntax highlighting, and status
  line now read in the preview the way they read in the pane, instead of as flat text.
- **A README is rendered as markdown**, not dumped: headings in the title colour, bullets
  marked, inline `code` and `**bold**` styled, and links flattened to their text — a pane
  this narrow has no room for a URL, and the badges at the top of a README are mostly URL.
  It shares the renderer with the `⌥c` changelog popup.
- **The whole README is there**, where the card used to stop at 30 lines. That cut dates
  from a preview that could not scroll, when anything past the first screen was
  unreachable anyway; with `⌥j`/`⌥k` it only hid the text you had scrolled down to read.
  A 400-line bound remains for pathological files, and a card that hits it now says how
  many lines it left out rather than ending as if the README did.
- The preview is now a **card**: a header carrying the entry's name and its state as a
  filled pill, a column of aligned `label   value` rows, then each body under a
  captioned rule. It is drawn from your herdr `[theme.custom]` colours like the rest of
  the switcher, where it used to hardcode its own — a status pill here is now the same
  colour as that entry's bullet in the list, and the tab marker is the same `▌` the list
  marks its selection with.
- A **workspace preview lists its tabs** — each with its live status, pane count, and a
  marker on the active one. It only ever showed counts before.
- An **agent's recent output is clipped to the preview pane** instead of wrapped. The
  output arrives at the _agent's_ pane width, which is far wider than the preview, so
  wrapping shredded every line into fragments. Blank runs are collapsed too, so what you
  see is the output rather than the empty half of somebody's screen.
- **`jq` is no longer a requirement of any kind.** The preview was the last thing that
  called it; agents and workspaces are now read with `serde_json`, as the switcher's list
  already was. Nothing in the plugin shells out to `jq` any more, so it has been dropped
  from the requirements — including the claim that agents and workspaces needed it, which
  had not been true of the list itself for some time.

### Fixed

- **The agent preview showed raw JSON** instead of the agent. herdr nests the record
  under `result.agent`, and the preview read `result.agent_status` — which is not an
  error, just absent — so it printed the whole envelope as the agent's name and
  `unknown` as its status. The workspace preview had the same fault, and its tab list
  had been reading a field that does not exist. Agents and workspaces are now parsed in
  Rust rather than by jq filters.
- The repo preview no longer repeats the absolute path as the first line of its file
  tree; the card's own `path` row already carries it.

## [0.6.0] - 2026-07-16

### Added

- An **update action**: `ghq.update-plugin` installs the newest tagged version and
  rebuilds the switcher. It refuses to run against anything but an unambiguous GitHub
  install — a linked development checkout is left alone, with the manual steps printed
  instead, since installing over one would overwrite a working tree.
- An **update check**: once a day, the plugin asks GitHub whether a newer version is
  tagged and shows `↑ v0.6.0` at the end of the command bar. It never installs anything,
  and it yields to the keys rather than overdrawing them, so it goes unsaid on a narrow
  terminal. Turn it off with `update_check = "false"` for a plugin that makes no
  outbound requests at all.

  The switcher itself never touches the network: the check runs in a detached child
  process and leaves a cache the TUI reads. The picker often lives less than a second,
  and the fetch takes a few — a thread inside it would be killed before it finished.
  Offline, unreachable, or rate-limited, nothing is shown and the switcher opens as
  always.

- A **changelog viewer**: the `ghq.changelog` action opens this file as a popup, in the
  switcher's colours, with the version you are running marked `← installed`. It reads
  the `CHANGELOG.md` that ships beside the plugin, so it needs no network and always
  describes the code you actually have.

### Changed

- The settings dashboard is now part of the switcher's TUI instead of an fzf list, and
  opens as a session-modal popup sized to its content rather than a full-screen overlay.
  It reads as the form it is: no fuzzy prompt, no match counter, and no border label
  doubling herdr's own pane title. `↑`/`↓` walk it, `enter` cycles the value or
  edits `split_ratio` in place, `esc` closes. Needs herdr ≥ 0.7.4, already the declared
  minimum.

### Fixed

- Every setting is visible: the fzf dashboard cut off `notification_position` and
  truncated the `preview_position` hint. A window too short to fit the form now scrolls
  to keep the selection in view instead of silently hiding rows.
- Opening the switcher no longer fails on machines without `fzf`. Nothing in the plugin
  has used fzf since the settings dashboard moved into the TUI — the clone flow prompts
  with `read` — but `bin/action.sh` still refused to start the picker without it.

### Removed

- `fzf` is no longer a dependency.

## [0.5.0] - 2026-07-16

### Changed

- Previews now render on a worker thread instead of between a keypress and the next
  frame, so scrolling the list stays responsive on large repositories where
  `git status` dominates the ~100ms preview cost. The pane shows a `…` placeholder
  while a preview is in flight, and results the list has already scrolled past are
  dropped rather than drawn.

### Fixed

- The `open-workspace`, `open-tab`, and `open-split` actions behaved identically to
  plain `menu`: `bin/action.sh` exported `GHQ_FORCE_TARGET`, but nothing in the TUI
  read it, so Enter always fell back to `default_target`. A forced target now wins
  over `default_target`, and unrecognised values on either degrade to `workspace`
  rather than failing the open. Enter on an agent or workspace still focuses that
  entry — forcing a target only changes where a _repo_ lands.
- Panes that herdr reports with a terminal id but no agent label — stale or
  half-detected entries — no longer appear in the list as an agent named "agent".

## [0.4.0] - 2026-07-16

### Added

- `alt-p` toggles the preview pane at runtime.
- `tab` / `shift-tab` cycle the group filter (All → Agents → Workspaces → Repos),
  skipping empty groups; the active group is shown in the Switcher title.
- `alt-s` cycles the sort: `recent` (latest opened) → `name` → `kind`. Opens are
  remembered in `${XDG_STATE_HOME:-~/.local/state}/herdr-ghq/recent.tsv`.
- A `sort` key in the settings dashboard and the example config sets the startup
  default.

### Changed

- The default sort is now `recent`, so repositories you opened most recently float
  to the top of the resting list. While you are typing, fuzzy match score still
  orders the list.

## [0.3.4] - 2026-07-16

### Added

- A `?` keybindings cheatsheet popup (any key closes it).

### Changed

- List rows are now colourful: a kind icon, a bold primary name, and dim context.

## [0.3.3] - 2026-07-16

### Added

- A `title_color` config key (a `[theme.custom]` slot or a `#hex` value) colours the
  Search / Switcher / Preview box titles, defaulting to peach so they stand apart
  from the accent.

### Changed

- The documented keybinding for the switcher is now `prefix+space`.

## [0.3.2] - 2026-07-16

### Changed

- The command bar renders each key as a coloured background pill with dark ink, using
  full labels (open/tab/split/cd/workspace/git/update/remove/clone).
- The switcher:preview split now defaults to 4:6 (`preview_size = 60`).

## [0.3.1] - 2026-07-16

### Added

- `preview_position` (`right` | `down` | `up` | `left`) and `preview_size`.

### Changed

- The preview now defaults to the right (side-by-side). The command bar spans the
  full width regardless of preview position — something fzf could not do.

## [0.3.0] - 2026-07-16

### Changed

- The switcher is now a purpose-built Rust TUI (ratatui + nucleo) rather than an fzf
  wrapper, giving it full layout control: a Search box on top, the Switcher list, a
  Preview pane, and a full-width colourful command bar pinned to the bottom. The
  clone and settings flows stay on bash + fzf.
- `bin/picker.sh` is now a thin wrapper that builds the binary on first run and
  `exec`s it.

### Added

- **Requires [Rust / `cargo`](https://rustup.rs)** (`brew install rust`) to build the
  switcher on first launch.

## [0.2.3] - 2026-07-16

### Added

- `preview_position` (`down` | `right` | `up` | `left`) and `preview_size`.

### Changed

- The preview now defaults to the bottom, which is what makes an edge-to-edge command
  bar possible under fzf. Set `preview_position = "right"` to restore side-by-side.

## [0.2.2] - 2026-07-16

### Changed

- The command bar is compact (short labels, `·` separators) so every key including
  clone fits the list column without truncation, and the match counter sits at the
  right edge of the Search box.

## [0.2.1] - 2026-07-16

### Changed

- Adopted a component-box layout: a Search input box on top, a Switcher list below,
  and a Preview box on the right, dropping the outer wrapper border.
- Command hints moved into a full-width footer bar, each key in its own theme hue.
- The herdr overlay title is minimised to an icon.

## [0.2.0] - 2026-07-16

### Added

- **One list, three sources.** The picker is now a unified switcher blending running
  agents, open workspaces, and ghq repositories, with a kind-aware accept: `enter`
  jumps to an agent, switches to a workspace, or opens a repo in the default target.
- The open keys (`ctrl-w` / `ctrl-t` / `ctrl-s` / `ctrl-o`) act on a repo path or on
  an agent's cwd.
- A kind-aware preview: repos show a file tree, agents show recent output, workspaces
  show their tabs and panes.
- `include_agents` and `include_workspaces` config keys.

### Changed

- Rows carry a kind icon, a bold primary name, and dim context; repos drop the
  repeated `host/owner/` prefix and tag the host dimly.

### Notes

- Agents and workspaces require [`jq`](https://jqlang.github.io/jq/). Without it, the
  switcher degrades to repositories only.

## [0.1.0] - 2026-07-16

### Added

- Initial release: a one-key ghq repository switcher for herdr. Fuzzy-pick a repo and
  open it in a new workspace, tab, split, or the current pane, plus clone (`ghq get`),
  update, remove, and a handoff to the git-hub menu.

[Unreleased]: https://github.com/crafts69guy/herdr-ghq/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/crafts69guy/herdr-ghq/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/crafts69guy/herdr-ghq/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/crafts69guy/herdr-ghq/compare/v0.3.4...v0.4.0
[0.3.4]: https://github.com/crafts69guy/herdr-ghq/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/crafts69guy/herdr-ghq/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/crafts69guy/herdr-ghq/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/crafts69guy/herdr-ghq/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/crafts69guy/herdr-ghq/compare/v0.2.3...v0.3.0
[0.2.3]: https://github.com/crafts69guy/herdr-ghq/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/crafts69guy/herdr-ghq/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/crafts69guy/herdr-ghq/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/crafts69guy/herdr-ghq/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/crafts69guy/herdr-ghq/releases/tag/v0.1.0
