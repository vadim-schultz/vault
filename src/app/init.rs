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
use crate::domain::VaultLayout;
use crate::error::VaultError;
use crate::paths::{resolve_init, skip_service};
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
) -> Result<VaultLayout, VaultError> {
    let layout = resolve_init(vault_path)?;
    let options = InitOptions {
        no_service: no_service || skip_service(),
    };
    run(ctx, &layout, options)?;
    Ok(layout)
}

/// Initialize a new vault at `layout`.
///
/// # Errors
///
/// Returns [`VaultError`] when provisioning, snapshot, or registration fails.
pub fn run(
    ctx: &InitContext,
    layout: &VaultLayout,
    options: InitOptions,
) -> Result<(), VaultError> {
    provision_store(layout)?;
    let config = VaultConfig::defaults();
    config.write_to(&layout.config_path())?;
    take_baseline(layout, &config)?;
    register_globally(ctx, layout)?;
    if !options.no_service {
        start_watching(ctx)?;
    }
    Ok(())
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

fn start_watching(ctx: &InitContext) -> Result<(), VaultError> {
    if daemon::is_running() {
        return Ok(());
    }
    ctx.service.start()
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
            registry: Arc::new(crate::adapters::fakes::InMemoryRegistry::default()),
            service: recorder.clone(),
        };
        start_watching(&ctx).expect("start watching");
        assert_eq!(*recorder.starts.lock().expect("lock"), 0);
        std::env::remove_var(crate::paths::STATE_DIR_ENV);
    }

    #[test]
    fn noop_adapter_never_starts_anything() {
        let noop = NoopService;
        assert!(noop.start().is_ok());
        assert_eq!(noop.state(), crate::ports::ServiceState::Unsupported);
    }
}
