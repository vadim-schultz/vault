//! `vault diff` command.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::app::diff::{self, DiffOutcome};
use crate::at_date::AtDate;
use crate::cli::context::Stores;
use crate::cli::render::{render_content_diff, DiffInput};
use crate::cli::support::{rel_path_from_cli, run_blocking, Global};
use crate::paths;

/// Arguments for `vault diff`.
#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Path to the file.
    pub path: PathBuf,
    /// Start timestamp.
    #[arg(long)]
    pub at: Option<AtDate>,
    /// End timestamp.
    #[arg(long, requires = "at")]
    pub to: Option<AtDate>,
}

/// Run `vault diff`.
pub async fn run(global: &Global, args: DiffArgs) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let rel = rel_path_from_cli(&layout, &args.path)?;
    let path_display = rel.as_str().to_string();
    let at = args.at.map(|a| a.as_str().to_string());
    let to = args.to.map(|t| t.as_str().to_string());
    let outcome = run_blocking(move || {
        let stores = Stores::open(&layout)?;
        diff::run(
            &layout,
            &stores.object_store,
            &stores.meta_index,
            &rel,
            at.as_deref(),
            to.as_deref(),
        )
    })
    .await?;
    print!("{}", render_report(&path_display, &outcome));
    Ok(())
}

fn render_report(path: &str, outcome: &DiffOutcome) -> String {
    if outcome.left == outcome.right {
        return "No differences.\n".to_string();
    }
    render_content_diff(&DiffInput {
        path,
        left_label: &outcome.left_label,
        right_label: &outcome.right_label,
        left: outcome.left.as_deref(),
        right: outcome.right.as_deref(),
    })
}
