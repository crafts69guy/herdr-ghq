//! Preview text for the highlighted entry. Reuses the bash `preview.sh` (git
//! tree, agent output, workspace summary) and converts its ANSI to ratatui.

use std::process::Command;

use ansi_to_tui::IntoText;
use ratatui::text::Text;

use crate::data::{Config, Entry, Kind};

pub fn render(entry: &Entry, script_dir: &str, root: &str, cfg: &Config) -> Text<'static> {
    let kind = match entry.kind {
        Kind::Agent => "agent",
        Kind::Workspace => "workspace",
        Kind::Repo => "repo",
    };
    let dir = entry.dir.clone().unwrap_or_default();
    let readme = if cfg.bool("preview_readme", true) {
        "true"
    } else {
        "false"
    };
    let out = Command::new("bash")
        .arg(format!("{script_dir}/preview.sh"))
        .arg(kind)
        .arg(&entry.id)
        .arg(&dir)
        .env("GHQ_ROOT", root)
        .env("GHQ_PREVIEW_README", readme)
        .output();
    match out {
        Ok(o) => o
            .stdout
            .into_text()
            .unwrap_or_else(|_| Text::raw(String::from_utf8_lossy(&o.stdout).into_owned())),
        Err(e) => Text::raw(format!("preview unavailable: {e}")),
    }
}
