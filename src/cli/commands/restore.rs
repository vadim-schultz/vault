//! `vault restore` command.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::app::restore::{self, RestoreOutcome};
use crate::at_date::AtDate;
use crate::cli::context::{self, Stores};
use crate::cli::support::{rel_path_from_cli, run_blocking, Global};
use crate::paths;

/// Arguments for `vault restore`.
#[derive(Debug, Args)]
pub struct RestoreArgs {
    /// Path to the file.
    pub path: PathBuf,
    /// Timestamp (`YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, or RFC3339).
    #[arg(long)]
    pub at: AtDate,
    /// Print what would be restored without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `vault restore`.
pub async fn run(global: &Global, args: RestoreArgs) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let rel = rel_path_from_cli(&layout, &args.path)?;
    let at = args.at.as_str().to_string();
    let dry_run = args.dry_run;
    let outcome = run_blocking(move || {
        let stores = Stores::open(&layout)?;
        let clock = context::clock();
        restore::run(
            &layout,
            &clock,
            &stores.object_store,
            &stores.meta_index,
            &rel,
            &at,
            dry_run,
        )
    })
    .await?;
    println!("{}", render_report(&args.path, dry_run, &outcome));
    Ok(())
}

fn render_report(path: &std::path::Path, dry_run: bool, outcome: &RestoreOutcome) -> String {
    if dry_run {
        return format!("Would restore {} (dry run)", path.display());
    }
    match &outcome.commit_sha {
        Some(sha) => format!(
            "Restored {} ({} bytes, commit {})",
            path.display(),
            outcome.bytes_written,
            sha.as_str()
        ),
        None => format!("{} already matches that version", path.display()),
    }
}
