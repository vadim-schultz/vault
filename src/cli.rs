//! CLI argument parsing and subcommand dispatch.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

/// Document version history — not secrets management.
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
    Init,
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
}

/// Run the CLI, parsing arguments from the environment.
///
/// # Errors
///
/// Returns an error when a subcommand fails or is not yet implemented.
#[allow(clippy::unused_async)] // Subcommands gain real async I/O in Chapter 3+.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli)
}

fn dispatch(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        return Ok(());
    };

    match command {
        Command::Init => stub("init"),
        Command::Show { .. } => stub("show"),
        Command::Restore { .. } => stub("restore"),
        Command::Log { .. } => stub("log"),
        Command::Diff { .. } => stub("diff"),
        Command::Status => stub("status"),
        Command::List => stub("list"),
        Command::Ignore { .. } => stub("ignore"),
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
        assert!(matches!(cli.command, Some(Command::Init)));
    }
}
