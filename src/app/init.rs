//! `vault init` use-case.

use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::{
    DetachedSpawnService, GixObjectStore, NoopService, SqliteMetaIndex, SystemClock,
    SystemdService, TomlRegistry,
};
use crate::app::snapshot;
use crate::config::VaultConfig;
use crate::daemon;
use crate::domain::{missing_markers, VaultLayout, VaultState};
use crate::error::VaultError;
use crate::paths::{resolve_init, skip_service, CONFIG_FILE, GIT_DIR, META_DB, README_FILE};
use crate::ports::{RegistryStore, ServiceManager};

const README: &str = "\
Vault storage (recovery guide)
=============================

This directory is managed by the vault CLI. You normally do not edit it.

Layout
------

  config.toml   Watch roots and ignore patterns
  .git/         Git object store (file content history)
  meta.db       SQLite index (paths, timestamps, commit SHAs)

Global registry
---------------

  vault init registers this directory in the user-wide registry.toml
  (see docs). A singleton background daemon watches all registered vaults.

Inspect without vault (optional)
--------------------------------

  git --git-dir=.vault/.git log --oneline
  sqlite3 .vault/meta.db \".schema\"

Vault does not invoke git or sqlite3 internally; the on-disk layout is standard.

Daily use
---------

  vault show PATH --at DATE
  vault restore PATH --at DATE
";

/// Options controlling post-init daemon startup.
#[derive(Debug, Clone, Copy, Default)]
pub struct InitOptions {
    /// Skip service install and daemon start.
    pub no_service: bool,
}

/// What happened to the singleton daemon while ensuring it is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonAction {
    /// It was already running.
    AlreadyRunning,
    /// It was stopped, so it was (re)started.
    Started,
    /// Daemon start was skipped (`--no-service` / `VAULT_NO_SERVICE`).
    SkippedNoService,
}

/// Outcome of a `vault init` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitOutcome {
    /// No vault existed; it was fully provisioned.
    Created,
    /// The vault was already fully initialized; no filesystem changes were
    /// made to it.
    AlreadyReady(DaemonAction),
    /// The vault was missing some markers that are safe to regenerate
    /// (`README`, `config.toml`); they were restored.
    Repaired {
        /// Markers that were regenerated.
        filled: Vec<&'static str>,
        /// What happened to the daemon.
        daemon: DaemonAction,
    },
}

/// Context for vault initialization.
pub struct InitContext {
    /// Registry persistence.
    pub registry: Arc<dyn RegistryStore>,
    /// Service manager for daemon startup.
    pub service: Arc<dyn ServiceManager>,
}

impl InitContext {
    /// Build the default production context.
    #[must_use]
    pub fn production() -> Self {
        let service: Arc<dyn ServiceManager> = if skip_service() {
            Arc::new(NoopService)
        } else if SystemdService::is_available() {
            Arc::new(SystemdService)
        } else {
            Arc::new(DetachedSpawnService)
        };
        Self {
            registry: Arc::new(TomlRegistry),
            service,
        }
    }
}

/// Initialize a vault from CLI-style arguments.
///
/// # Errors
///
/// Returns [`VaultError`] when path resolution or initialization fails.
pub fn initialize(
    ctx: &InitContext,
    vault_path: Option<PathBuf>,
    no_service: bool,
) -> Result<(VaultLayout, InitOutcome), VaultError> {
    let (layout, state) = resolve_init(vault_path)?;
    let options = InitOptions {
        no_service: no_service || skip_service(),
    };
    let outcome = run(ctx, &layout, state, options)?;
    Ok((layout, outcome))
}

/// Initialize (or repair, or no-op on) the vault at `layout`, depending on
/// its current [`VaultState`].
///
/// # Errors
///
/// Returns [`VaultError`] when provisioning, snapshot, registration, or
/// repair fails, or when `state` is [`VaultState::Partial`] and a
/// data-bearing marker (`.git`/`meta.db`) is missing — repairing those would
/// risk silently orphaning or hiding history, so it is refused rather than
/// attempted.
pub fn run(
    ctx: &InitContext,
    layout: &VaultLayout,
    state: VaultState,
    options: InitOptions,
) -> Result<InitOutcome, VaultError> {
    match state {
        VaultState::Absent => create(ctx, layout, options),
        VaultState::Ready => reconfirm(ctx, layout, options),
        VaultState::Partial(present) => repair(ctx, layout, options, &present),
    }
}

fn create(
    ctx: &InitContext,
    layout: &VaultLayout,
    options: InitOptions,
) -> Result<InitOutcome, VaultError> {
    provision_store(layout)?;
    let config = VaultConfig::defaults();
    config.write_to(&layout.config_path())?;
    take_baseline(layout, &config)?;
    register_globally(ctx, layout)?;
    ensure_daemon(ctx, options)?;
    Ok(InitOutcome::Created)
}

