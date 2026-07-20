//! The picker's keymap: a chord → action table, built from defaults and then
//! overridden by the flat config, plus an optional Vim-style modal layer.
//!
//! Why this exists: keys used to be hardcoded `match` arms, so nothing could be
//! remapped and a vimmer stuck with `^j`/`^k` even inside a picker whose whole
//! job is type-to-filter. Now every binding is a [`Chord`] → [`Action`] entry a
//! user can rebind with `keys.<action> = "..."` lines, and `keymode = modal`
//! adds a Normal mode where bare `hjkl`/`gg`/`G` navigate and the readline
//! chords (`^w`/`^u`) are free for editing.
//!
//! The default (`keymode = insert`) reproduces the old bindings exactly, so this
//! is not a breaking change — it is the same picker with a table behind it.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Accept as AcceptKind;
use crate::data::Config;

/// A key press reduced to what the keymap distinguishes: a base key plus the two
/// modifiers the picker uses. Shift is folded into the char case and [`Key::BackTab`].
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
}

/// The base keys a chord can carry.
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// Which keymap is live.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Mode {
    /// Type-to-filter: printable keys land in the query.
    Insert,
    /// Vim Normal: bare keys are commands, `i`/`/` return to Insert.
    Normal,
}

/// What a chord does. `Accept` carries the terminal action the picker returns to
/// the caller; the rest mutate picker state in place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Quit,
    Help,
    Changelog,
    NextGroup,
    PrevGroup,
    Down,
    Up,
    PageDown,
    PageUp,
    Top,
    Bottom,
    TogglePreview,
    PreviewDown,
    PreviewUp,
    CycleSort,
    Backspace,
    ClearQuery,
    DeleteWord,
    EnterInsert,
    EnterNormal,
    Accept(AcceptKind),
}

/// The action vocabulary, paired with the config name that rebinds it. One table
/// drives both the override lookup and the docs — a new action is one row here.
const NAMES: &[(&str, Action)] = &[
    ("quit", Action::Quit),
    ("help", Action::Help),
    ("changelog", Action::Changelog),
    ("next_group", Action::NextGroup),
    ("prev_group", Action::PrevGroup),
    ("down", Action::Down),
    ("up", Action::Up),
    ("page_down", Action::PageDown),
    ("page_up", Action::PageUp),
    ("top", Action::Top),
    ("bottom", Action::Bottom),
    ("toggle_preview", Action::TogglePreview),
    ("preview_down", Action::PreviewDown),
    ("preview_up", Action::PreviewUp),
    ("cycle_sort", Action::CycleSort),
    ("clear_query", Action::ClearQuery),
    ("delete_word", Action::DeleteWord),
    ("insert_mode", Action::EnterInsert),
    ("normal_mode", Action::EnterNormal),
    ("open", Action::Accept(AcceptKind::Default)),
    ("clone", Action::Accept(AcceptKind::Clone)),
    ("update_plugin", Action::Accept(AcceptKind::UpdatePlugin)),
    ("workspace", Action::Accept(AcceptKind::Workspace)),
    ("tab", Action::Accept(AcceptKind::Tab)),
    ("split", Action::Accept(AcceptKind::Split)),
    ("pane", Action::Accept(AcceptKind::Pane)),
    ("git", Action::Accept(AcceptKind::Git)),
    ("update", Action::Accept(AcceptKind::Update)),
    ("remove", Action::Accept(AcceptKind::Remove)),
];

/// The chord → action tables for both modes, plus whether Normal mode exists.
pub struct Keymap {
    insert: HashMap<Chord, Action>,
    normal: HashMap<Chord, Action>,
    /// `keymode = modal`: start in Normal, and Esc in Insert returns to Normal
    /// rather than quitting.
    pub modal: bool,
}

impl Keymap {
    /// Build the defaults, then apply `keys.*` overrides and `keymode`.
    pub fn load(cfg: &Config) -> Self {
        let modal = cfg.get("keymode", "insert") == "modal";
        let mut km = Keymap {
            insert: default_insert(),
            normal: default_normal(),
            modal,
        };
        km.apply_overrides(cfg);
        // In modal mode, Esc leaves Insert for Normal instead of quitting; Normal
        // keeps Esc = Quit (set in `default_normal`).
        if modal {
            km.insert.insert(chord(Key::Esc), Action::EnterNormal);
        }
        km
    }

