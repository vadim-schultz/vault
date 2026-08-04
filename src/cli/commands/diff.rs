//! `vault diff` command.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::app::diff::{self, DiffOutcome};
use crate::at_date::AtDate;
use crate::cli::context::Stores;
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
    print!("{}", render_report(&outcome));
    Ok(())
}

fn render_report(outcome: &DiffOutcome) -> String {
    if outcome.left == outcome.right {
        return "No differences.\n".to_string();
    }
    render_content_diff(outcome)
}

fn render_content_diff(outcome: &DiffOutcome) -> String {
    let Some((left_text, right_text)) = as_utf8_pair(outcome.left.as_ref(), outcome.right.as_ref())
    else {
        return "Binary files differ.\n".to_string();
    };
    similar::TextDiff::from_lines(left_text, right_text)
        .unified_diff()
        .header(&outcome.left_label, &outcome.right_label)
        .to_string()
}

fn as_utf8_pair<'a>(
    left: Option<&'a Vec<u8>>,
    right: Option<&'a Vec<u8>>,
) -> Option<(&'a str, &'a str)> {
    let left = std::str::from_utf8(left.map_or(&[][..], Vec::as_slice)).ok()?;
    let right = std::str::from_utf8(right.map_or(&[][..], Vec::as_slice)).ok()?;
    Some((left, right))
}
