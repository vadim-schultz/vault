//! CLI argument parsing and subcommand dispatch.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use crate::config;
use crate::daemon;
use crate::error::VaultError;
use crate::init;
use crate::paths;
use crate::status;

/// Automatic version history.
#[derive(Debug, Parser)]
#[command(name = "vault", version, about)]
pub struct Cli {
    /// Enable verbose output.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Path to the `.vault/` directory (auto-discovered when omitted).
    #[arg(long, global = true)]
    pub vault_path: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Vault subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a vault in the current directory.
    Init {
        /// Skip installing or starting the background daemon.
        #[arg(long)]
        no_service: bool,
    },
    /// Show a file as it was at a given timestamp.
    Show {
        /// Path to the file.
        path: std::path::PathBuf,
        /// Timestamp (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`).
        #[arg(long)]
        at: String,
    },
    /// Restore a file to an earlier version.
    Restore {
        /// Path to the file.
        path: std::path::PathBuf,
        /// Timestamp (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`).
        #[arg(long)]
        at: String,
        /// Print what would be restored without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Browse version history for a file or the whole vault.
    Log {
        /// Optional path to filter history.
        path: Option<std::path::PathBuf>,
    },
    /// Compare a file between two points in time.
    Diff {
        /// Path to the file.
        path: std::path::PathBuf,
        /// Start timestamp.
        #[arg(long)]
        at: Option<String>,
        /// End timestamp.
        #[arg(long)]
        to: Option<String>,
    },
    /// Report watcher health and last snapshot time.
    Status,
    /// List tracked files and their latest version timestamp.
    List,
    /// Add an ignore glob pattern.
    Ignore {
        /// Glob pattern to ignore (e.g. `*.pdf`).
        pattern: String,
    },
    /// Run the singleton background watcher (hidden).
    #[command(hide = true)]
    Daemon {
        /// Run in the foreground (used by systemd and tests).
        #[arg(long)]
        foreground: bool,
    },
}

/// Run the CLI, parsing arguments from the environment.
///
/// # Errors
///
/// Returns an error when a subcommand fails or is not yet implemented.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli).await
}

async fn dispatch(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        return Ok(());
    };

    match command {
        Command::Init { no_service } => {
            let paths = run_blocking(move || init::initialize(cli.vault_path, no_service)).await?;
            let vault_display = paths.vault_dir.display();
            println!("Vault initialized at {vault_display}");
            if cli.verbose {
                eprintln!("initialized vault at {vault_display}");
            }
            Ok(())
        }
        Command::Status => {
            let report = run_blocking(status::report).await?;
            println!("{report}");
            Ok(())
        }
        Command::Ignore { pattern } => {
            let vault_paths = paths::resolve_vault(cli.vault_path)?;
            let pattern_for_msg = pattern.clone();
            run_blocking(move || config::VaultConfig::add_ignore_pattern(&vault_paths, &pattern))
                .await?;
            println!("Added ignore pattern: {pattern_for_msg}");
            Ok(())
        }
        Command::Daemon { foreground: _ } => run_daemon().await,
        Command::Show { .. } => stub("show"),
        Command::Restore { .. } => stub("restore"),
        Command::Log { .. } => stub("log"),
        Command::Diff { .. } => stub("diff"),
        Command::List => stub("list"),
    }
}

async fn run_blocking<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T, VaultError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await?
        .map_err(|err| anyhow::anyhow!(err))
}

async fn run_daemon() -> Result<()> {
    match daemon::run_foreground().await {
        Ok(()) => Ok(()),
        Err(VaultError::DaemonAlreadyRunning { pid }) => {
            bail!("vault daemon already running (pid {pid})")
        }
        Err(err) => Err(anyhow::anyhow!(err)),
    }
}

fn stub(name: &str) -> Result<()> {
    bail!("{name} not implemented yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo_toml() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    }

    #[test]
    fn help_lists_subcommands() {
        let cli = Cli::try_parse_from(["vault", "init"]).expect("parse init");
        assert!(matches!(cli.command, Some(Command::Init { .. })));
    }
}