    /// The action a chord triggers in `mode`, if any.
    pub fn action(&self, mode: Mode, ch: Chord) -> Option<Action> {
        match mode {
            Mode::Insert => self.insert.get(&ch),
            Mode::Normal => self.normal.get(&ch),
        }
        .copied()
    }

    /// The mode the picker starts in.
    pub fn start_mode(&self) -> Mode {
        if self.modal {
            Mode::Normal
        } else {
            Mode::Insert
        }
    }

    /// Rebind actions the config names. `keys.<action> = "chord[,chord…]"` clears
    /// that action's default chords in both maps and binds the listed ones. An
    /// unparseable chord is skipped, so one typo cannot silently unbind an action.
    fn apply_overrides(&mut self, cfg: &Config) {
        for (name, act) in NAMES {
            let spec = cfg.get(&format!("keys.{name}"), "");
            if spec.is_empty() {
                continue;
            }
            let chords: Vec<Chord> = spec.split(',').filter_map(parse_chord).collect();
            if chords.is_empty() {
                continue;
            }
            self.insert.retain(|_, a| a != act);
            self.normal.retain(|_, a| a != act);
            for ch in chords {
                self.insert.insert(ch, *act);
                self.normal.insert(ch, *act);
            }
        }
    }
}

/// A modifier-free chord for `key`.
fn chord(key: Key) -> Chord {
    Chord {
        key,
        ctrl: false,
        alt: false,
    }
}

fn ctrl(key: Key) -> Chord {
    Chord {
        key,
        ctrl: true,
        alt: false,
    }
}

fn alt(key: Key) -> Chord {
    Chord {
        key,
        ctrl: false,
        alt: true,
    }
}

/// The default Insert map — the picker's historical bindings, verbatim.
fn default_insert() -> HashMap<Chord, Action> {
    use Action::*;
    let mut m = HashMap::new();
    m.insert(chord(Key::Esc), Quit);
    m.insert(ctrl(Key::Char('c')), Quit);
    m.insert(chord(Key::Char('?')), Help);
    m.insert(chord(Key::Tab), NextGroup);
    m.insert(chord(Key::BackTab), PrevGroup);
    m.insert(alt(Key::Char('p')), TogglePreview);
    m.insert(alt(Key::Char('j')), PreviewDown);
    m.insert(alt(Key::Char('k')), PreviewUp);
    m.insert(alt(Key::Char('s')), CycleSort);
    m.insert(alt(Key::Char('c')), Changelog);
    m.insert(alt(Key::Char('u')), Accept(AcceptKind::UpdatePlugin));
    m.insert(alt(Key::Enter), Accept(AcceptKind::Clone));
    m.insert(chord(Key::Enter), Accept(AcceptKind::Default));
    m.insert(chord(Key::Up), Up);
    m.insert(chord(Key::Down), Down);
    m.insert(chord(Key::PageUp), PageUp);
    m.insert(chord(Key::PageDown), PageDown);
    m.insert(chord(Key::Backspace), Backspace);
    m.insert(ctrl(Key::Char('j')), Down);
    m.insert(ctrl(Key::Char('n')), Down);
    m.insert(ctrl(Key::Char('k')), Up);
    m.insert(ctrl(Key::Char('p')), Up);
    m.insert(ctrl(Key::Char('w')), Accept(AcceptKind::Workspace));
    m.insert(ctrl(Key::Char('t')), Accept(AcceptKind::Tab));
    m.insert(ctrl(Key::Char('s')), Accept(AcceptKind::Split));
    m.insert(ctrl(Key::Char('o')), Accept(AcceptKind::Pane));
    m.insert(ctrl(Key::Char('g')), Accept(AcceptKind::Git));
    m.insert(ctrl(Key::Char('u')), Accept(AcceptKind::Update));
    m.insert(ctrl(Key::Char('x')), Accept(AcceptKind::Remove));
    m
}

