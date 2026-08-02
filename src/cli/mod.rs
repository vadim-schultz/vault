//! CLI argument parsing and subcommand dispatch.

mod render;

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use crate::adapters::{GixObjectStore, SqliteMetaIndex, SystemClock};
use crate::app::{add_ignore, diff, init, list, log, restore, show, status};
use crate::at_date::AtDate;
use crate::daemon;
use crate::domain::{RelPath, VaultLayout};
use crate::error::VaultError;
use crate::paths;

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
    Init {
        /// Skip installing or starting the background daemon.
        #[arg(long)]
        no_service: bool,
    },
    /// Show a file as it was at a given timestamp.
    Show {
        /// Path to the file.
        path: std::path::PathBuf,
        /// Timestamp (`YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, or RFC3339).
        #[arg(long)]
        at: AtDate,
    },
    /// Restore a file to an earlier version.
    Restore {
        /// Path to the file.
        path: std::path::PathBuf,
        /// Timestamp (`YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, or RFC3339).
        #[arg(long)]
        at: AtDate,
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
        at: Option<AtDate>,
        /// End timestamp.
        #[arg(long)]
        to: Option<AtDate>,
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
/// Returns an error when argument parsing or command execution fails.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli).await
}

async fn dispatch(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        return Ok(());
    };

    match command {
        Command::Init { no_service } => handle_init(cli.vault_path, cli.verbose, no_service).await,
        Command::Status => handle_status().await,
        Command::Ignore { pattern } => handle_ignore(cli.vault_path, pattern).await,
        Command::Daemon { foreground: _ } => run_daemon().await,
        Command::Show { path, at } => handle_show(cli.vault_path, path, at).await,
        Command::Restore { path, at, dry_run } => {
            handle_restore(cli.vault_path, path, at, dry_run).await
        }
        Command::Log { path } => handle_log(cli.vault_path, path).await,
        Command::Diff { path, at, to } => handle_diff(cli.vault_path, path, at, to).await,
        Command::List => handle_list(cli.vault_path).await,
    }
}

async fn handle_init(vault_path: Option<PathBuf>, verbose: bool, no_service: bool) -> Result<()> {
    let ctx = init::InitContext::production();
    let layout = run_blocking(move || init::initialize(&ctx, vault_path, no_service)).await?;
    let vault_display = layout.vault_dir.display();
    println!("Vault initialized at {vault_display}");
    if verbose {
        eprintln!("initialized vault at {vault_display}");
    }
    Ok(())
}

async fn handle_status() -> Result<()> {
    let report = run_blocking(status::report_default).await?;
    println!("{report}");
    Ok(())
}

async fn handle_ignore(vault_path: Option<PathBuf>, pattern: String) -> Result<()> {
    let layout = paths::resolve_vault(vault_path)?;
    let pattern_for_msg = pattern.clone();
    run_blocking(move || add_ignore::add_pattern(&layout, &pattern)).await?;
    println!("Added ignore pattern: {pattern_for_msg}");
    Ok(())
}

fn rel_path_from_cli(layout: &VaultLayout, path: &Path) -> Result<RelPath, VaultError> {
    if path.is_absolute() {
        RelPath::from_worktree(&layout.worktree, path)
    } else {
        RelPath::from_rel(path)
    }
}

async fn handle_show(vault_path: Option<PathBuf>, path: PathBuf, at: AtDate) -> Result<()> {
    let layout = paths::resolve_vault(vault_path)?;
    let rel = rel_path_from_cli(&layout, &path)?;
    let at = at.as_str().to_string();
    let bytes = run_blocking(move || {
        let object_store = GixObjectStore::open(&layout)?;
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path())?;
        show::run(&object_store, &meta_index, &rel, &at)
    })
    .await?;
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}

async fn handle_restore(
    vault_path: Option<PathBuf>,
    path: PathBuf,
    at: AtDate,
    dry_run: bool,
) -> Result<()> {
    let layout = paths::resolve_vault(vault_path)?;
    let rel = rel_path_from_cli(&layout, &path)?;
    let at = at.as_str().to_string();
    let outcome = run_blocking(move || {
        let object_store = GixObjectStore::open(&layout)?;
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path())?;
        restore::run(
            &layout,
            &SystemClock,
            &object_store,
            &meta_index,
            &rel,
            &at,
            dry_run,
        )
    })
    .await?;
    println!("{}", render::restore_report(&path, dry_run, &outcome));
    Ok(())
}

async fn handle_log(vault_path: Option<PathBuf>, path: Option<PathBuf>) -> Result<()> {
    let layout = paths::resolve_vault(vault_path)?;
    let rel = path.map(|p| rel_path_from_cli(&layout, &p)).transpose()?;
    let entries = run_blocking(move || {
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path())?;
        log::run(&meta_index, rel.as_ref())
    })
    .await?;
    print!("{}", render::log_report(&entries));
    Ok(())
}

async fn handle_diff(
    vault_path: Option<PathBuf>,
    path: PathBuf,
    at: Option<AtDate>,
    to: Option<AtDate>,
) -> Result<()> {
    if to.is_some() && at.is_none() {
        bail!("--to requires --at");
    }
    let layout = paths::resolve_vault(vault_path)?;
    let rel = rel_path_from_cli(&layout, &path)?;
    let at = at.map(|a| a.as_str().to_string());
    let to = to.map(|t| t.as_str().to_string());
    let outcome = run_blocking(move || {
        let object_store = GixObjectStore::open(&layout)?;
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path())?;
        diff::run(
            &layout,
            &object_store,
            &meta_index,
            &rel,
            at.as_deref(),
            to.as_deref(),
        )
    })
    .await?;
    print!("{}", render::diff_report(&outcome));
    Ok(())
}

async fn handle_list(vault_path: Option<PathBuf>) -> Result<()> {
    let layout = paths::resolve_vault(vault_path)?;
    let files = run_blocking(move || {
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path())?;
        list::run(&meta_index)
    })
    .await?;
    print!("{}", render::list_report(&files));
    Ok(())
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
    daemon::run_foreground()
        .await
        .map_err(|err| anyhow::anyhow!(err))
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
        assert!(matches!(cli.command, Some(Command::Init { .. })));
    }

    #[test]
    fn vault_path_help_does_not_promise_discovery() {
        let help = Cli::command().render_long_help().to_string();
        assert!(!help.contains("auto-discovered"));
        assert!(help.contains("current directory"));
    }
}
