//! `vault log` command.

mod render;

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::app::log;
use crate::cli::context::Stores;
use crate::cli::support::{rel_path_from_cli, run_blocking, Global};
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
    let reports = run_blocking(move || {
        let stores = Stores::open(&layout)?;
        log::run(&stores.object_store, &stores.meta_index, rel.as_ref())
    })
    .await?;
    print!("{}", render::render_report(&reports, global.verbose));
    Ok(())
}
