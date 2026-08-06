//! `vault show` command.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::app::show::{self, ShowOutput};
use crate::at_date::AtDate;
use crate::cli::context::Stores;
use crate::cli::render::render_full_diffs;
use crate::cli::support::{rel_path_from_cli, run_blocking, Global};
use crate::domain::CommitReport;
use crate::paths;

/// Arguments for `vault show`.
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Path to a file or directory; omitted means the whole vault.
    pub path: Option<PathBuf>,
    /// Timestamp (`YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, or RFC3339).
    #[arg(long)]
    pub at: AtDate,
}

/// Run `vault show`.
pub async fn run(global: &Global, args: ShowArgs) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let rel = args
        .path
        .map(|p| rel_path_from_cli(&layout, &p))
        .transpose()?;
    let at = args.at.as_str().to_string();
    let output = run_blocking(move || {
        let stores = Stores::open(&layout)?;
        show::run(&stores.object_store, &stores.meta_index, rel.as_ref(), &at)
    })
    .await?;
    match output {
        ShowOutput::Content(bytes) => std::io::stdout().write_all(&bytes)?,
        ShowOutput::Report(report) => print!("{}", render_report(&report)),
    }
    Ok(())
}

fn render_report(report: &CommitReport) -> String {
    format!("{}\n{}", report.message, render_full_diffs(&report.files))
}
