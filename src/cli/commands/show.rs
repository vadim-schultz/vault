//! `vault show` command.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::app::show;
use crate::at_date::AtDate;
use crate::cli::context::Stores;
use crate::cli::support::{rel_path_from_cli, run_blocking, Global};
use crate::paths;

/// Arguments for `vault show`.
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Path to the file.
    pub path: PathBuf,
    /// Timestamp (`YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, or RFC3339).
    #[arg(long)]
    pub at: AtDate,
}

/// Run `vault show`.
pub async fn run(global: &Global, args: ShowArgs) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let rel = rel_path_from_cli(&layout, &args.path)?;
    let at = args.at.as_str().to_string();
    let bytes = run_blocking(move || {
        let stores = Stores::open(&layout)?;
        show::run(&stores.object_store, &stores.meta_index, &rel, &at)
    })
    .await?;
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}
