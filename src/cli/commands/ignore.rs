//! `vault ignore` command.

use anyhow::Result;
use clap::Args;

use crate::app::add_ignore;
use crate::cli::support::{run_blocking, Global};
use crate::paths;

/// Arguments for `vault ignore`.
#[derive(Debug, Args)]
pub struct IgnoreArgs {
    /// Glob pattern to ignore (e.g. `*.pdf`).
    pub pattern: String,
}

/// Run `vault ignore`.
pub async fn run(global: &Global, args: IgnoreArgs) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let pattern = args.pattern;
    let pattern_for_msg = pattern.clone();
    run_blocking(move || add_ignore::add_pattern(&layout, &pattern)).await?;
    println!("Added ignore pattern: {pattern_for_msg}");
    Ok(())
}
