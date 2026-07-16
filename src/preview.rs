//! Preview text for the highlighted entry. Reuses the bash `preview.sh` (git
//! tree, agent output, workspace summary) and converts its ANSI to ratatui.
//!
//! `render` shells out and costs ~100ms on a large repo — mostly `git status`
//! — so it runs on a [`Worker`] thread rather than between a keypress and the
//! next frame.

use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use ansi_to_tui::IntoText;
use ratatui::text::Text;

use crate::data::{Config, Entry, Kind};

/// A render request. `seq` lets the UI drop results it has already scrolled past.
struct Job {
    seq: u64,
    entry: Entry,
}

/// A finished preview, tagged with the `seq` of the job that produced it.
pub struct Done {
    pub seq: u64,
    pub text: Text<'static>,
}

/// Renders previews off the UI thread, newest request first.
pub struct Worker {
    jobs: Sender<Job>,
    done: Receiver<Done>,
}

impl Worker {
    pub fn spawn(script_dir: String, root: String, cfg: Config) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done_rx) = mpsc::channel::<Done>();
        thread::spawn(move || {
            while let Ok(mut job) = job_rx.recv() {
                // Skip ahead to the newest request: while the user scrolls, only
                // the entry they land on is worth the subprocess.
                while let Ok(newer) = job_rx.try_recv() {
                    job = newer;
                }
                let text = render(&job.entry, &script_dir, &root, &cfg);
                if done_tx.send(Done { seq: job.seq, text }).is_err() {
                    break; // the UI is gone
                }
            }
        });
        Self {
            jobs: job_tx,
            done: done_rx,
        }
    }

    /// Queues a render. Returns false if the worker thread is gone.
    pub fn request(&self, seq: u64, entry: Entry) -> bool {
        self.jobs.send(Job { seq, entry }).is_ok()
    }

    /// Non-blocking: the next finished preview, if one has landed.
    pub fn poll(&self) -> Option<Done> {
        self.done.try_recv().ok()
    }
}

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
