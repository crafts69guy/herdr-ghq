# Changelog

All notable changes to this plugin are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The settings dashboard now opens as a session-modal popup sized to its content
  (96×26) instead of a full-screen overlay, so it reads as the small form it is and
  leaves the workspace visible behind it. Needs herdr ≥ 0.7.4, which is already the
  declared minimum.

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
  entry — forcing a target only changes where a *repo* lands.
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

[Unreleased]: https://github.com/crafts69guy/herdr-ghq/compare/v0.5.0...HEAD
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
