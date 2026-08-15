//! CLI argument parsing and subcommand dispatch.

mod commands;
mod context;
mod render;
mod support;

use anyhow::Result;
use clap::{Parser, Subcommand};

use support::Global;

/// Automatic version history.
#[derive(Debug, Parser)]
#[command(name = "vault", version, about)]
pub struct Cli {
    /// Enable verbose output.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Path to the `.vault/` directory (default: `./.vault` under the current directory).
    #[arg(long, global = true)]
    pub vault_path: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Vault subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a vault in the current directory.
    Init(commands::init::InitArgs),
    /// Show a file as it was at a given timestamp.
    Show(commands::show::ShowArgs),
    /// Restore a file to an earlier version.
    Restore(commands::restore::RestoreArgs),
    /// Browse version history for a file or the whole vault.
    Log(commands::log::LogArgs),
    /// Compare a file between two points in time.
    Diff(commands::diff::DiffArgs),
    /// Report watcher health and last snapshot time.
    Status,
    /// Remove registered vaults whose root directory no longer exists.
    Prune,
    /// Rebuild `meta.db` by replaying `.git`'s commit history.
    Reindex(commands::reindex::ReindexArgs),
    /// List tracked files and their latest version timestamp.
    List,
    /// Add an ignore glob pattern.
    Ignore(commands::ignore::IgnoreArgs),
    /// Run the singleton background watcher (hidden).
    #[command(hide = true)]
    Daemon(commands::daemon::DaemonArgs),
}

/// Run the CLI, parsing arguments from the environment.
///
/// # Errors
///
/// Returns an error when argument parsing or command execution fails.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli).await
}

async fn dispatch(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        return Ok(());
    };
    let global = Global {
        vault_path: cli.vault_path,
        verbose: cli.verbose,
    };

    match command {
        Command::Init(args) => commands::init::run(&global, args).await,
        Command::Show(args) => commands::show::run(&global, args).await,
        Command::Restore(args) => commands::restore::run(&global, args).await,
        Command::Log(args) => commands::log::run(&global, args).await,
        Command::Diff(args) => commands::diff::run(&global, args).await,
        Command::Status => commands::status::run().await,
        Command::Prune => commands::prune::run().await,
        Command::Reindex(args) => commands::reindex::run(&global, args).await,
        Command::List => commands::list::run(&global).await,
        Command::Ignore(args) => commands::ignore::run(&global, args).await,
        Command::Daemon(args) => commands::daemon::run(args).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn version_matches_cargo_toml() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    }

    #[test]
    fn help_lists_subcommands() {
        let cli = Cli::try_parse_from(["vault", "init"]).expect("parse init");
        assert!(matches!(cli.command, Some(Command::Init(_))));
    }

    #[test]
    fn vault_path_help_does_not_promise_discovery() {
        let help = Cli::command().render_long_help().to_string();
        assert!(!help.contains("auto-discovered"));
        assert!(help.contains("current directory"));
    }
}