fn reconfirm(
    ctx: &InitContext,
    layout: &VaultLayout,
    options: InitOptions,
) -> Result<InitOutcome, VaultError> {
    register_globally(ctx, layout)?;
    let daemon = ensure_daemon(ctx, options)?;
    Ok(InitOutcome::AlreadyReady(daemon))
}

fn repair(
    ctx: &InitContext,
    layout: &VaultLayout,
    options: InitOptions,
    present: &[&'static str],
) -> Result<InitOutcome, VaultError> {
    let missing = missing_markers(present);
    if missing.contains(&GIT_DIR) || missing.contains(&META_DB) {
        return Err(VaultError::PartialVault {
            path: layout.vault_dir.clone(),
            found: present.join(", "),
            missing: missing.join(", "),
        });
    }
    let filled = repair_safe_markers(layout, &missing)?;
    register_globally(ctx, layout)?;
    let daemon = ensure_daemon(ctx, options)?;
    Ok(InitOutcome::Repaired { filled, daemon })
}

/// Regenerate whichever of `missing` are safe to regenerate without data
/// risk (`README`, `config.toml`). Callers must have already ruled out
/// `.git`/`meta.db` being present in `missing`.
fn repair_safe_markers(
    layout: &VaultLayout,
    missing: &[&'static str],
) -> Result<Vec<&'static str>, VaultError> {
    let mut filled = Vec::new();
    for marker in missing {
        match *marker {
            README_FILE => {
                std::fs::write(layout.readme_path(), README)?;
                filled.push(README_FILE);
            }
            CONFIG_FILE => {
                VaultConfig::defaults().write_to(&layout.config_path())?;
                filled.push(CONFIG_FILE);
            }
            other => unreachable!("repair_safe_markers called with data-bearing marker {other}"),
        }
    }
    Ok(filled)
}

fn provision_store(layout: &VaultLayout) -> Result<(), VaultError> {
    std::fs::create_dir_all(&layout.vault_dir)?;
    GixObjectStore::init(layout)?;
    SqliteMetaIndex::open(layout.meta_db_path())?;
    std::fs::write(layout.readme_path(), README)?;
    Ok(())
}

fn take_baseline(layout: &VaultLayout, config: &VaultConfig) -> Result<(), VaultError> {
    let object_store = GixObjectStore::open(layout)?;
    let meta_index = SqliteMetaIndex::open(layout.meta_db_path())?;
    snapshot::baseline(layout, config, &SystemClock, &object_store, &meta_index)
}

fn register_globally(ctx: &InitContext, layout: &VaultLayout) -> Result<(), VaultError> {
    ctx.registry.register(&layout.worktree)?;
    Ok(())
}

fn ensure_daemon(ctx: &InitContext, options: InitOptions) -> Result<DaemonAction, VaultError> {
    if options.no_service {
        return Ok(DaemonAction::SkippedNoService);
    }
    if daemon::is_running() {
        return Ok(DaemonAction::AlreadyRunning);
    }
    ctx.service.start()?;
    Ok(DaemonAction::Started)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fakes::RecordingServiceManager;
    use crate::adapters::NoopService;

    #[test]
    fn starts_service_once_when_daemon_stopped() {
        let recorder = Arc::new(RecordingServiceManager::default());
        recorder.start().expect("start");
        assert_eq!(*recorder.starts.lock().unwrap(), 1);
    }

    #[test]
    fn does_not_start_when_daemon_already_running() {
        let _state_lock = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::env::set_var(crate::paths::STATE_DIR_ENV, dir.path());
        let _guard = crate::daemon::DaemonGuard::acquire().expect("daemon lock");
        let recorder = Arc::new(RecordingServiceManager::default());
        let ctx = InitContext {
            registry: Arc::new(TomlRegistry),
            service: recorder.clone(),
        };
        let action = ensure_daemon(&ctx, InitOptions::default()).expect("ensure daemon");
        assert_eq!(action, DaemonAction::AlreadyRunning);
        assert_eq!(*recorder.starts.lock().expect("lock"), 0);
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }

    #[test]
    fn ensure_daemon_skips_when_no_service() {
        let recorder = Arc::new(RecordingServiceManager::default());
        let ctx = InitContext {
            registry: Arc::new(TomlRegistry),
            service: recorder.clone(),
        };
        let action = ensure_daemon(&ctx, InitOptions { no_service: true }).expect("ensure daemon");
        assert_eq!(action, DaemonAction::SkippedNoService);
        assert_eq!(*recorder.starts.lock().expect("lock"), 0);
    }

    #[test]
    fn ready_state_registers_and_ensures_daemon_without_provisioning() {
        let _state_lock = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let state_dir = tempfile::TempDir::new().expect("state tempdir");
        std::env::set_var(crate::paths::STATE_DIR_ENV, state_dir.path());
        let _daemon_guard = crate::daemon::DaemonGuard::acquire().expect("daemon lock");

        let vault_dir = tempfile::TempDir::new().expect("vault tempdir");
        let layout = VaultLayout::from_worktree(vault_dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        // Deliberately do NOT create any of the four init markers — Ready-branch
        // handling must not touch the filesystem, so a missing marker would only
        // surface if the branch wrongly tried to provision.
        let recorder = Arc::new(RecordingServiceManager::default());
        let ctx = InitContext {
            registry: Arc::new(TomlRegistry),
            service: recorder.clone(),
        };

        let outcome =
            run(&ctx, &layout, VaultState::Ready, InitOptions::default()).expect("ready run");

        assert_eq!(
            outcome,
            InitOutcome::AlreadyReady(DaemonAction::AlreadyRunning)
        );
        assert_eq!(*recorder.starts.lock().expect("lock"), 0);
        assert!(!layout.readme_path().exists());
        assert!(!layout.config_path().exists());
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }

    #[test]
    fn ready_state_starts_daemon_when_stopped() {
        let _state_lock = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let state_dir = tempfile::TempDir::new().expect("state tempdir");
        std::env::set_var(crate::paths::STATE_DIR_ENV, state_dir.path());
        // No DaemonGuard acquired — daemon::is_running() reads this fresh,
        // empty state dir and reports stopped.

        let vault_dir = tempfile::TempDir::new().expect("vault tempdir");
        let layout = VaultLayout::from_worktree(vault_dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        let recorder = Arc::new(RecordingServiceManager::default());
        let ctx = InitContext {
            registry: Arc::new(TomlRegistry),
            service: recorder.clone(),
        };

        let outcome =
            run(&ctx, &layout, VaultState::Ready, InitOptions::default()).expect("ready run");

        assert_eq!(outcome, InitOutcome::AlreadyReady(DaemonAction::Started));
        assert_eq!(*recorder.starts.lock().expect("lock"), 1);
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }

    #[test]
    fn partial_state_repairs_readme_and_config_when_git_and_meta_present() {
        let _state_lock = crate::paths::STATE_ENV_LOCK.lock().expect("lock");
        let state_dir = tempfile::TempDir::new().expect("state tempdir");
        std::env::set_var(crate::paths::STATE_DIR_ENV, state_dir.path());
        let _daemon_guard = crate::daemon::DaemonGuard::acquire().expect("daemon lock");

        let vault_dir = tempfile::TempDir::new().expect("vault tempdir");
        let layout = VaultLayout::from_worktree(vault_dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        crate::storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git init");
        crate::storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite init");
        let present = vec![GIT_DIR, META_DB];
        let recorder = Arc::new(RecordingServiceManager::default());
        let ctx = InitContext {
            registry: Arc::new(TomlRegistry),
            service: recorder.clone(),
        };

        let outcome = run(
            &ctx,
            &layout,
            VaultState::Partial(present),
            InitOptions::default(),
        )
        .expect("partial run");

        let InitOutcome::Repaired { mut filled, daemon } = outcome else {
            panic!("expected Repaired outcome");
        };
        filled.sort_unstable();
        assert_eq!(filled, vec![README_FILE, CONFIG_FILE]);
        assert_eq!(daemon, DaemonAction::AlreadyRunning);
        assert!(layout.readme_path().exists());
        assert!(layout.config_path().exists());
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }

    #[test]
    fn partial_state_refuses_repair_when_git_missing() {
        let vault_dir = tempfile::TempDir::new().expect("vault tempdir");
        let layout = VaultLayout::from_worktree(vault_dir.path().to_path_buf());
        std::fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        crate::storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite init");
        std::fs::write(layout.readme_path(), b"x").expect("readme");
        std::fs::write(layout.config_path(), b"x").expect("config");
        let present = vec![README_FILE, CONFIG_FILE, META_DB];
        let ctx = InitContext {
            registry: Arc::new(TomlRegistry),
            service: Arc::new(NoopService),
        };

        let err = run(
            &ctx,
            &layout,
            VaultState::Partial(present),
            InitOptions::default(),
        )
        .expect_err("should refuse repair");

        match err {
            VaultError::PartialVault { missing, .. } => assert!(missing.contains(GIT_DIR)),
            other => panic!("expected PartialVault, got {other:?}"),
        }
    }

    #[test]
    fn noop_adapter_never_starts_anything() {
        let noop = NoopService;
        assert!(noop.start().is_ok());
        assert_eq!(noop.state(), crate::ports::ServiceState::Unsupported);
    }
}