/// The default Normal map (only reached with `keymode = modal`): bare-key Vim
/// navigation, `i`/`/` to filter, and the same accept + manage verbs on unshifted
/// keys — so a vimmer runs the picker without holding a modifier.
fn default_normal() -> HashMap<Chord, Action> {
    use Action::*;
    let mut m = HashMap::new();
    m.insert(chord(Key::Esc), Quit);
    m.insert(chord(Key::Char('q')), Quit);
    m.insert(ctrl(Key::Char('c')), Quit);
    m.insert(chord(Key::Char('?')), Help);
    // Motion.
    m.insert(chord(Key::Char('j')), Down);
    m.insert(chord(Key::Char('k')), Up);
    m.insert(chord(Key::Down), Down);
    m.insert(chord(Key::Up), Up);
    m.insert(chord(Key::Char('g')), Top);
    m.insert(chord(Key::Char('G')), Bottom);
    m.insert(chord(Key::PageDown), PageDown);
    m.insert(chord(Key::PageUp), PageUp);
    m.insert(chord(Key::Char('d')), PageDown);
    m.insert(chord(Key::Char('u')), PageUp);
    // Enter Insert (filter).
    m.insert(chord(Key::Char('i')), EnterInsert);
    m.insert(chord(Key::Char('/')), EnterInsert);
    // Groups + view.
    m.insert(chord(Key::Tab), NextGroup);
    m.insert(chord(Key::BackTab), PrevGroup);
    m.insert(chord(Key::Char('S')), CycleSort);
    m.insert(chord(Key::Char('p')), TogglePreview);
    m.insert(alt(Key::Char('j')), PreviewDown);
    m.insert(alt(Key::Char('k')), PreviewUp);
    m.insert(chord(Key::Char('c')), Changelog);
    // Accept + manage, unshifted.
    m.insert(chord(Key::Enter), Accept(AcceptKind::Default));
    m.insert(chord(Key::Char('w')), Accept(AcceptKind::Workspace));
    m.insert(chord(Key::Char('t')), Accept(AcceptKind::Tab));
    m.insert(chord(Key::Char('s')), Accept(AcceptKind::Split));
    m.insert(chord(Key::Char('o')), Accept(AcceptKind::Pane));
    m.insert(chord(Key::Char('h')), Accept(AcceptKind::Git));
    m.insert(chord(Key::Char('x')), Accept(AcceptKind::Remove));
    m.insert(chord(Key::Char('U')), Accept(AcceptKind::Update));
    m
}

/// Reduce a crossterm key event to a [`Chord`], or `None` for keys the picker
/// does not model (function keys, etc.). Shift is already baked into the char,
/// so it is not tracked separately except as [`Key::BackTab`].
pub fn chord_of(k: &KeyEvent) -> Option<Chord> {
    let key = match k.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        _ => return None,
    };
    Some(Chord {
        key,
        ctrl: k.modifiers.contains(KeyModifiers::CONTROL),
        alt: k.modifiers.contains(KeyModifiers::ALT),
    })
}

/// Parse a config chord spec like `ctrl-j`, `alt-p`, `shift-tab`, `enter`, `g`.
/// Modifiers precede the key, `-`-separated; the last segment is the key.
pub fn parse_chord(spec: &str) -> Option<Chord> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let parts: Vec<&str> = spec.split('-').collect();
    let (mods, key_part) = parts.split_at(parts.len() - 1);
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "c" | "^" => ctrl = true,
            "alt" | "opt" | "meta" | "a" | "m" => alt = true,
            "shift" => shift = true,
            _ => return None,
        }
    }
    let key = parse_key(key_part[0], shift)?;
    Some(Chord { key, ctrl, alt })
}

