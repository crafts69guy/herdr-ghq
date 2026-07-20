//! Data layer: theme, plugin config, and the unified entry list (agents,
//! workspaces, ghq repos).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use ratatui::style::Color;

use crate::runner::CommandRunner;

// --- theme -----------------------------------------------------------------

/// Colours pulled from herdr's `[theme.custom]` so the TUI matches the terminal.
/// The default is empty — every slot then falls back to its ratatui colour,
/// which is also what a user with no `[theme.custom]` section gets.
#[derive(Clone, Default)]
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
/// The default is empty, which is what an unconfigured plugin has: every
/// `get`/`bool` then answers with the caller's own default.
#[derive(Clone, Default)]
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

#[derive(Clone, Copy, PartialEq, Debug)]
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
#[derive(Clone, Copy, PartialEq, Debug)]
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

pub fn ghq_root(runner: &dyn CommandRunner) -> String {
    runner.capture("ghq", &["root"]).unwrap_or_default()
}

/// Status → colour, shared with the preview card so an agent's pill there is the
/// same colour as its bullet in the list.
pub fn state_color(theme: &Theme, status: &str) -> Color {
    match status {
        "idle" | "ready" | "done" => theme.or("green", Color::Green),
        "working" => theme.or("yellow", Color::Yellow),
        "blocked" => theme.or("red", Color::Red),
        _ => theme.or("subtext0", Color::DarkGray),
    }
}

pub fn load(runner: &dyn CommandRunner, cfg: &Config, theme: &Theme, root: &str) -> Vec<Entry> {
    let mut entries = Vec::new();

    if cfg.bool("include_agents", true) {
        if let Some(json) = runner.capture("herdr", &["agent", "list"]) {
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
        if let Some(json) = runner.capture("herdr", &["workspace", "list"]) {
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

    if let Some(list) = runner.capture("ghq", &["list"]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::MockRunner;

    const AGENTS: &str = r#"{"result":{"agents":[
        {"terminal_id":"term-1","agent":"claude","agent_status":"working","foreground_cwd":"/home/u/proj"},
        {"terminal_id":"","agent":"ghost","agent_status":"idle"},
        {"terminal_id":"term-2","agent":"","agent_status":"idle"}
    ]}}"#;
    const WORKSPACES: &str = r#"{"result":{"workspaces":[
        {"workspace_id":"ws-1","label":"work","number":2,"pane_count":3,"focused":true}
    ]}}"#;
    const REPOS: &str = "github.com/o/repo-a\nbitbucket.org/o/repo-b\n";

    fn seeded() -> MockRunner {
        MockRunner::new()
            .on("herdr agent list", AGENTS)
            .on("herdr workspace list", WORKSPACES)
            .on("ghq list", REPOS)
    }

    #[test]
    fn load_maps_each_source_into_entries_in_order() {
        let entries = load(&seeded(), &Config::default(), &Theme::default(), "/root");
        // One valid agent (the id-less and label-less ones are dropped), one
        // workspace, then the two repos — agents, workspaces, repos in that order.
        assert_eq!(entries.len(), 4);

        assert_eq!(entries[0].kind, Kind::Agent);
        assert_eq!(entries[0].id, "term-1");
        assert_eq!(entries[0].dir.as_deref(), Some("/home/u/proj"));
        assert_eq!(entries[0].primary, "proj · claude");
        assert_eq!(entries[0].secondary, "working");

        assert_eq!(entries[1].kind, Kind::Workspace);
        assert_eq!(entries[1].id, "ws-1");
        assert!(
            entries[1].secondary.contains("current"),
            "{:?}",
            entries[1].secondary
        );

        assert_eq!(entries[2].kind, Kind::Repo);
        assert_eq!(entries[2].id, "github.com/o/repo-a");
        assert_eq!(entries[2].dir.as_deref(), Some("/root/github.com/o/repo-a"));
        assert_eq!(entries[2].primary, "o/repo-a");
        assert_eq!(entries[2].label, "repo-a");
    }

    #[test]
    fn load_skips_a_source_when_its_include_flag_is_false() {
        let mut map = HashMap::new();
        map.insert("include_agents".to_string(), "false".to_string());
        map.insert("include_workspaces".to_string(), "false".to_string());
        let cfg = Config { map };
        let runner = seeded();

        let entries = load(&runner, &cfg, &Theme::default(), "/root");
        // Only the two repos survive.
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.kind == Kind::Repo));
        // And the disabled sources were never even asked for.
        let asked_agents = runner
            .calls()
            .iter()
            .any(|c| c.contains(&"agent".to_string()) && c.contains(&"list".to_string()));
        assert!(
            !asked_agents,
            "agent list should not run when include_agents=false"
        );
    }

    #[test]
    fn load_survives_a_source_that_returns_nothing() {
        // No seeds: every command returns empty stdout, which is unparseable JSON
        // for the herdr sources and an empty repo list — the switcher must simply
        // come up empty rather than panic.
        let entries = load(
            &MockRunner::new(),
            &Config::default(),
            &Theme::default(),
            "/root",
        );
        assert!(entries.is_empty());
    }
}
