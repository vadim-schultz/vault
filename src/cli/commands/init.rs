//! `vault init` command.

use anyhow::Result;
use clap::Args;

use crate::app::init;
use crate::cli::support::{run_blocking, Global};

/// Arguments for `vault init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Skip installing or starting the background daemon.
    #[arg(long)]
    pub no_service: bool,
}

/// Run `vault init`.
pub async fn run(global: &Global, args: InitArgs) -> Result<()> {
    let vault_path = global.vault_path.clone();
    let ctx = init::InitContext::production();
    let layout = run_blocking(move || init::initialize(&ctx, vault_path, args.no_service)).await?;
    let vault_display = layout.vault_dir.display();
    println!("Vault initialized at {vault_display}");
    if global.verbose {
        eprintln!("initialized vault at {vault_display}");
    }
    Ok(())
}
