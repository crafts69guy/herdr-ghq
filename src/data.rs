//! Data layer: theme, plugin config, and the unified entry list (agents,
//! workspaces, ghq repos).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ratatui::style::Color;

// --- theme -----------------------------------------------------------------

/// Colours pulled from herdr's `[theme.custom]` so the TUI matches the terminal.
#[derive(Clone)]
pub struct Theme {
    slots: HashMap<String, Color>,
}

impl Theme {
    pub fn load() -> Self {
        let path = env::var("HERDR_CONFIG_PATH").unwrap_or_else(|_| {
            format!(
                "{}/.config/herdr/config.toml",
                env::var("HOME").unwrap_or_default()
            )
        });
        let mut slots = HashMap::new();
        if let Ok(text) = fs::read_to_string(path) {
            let mut in_section = false;
            for line in text.lines() {
                let t = line.trim();
                if t.starts_with('[') {
                    in_section = t == "[theme.custom]";
                    continue;
                }
                if !in_section {
                    continue;
                }
                if let Some((k, v)) = t.split_once('=') {
                    if let Some(color) = parse_hex(v.trim()) {
                        slots.insert(k.trim().to_string(), color);
                    }
                }
            }
        }
        Theme { slots }
    }

    pub fn get(&self, key: &str) -> Option<Color> {
        self.slots.get(key).copied()
    }

    pub fn or(&self, key: &str, fallback: Color) -> Color {
        self.get(key).unwrap_or(fallback)
    }

    /// Resolve a colour spec that is either a `[theme.custom]` slot name
    /// (e.g. `peach`) or a literal `#rrggbb`.
    pub fn resolve(&self, spec: &str) -> Option<Color> {
        if spec.starts_with('#') {
            parse_hex(spec)
        } else {
            self.get(spec)
        }
    }
}

fn parse_hex(raw: &str) -> Option<Color> {
    let s = raw.trim().trim_matches('"');
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

// --- plugin config ---------------------------------------------------------

/// Flat `key = value` config from the plugin's config dir.
#[derive(Clone)]
pub struct Config {
    map: HashMap<String, String>,
}

impl Config {
    pub fn load() -> Self {
        let dir = env::var("HERDR_PLUGIN_CONFIG_DIR").unwrap_or_default();
        let mut map = HashMap::new();
        if !dir.is_empty() {
            if let Ok(text) = fs::read_to_string(PathBuf::from(dir).join("config.toml")) {
                for line in text.lines() {
                    let t = line.trim();
                    if t.is_empty() || t.starts_with('#') || t.starts_with('[') {
                        continue;
                    }
                    if let Some((k, v)) = t.split_once('=') {
                        let v = v.trim();
                        let v = v.split_once('#').map(|(a, _)| a.trim()).unwrap_or(v);
                        map.insert(k.trim().to_string(), v.trim_matches('"').to_string());
                    }
                }
            }
        }
        Config { map }
    }

    pub fn get(&self, key: &str, default: &str) -> String {
        match self.map.get(key) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => default.to_string(),
        }
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.get(key, if default { "true" } else { "false" }) == "true"
    }
}

// --- entries ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Agent,
    Workspace,
    Repo,
}

impl Kind {
    /// Stable ordering used by the "Kind" sort (agents first, repos last).
    pub fn order(self) -> u8 {
        match self {
            Kind::Agent => 0,
            Kind::Workspace => 1,
            Kind::Repo => 2,
        }
    }
}

/// How the no-query browse list is ordered. Fuzzy score always wins while the
/// user is typing; this only decides the resting order.
#[derive(Clone, Copy, PartialEq)]
pub enum SortMode {
    /// Latest opened first (default), from the recency history file.
    Recent,
    /// Alphabetical by the primary column.
    Name,
    /// Grouped by kind: agents, then workspaces, then repos.
    Kind,
}

impl SortMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "name" => SortMode::Name,
            "kind" => SortMode::Kind,
            _ => SortMode::Recent,
        }
    }

    pub fn next(self) -> Self {
        match self {
            SortMode::Recent => SortMode::Name,
            SortMode::Name => SortMode::Kind,
            SortMode::Kind => SortMode::Recent,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::Recent => "recent",
            SortMode::Name => "name",
            SortMode::Kind => "kind",
        }
    }
}

/// Which group the list is narrowed to. `All` blends every source.
#[derive(Clone, Copy, PartialEq)]
pub enum GroupFilter {
    All,
    Only(Kind),
}

impl GroupFilter {
    /// Does `kind` pass this filter?
    pub fn matches(self, kind: Kind) -> bool {
        match self {
            GroupFilter::All => true,
            GroupFilter::Only(k) => k == kind,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GroupFilter::All => "All",
            GroupFilter::Only(Kind::Agent) => "Agents",
            GroupFilter::Only(Kind::Workspace) => "Workspaces",
            GroupFilter::Only(Kind::Repo) => "Repos",
        }
    }
}

