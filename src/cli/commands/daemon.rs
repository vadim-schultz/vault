//! `vault daemon` command (hidden; used by systemd and tests).

use anyhow::Result;
use clap::Args;

use crate::daemon;

/// Arguments for the hidden `vault daemon` command.
#[derive(Debug, Args)]
pub struct DaemonArgs {
    /// Run in the foreground (used by systemd and tests).
    #[arg(long)]
    pub foreground: bool,
}

/// Run `vault daemon`.
pub async fn run(_args: DaemonArgs) -> Result<()> {
    daemon::run_foreground()
        .await
        .map_err(|err| anyhow::anyhow!(err))
}
