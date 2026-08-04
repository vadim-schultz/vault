//! `vault log` command.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::app::log;
use crate::cli::context;
use crate::cli::support::{rel_path_from_cli, run_blocking, Global};
use crate::domain::SnapshotEntry;
use crate::paths;

/// Arguments for `vault log`.
#[derive(Debug, Args)]
pub struct LogArgs {
    /// Optional path to filter history.
    pub path: Option<PathBuf>,
}

/// Run `vault log`.
pub async fn run(global: &Global, args: LogArgs) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let rel = args
        .path
        .map(|p| rel_path_from_cli(&layout, &p))
        .transpose()?;
    let entries = run_blocking(move || {
        let meta_index = context::open_meta_index(&layout)?;
        log::run(&meta_index, rel.as_ref())
    })
    .await?;
    print!("{}", render_report(&entries));
    Ok(())
}

fn render_report(entries: &[SnapshotEntry]) -> String {
    if entries.is_empty() {
        return "No snapshots yet.\n".to_string();
    }
    entries.iter().map(render_line).collect()
}

fn render_line(entry: &SnapshotEntry) -> String {
    match &entry.event {
        Some(event) => format!(
            "{} {} {}\n",
            entry.commit_sha.as_str(),
            entry.created_at,
            event.as_str()
        ),
        None => format!("{} {}\n", entry.commit_sha.as_str(), entry.created_at),
    }
}
