//! Accept dispatch — runs AFTER the TUI is torn down, so interactive bits
//! (clone prompt, remove confirm, update output) use the normal pane.

use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{anyhow, Result};

use crate::data::{Config, Entry, Kind};

/// Which accept key was pressed.
#[derive(Clone, Copy, PartialEq)]
pub enum Accept {
    Default,
    Workspace,
    Tab,
    Split,
    Pane,
    Git,
    Update,
    Remove,
    Clone,
}

pub fn dispatch(
    entry: Option<Entry>,
    accept: Accept,
    origin_pane: &str,
    cfg: &Config,
    script_dir: &str,
) -> Result<()> {
    if accept == Accept::Clone {
        // Hand the whole terminal to the bash clone flow.
        let err = Command::new("bash").arg(format!("{script_dir}/get.sh")).exec();
        return Err(anyhow!("failed to exec get.sh: {err}"));
    }

    let e = entry.ok_or_else(|| anyhow!("no selection"))?;

    match e.kind {
        Kind::Workspace => focus_workspace(&e.id),
        Kind::Agent => {
            let target = open_kind(accept);
            match (target, e.dir.as_deref()) {
                (Some(t), Some(dir)) => open_repo(t, dir, origin_pane, &e.label, cfg),
                _ => focus_agent(&e.id),
            }
        }
        Kind::Repo => {
            let dir = e.dir.clone().unwrap_or_default();
            match accept {
                Accept::Default => {
                    let t = cfg.get("default_target", "workspace");
                    open_repo(&t, &dir, origin_pane, &e.label, cfg)
                }
                Accept::Workspace => open_repo("workspace", &dir, origin_pane, &e.label, cfg),
                Accept::Tab => open_repo("tab", &dir, origin_pane, &e.label, cfg),
                Accept::Split => open_repo("split", &dir, origin_pane, &e.label, cfg),
                Accept::Pane => open_repo("pane", &dir, origin_pane, &e.label, cfg),
                Accept::Git => {
                    open_repo("tab", &dir, origin_pane, &e.label, cfg)?;
                    git_handoff(&e.label)
                }
                Accept::Update => update(&e.id, &e.label),
                Accept::Remove => remove(&dir, &e.label),
                Accept::Clone => unreachable!(),
            }
        }
    }
}

fn open_kind(accept: Accept) -> Option<&'static str> {
    match accept {
        Accept::Workspace => Some("workspace"),
        Accept::Tab => Some("tab"),
        Accept::Split => Some("split"),
        Accept::Pane => Some("pane"),
        _ => None,
    }
}

fn herdr(args: &[&str]) -> Result<()> {
    let status = Command::new("herdr").args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("herdr {} failed", args.join(" ")))
    }
}

fn open_repo(target: &str, path: &str, origin: &str, label: &str, cfg: &Config) -> Result<()> {
    if !std::path::Path::new(path).is_dir() {
        return Err(anyhow!("path no longer exists: {path}"));
    }
    match target {
        "workspace" => herdr(&["workspace", "create", "--cwd", path, "--label", label, "--focus"]),
        "tab" => herdr(&["tab", "create", "--cwd", path, "--label", label, "--focus"]),
        "split" => {
            let dir = cfg.get("split_direction", "right");
            let ratio = cfg.get("split_ratio", "0.5");
            let mut args = vec!["pane", "split"];
            if !origin.is_empty() {
                args.push(origin);
            }
            args.extend_from_slice(&["--direction", &dir, "--ratio", &ratio, "--cwd", path, "--focus"]);
            herdr(&args)
        }
        "pane" => {
            if origin.is_empty() {
                return Err(anyhow!("no origin pane to cd into"));
            }
            herdr(&["pane", "send-text", origin, &format!("cd '{path}'")])?;
            herdr(&["pane", "send-keys", origin, "enter"])
        }
        other => Err(anyhow!("unknown target {other}")),
    }
}

fn focus_workspace(id: &str) -> Result<()> {
    herdr(&["workspace", "focus", id])
}

fn focus_agent(id: &str) -> Result<()> {
    herdr(&["agent", "focus", id])
}

fn git_handoff(label: &str) -> Result<()> {
    let installed = Command::new("herdr")
        .args(["plugin", "list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("- git-hub "))
        .unwrap_or(false);
    if installed {
        let _ = herdr(&["plugin", "action", "invoke", "menu", "--plugin", "git-hub"]);
    } else {
        eprintln!("git-hub is not installed — opened {label} in a new tab.");
    }
    Ok(())
}

fn update(rel: &str, label: &str) -> Result<()> {
    println!("\x1b[1mUpdating\x1b[0m {rel}\n");
    let _ = Command::new("ghq").args(["get", "-u", "--", rel]).status();
    println!("\n\x1b[2m{label}: press Enter to close\x1b[0m");
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
    Ok(())
}

fn remove(path: &str, label: &str) -> Result<()> {
    println!("\x1b[1;31mRemove repository\x1b[0m\n  {path}\n");
    print!("Type the repo name (\x1b[1m{label}\x1b[0m) to confirm: ");
    io::stdout().flush().ok();
    let mut reply = String::new();
    io::stdin().read_line(&mut reply)?;
    if reply.trim() == label {
        Command::new("rm").args(["-rf", "--", path]).status()?;
        println!("Removed {label}.");
    } else {
        println!("Aborted.");
    }
    Ok(())
}