#[derive(Clone)]
pub struct Entry {
    pub kind: Kind,
    /// Target id: terminal id (agent), workspace id, or ghq relative path (repo).
    pub id: String,
    /// Absolute directory when one applies (repo path, agent cwd).
    pub dir: Option<String>,
    /// Human label used for workspace/tab names and confirmations.
    pub label: String,
    // Display columns.
    pub icon: String,
    pub icon_color: Color,
    pub primary: String,
    pub secondary: String,
    /// Plain text used for fuzzy matching.
    pub search: String,
}

pub fn ghq_root() -> String {
    run(&["ghq", "root"]).unwrap_or_default().trim().to_string()
}

/// Run a command and capture trimmed stdout, or None on failure.
fn run(args: &[&str]) -> Option<String> {
    let out = Command::new(args[0]).args(&args[1..]).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

fn state_color(theme: &Theme, status: &str) -> Color {
    match status {
        "idle" | "ready" | "done" => theme.or("green", Color::Green),
        "working" => theme.or("yellow", Color::Yellow),
        "blocked" => theme.or("red", Color::Red),
        _ => theme.or("subtext0", Color::DarkGray),
    }
}

pub fn load(cfg: &Config, theme: &Theme, root: &str) -> Vec<Entry> {
    let mut entries = Vec::new();

    if cfg.bool("include_agents", true) {
        if let Some(json) = run(&["herdr", "agent", "list"]) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(arr) = v["result"]["agents"].as_array() {
                    for a in arr {
                        let tid = a["terminal_id"].as_str().unwrap_or("").to_string();
                        if tid.is_empty() {
                            continue;
                        }
                        // herdr can report a pane with a terminal id but no agent label
                        // (a stale or half-detected entry). Those are not agents.
                        let Some(agent) = a["agent"].as_str().filter(|s| !s.is_empty()) else {
                            continue;
                        };
                        let status = a["agent_status"].as_str().unwrap_or("unknown");
                        let cwd = a["foreground_cwd"]
                            .as_str()
                            .or_else(|| a["cwd"].as_str())
                            .unwrap_or("")
                            .to_string();
                        let base = basename(&cwd);
                        entries.push(Entry {
                            kind: Kind::Agent,
                            id: tid,
                            dir: if cwd.is_empty() { None } else { Some(cwd) },
                            label: base.clone(),
                            icon: "●".into(),
                            icon_color: state_color(theme, status),
                            primary: format!("{base} · {agent}"),
                            secondary: status.to_string(),
                            search: format!("{base} {agent} {status}"),
                        });
                    }
                }
            }
        }
    }

    if cfg.bool("include_workspaces", true) {
        if let Some(json) = run(&["herdr", "workspace", "list"]) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(arr) = v["result"]["workspaces"].as_array() {
                    for w in arr {
                        let wid = w["workspace_id"].as_str().unwrap_or("").to_string();
                        if wid.is_empty() {
                            continue;
                        }
                        let label = w["label"].as_str().unwrap_or("workspace").to_string();
                        let num = w["number"].as_i64().unwrap_or(0);
                        let panes = w["pane_count"].as_i64().unwrap_or(0);
                        let focused = w["focused"].as_bool().unwrap_or(false);
                        let mut sec = format!("#{num} · {panes}p");
                        if focused {
                            sec.push_str(" · current");
                        }
                        entries.push(Entry {
                            kind: Kind::Workspace,
                            id: wid,
                            dir: None,
                            label: label.clone(),
                            icon: "".into(),
                            icon_color: theme.or("accent", Color::Cyan),
                            primary: label.clone(),
                            secondary: sec,
                            search: format!("{label} workspace"),
                        });
                    }
                }
            }
        }
    }

    if let Some(list) = run(&["ghq", "list"]) {
        for rel in list.lines().filter(|l| !l.is_empty()) {
            let (host, rest) = rel.split_once('/').unwrap_or(("", rel));
            let (icon, color) = host_icon(host, theme);
            let short = host.split('.').next().unwrap_or(host).to_string();
            entries.push(Entry {
                kind: Kind::Repo,
                id: rel.to_string(),
                dir: Some(format!("{root}/{rel}")),
                label: basename(rel),
                icon: icon.into(),
                icon_color: color,
                primary: rest.to_string(),
                secondary: short.clone(),
                search: format!("{rel} {short}"),
            });
        }
    }

    entries
}

fn host_icon(host: &str, theme: &Theme) -> (&'static str, Color) {
    match host {
        "github.com" => ("", theme.or("mauve", Color::Magenta)),
        "bitbucket.org" => ("", theme.or("blue", Color::Blue)),
        "gitlab.com" => ("", theme.or("peach", Color::Yellow)),
        _ => ("", theme.or("subtext0", Color::DarkGray)),
    }
}

fn basename(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_string()
}
