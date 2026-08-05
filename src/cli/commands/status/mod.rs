//! `vault status` command.

mod render;

use anyhow::Result;

use crate::app::status;
use crate::cli::support::run_blocking;

/// Run `vault status`.
pub async fn run() -> Result<()> {
    let report = run_blocking(status::report_default).await?;
    println!("{report}");
    Ok(())
}
