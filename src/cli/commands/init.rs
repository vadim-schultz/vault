//! `vault init` command.

use anyhow::Result;
use clap::Args;

use crate::app::init::{self, DaemonAction, InitOutcome};
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
    let (layout, outcome) =
        run_blocking(move || init::initialize(&ctx, vault_path, args.no_service)).await?;
    let vault_display = layout.vault_dir.display().to_string();
    print_outcome(&outcome, &vault_display);
    if global.verbose {
        eprintln!("init outcome for {vault_display}: {outcome:?}");
    }
    Ok(())
}

fn print_outcome(outcome: &InitOutcome, vault_display: &str) {
    match outcome {
        InitOutcome::Created => print_created(vault_display),
        InitOutcome::AlreadyReady(daemon) => print_already_ready(vault_display, *daemon),
        InitOutcome::Repaired { filled, daemon } => print_repaired(vault_display, filled, *daemon),
    }
}

fn print_created(vault_display: &str) {
    println!("Vault initialized at {vault_display}");
}

fn print_already_ready(vault_display: &str, daemon: DaemonAction) {
    println!("Vault already initialized at {vault_display}");
    println!("{}", daemon_line(daemon));
}

fn print_repaired(vault_display: &str, filled: &[&'static str], daemon: DaemonAction) {
    println!(
        "Vault repaired at {vault_display} (restored: {})",
        filled.join(", ")
    );
    if filled.contains(&crate::paths::CONFIG_FILE) {
        println!("restored config.toml with defaults — re-apply any custom watch_roots/ignore");
    }
    println!("{}", daemon_line(daemon));
}

fn daemon_line(daemon: DaemonAction) -> String {
    match daemon {
        DaemonAction::AlreadyRunning => match crate::daemon::read_heartbeat() {
            Some(heartbeat) => format!("Daemon already running (pid {})", heartbeat.pid),
            None => "Daemon already running".to_string(),
        },
        DaemonAction::Started => "Daemon was stopped — restarted it".to_string(),
        DaemonAction::SkippedNoService => "Daemon start skipped (--no-service)".to_string(),
    }
}