fn parse_key(name: &str, shift: bool) -> Option<Key> {
    let key = match name.to_ascii_lowercase().as_str() {
        "enter" | "return" | "cr" => Key::Enter,
        "esc" | "escape" => Key::Esc,
        "tab" if shift => Key::BackTab,
        "tab" => Key::Tab,
        "backtab" => Key::BackTab,
        "backspace" | "bs" => Key::Backspace,
        "up" => Key::Up,
        "down" => Key::Down,
        "pgup" | "pageup" => Key::PageUp,
        "pgdn" | "pagedown" => Key::PageDown,
        "home" => Key::Home,
        "end" => Key::End,
        "space" => Key::Char(' '),
        s if s.chars().count() == 1 => {
            let c = name.chars().next().unwrap();
            // `shift-a` means the uppercase char; a bare uppercase works too.
            return Some(Key::Char(if shift { c.to_ascii_uppercase() } else { c }));
        }
        _ => return None,
    };
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chord_reads_modifiers_and_named_keys() {
        assert_eq!(
            parse_chord("ctrl-j"),
            Some(Chord {
                key: Key::Char('j'),
                ctrl: true,
                alt: false
            })
        );
        assert_eq!(
            parse_chord("alt-p"),
            Some(Chord {
                key: Key::Char('p'),
                ctrl: false,
                alt: true
            })
        );
        assert_eq!(parse_chord("shift-tab"), Some(chord(Key::BackTab)));
        assert_eq!(parse_chord("enter"), Some(chord(Key::Enter)));
        assert_eq!(parse_chord("g"), Some(chord(Key::Char('g'))));
        assert_eq!(parse_chord("space"), Some(chord(Key::Char(' '))));
        assert!(parse_chord("bogusmod-j").is_none());
        assert!(parse_chord("").is_none());
    }

    #[test]
    fn defaults_reproduce_the_historical_insert_bindings() {
        let km = Keymap::load(&Config::default());
        assert_eq!(
            km.action(Mode::Insert, chord(Key::Enter)),
            Some(Action::Accept(AcceptKind::Default))
        );
        assert_eq!(
            km.action(Mode::Insert, ctrl(Key::Char('t'))),
            Some(Action::Accept(AcceptKind::Tab))
        );
        assert_eq!(
            km.action(Mode::Insert, ctrl(Key::Char('j'))),
            Some(Action::Down)
        );
        assert_eq!(
            km.action(Mode::Insert, alt(Key::Char('p'))),
            Some(Action::TogglePreview)
        );
        assert_eq!(km.action(Mode::Insert, chord(Key::Esc)), Some(Action::Quit));
        // A plain letter is not bound — it falls through to the query.
        assert_eq!(km.action(Mode::Insert, chord(Key::Char('j'))), None);
        // Default keymode is insert, so the picker starts typing.
        assert_eq!(km.start_mode(), Mode::Insert);
    }

    #[test]
    fn modal_mode_starts_in_normal_and_reroutes_esc() {
        let cfg = Config::from_pairs(&[("keymode", "modal")]);
        let km = Keymap::load(&cfg);
        assert_eq!(km.start_mode(), Mode::Normal);
        // Bare hjkl navigate in Normal.
        assert_eq!(
            km.action(Mode::Normal, chord(Key::Char('j'))),
            Some(Action::Down)
        );
        assert_eq!(
            km.action(Mode::Normal, chord(Key::Char('k'))),
            Some(Action::Up)
        );
        assert_eq!(
            km.action(Mode::Normal, chord(Key::Char('i'))),
            Some(Action::EnterInsert)
        );
        // Esc in Insert returns to Normal rather than quitting.
        assert_eq!(
            km.action(Mode::Insert, chord(Key::Esc)),
            Some(Action::EnterNormal)
        );
        // Esc in Normal still quits.
        assert_eq!(km.action(Mode::Normal, chord(Key::Esc)), Some(Action::Quit));
    }

    #[test]
    fn a_keys_override_rebinds_an_action_in_both_maps() {
        let cfg = Config::from_pairs(&[("keys.remove", "ctrl-d")]);
        let km = Keymap::load(&cfg);
        // The new chord triggers Remove…
        assert_eq!(
            km.action(Mode::Insert, ctrl(Key::Char('d'))),
            Some(Action::Accept(AcceptKind::Remove))
        );
        // …and the old default chord for Remove is gone.
        assert_eq!(km.action(Mode::Insert, ctrl(Key::Char('x'))), None);
    }

    #[test]
    fn an_override_can_bind_readline_editing_over_the_action_chords() {
        // A vimmer reclaims ^u/^w for editing without switching to modal.
        let cfg = Config::from_pairs(&[
            ("keys.clear_query", "ctrl-u"),
            ("keys.delete_word", "ctrl-w"),
        ]);
        let km = Keymap::load(&cfg);
        assert_eq!(
            km.action(Mode::Insert, ctrl(Key::Char('u'))),
            Some(Action::ClearQuery)
        );
        assert_eq!(
            km.action(Mode::Insert, ctrl(Key::Char('w'))),
            Some(Action::DeleteWord)
        );
    }
}
