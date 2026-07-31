//! Singleton filesystem watcher for all registered vaults.

pub mod router;
pub mod worker;

use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, NoCache};
use tokio::sync::{mpsc, watch};

pub use router::Router;

use crate::error::VaultError;
use crate::paths::{ensure_state_dir, registry_path};
use crate::registry::VaultRegistry;

/// Run the watcher until `shutdown` becomes true.
///
/// # Panics
///
/// Panics if internal mutexes are poisoned.
///
/// # Errors
///
/// Returns [`VaultError`] when notify setup fails.
pub async fn run(shutdown: watch::Receiver<bool>) -> Result<(), VaultError> {
    let state = Arc::new(Mutex::new(WatcherState::new()?));
    let (reload_tx, reload_rx) = mpsc::unbounded_channel();
    let mut debouncer = build_debouncer(Arc::clone(&state), reload_tx)?;

    apply_watches(&state, &mut debouncer)?;
    seed_registry_mtime();

    run_event_loop(shutdown, &state, &mut debouncer, reload_rx).await
}

fn build_debouncer(
    state: Arc<Mutex<WatcherState>>,
    reload_tx: mpsc::UnboundedSender<()>,
) -> Result<Debouncer<notify::RecommendedWatcher, NoCache>, VaultError> {
    let debounce_ms = min_debounce_ms(&state);
    let handle = tokio::runtime::Handle::current();
    new_debouncer(
        Duration::from_millis(debounce_ms),
        None,
        move |result: DebounceEventResult| {
            on_debounced_events(result, Arc::clone(&state), reload_tx.clone(), &handle);
        },
    )
    .map_err(|e| VaultError::Notify(e.to_string()))
}

fn min_debounce_ms(state: &Arc<Mutex<WatcherState>>) -> u64 {
    let guard = state.lock().expect("lock");
    guard.router.min_debounce_ms()
}

fn on_debounced_events(
    result: DebounceEventResult,
    state: Arc<Mutex<WatcherState>>,
    reload_tx: mpsc::UnboundedSender<()>,
    handle: &tokio::runtime::Handle,
) {
    let Ok(events) = result else {
        return;
    };
    let abs_paths = external_paths_from_events(&events);
    if abs_paths.is_empty() {
        return;
    }
    handle.spawn(async move {
        if let Err(err) = process_paths(state, abs_paths, reload_tx).await {
            let _ = append_notify_error(&err);
        }
    });
}

fn external_paths_from_events(events: &[notify_debouncer_full::DebouncedEvent]) -> Vec<PathBuf> {
    events
        .iter()
        .flat_map(|event| event.paths.clone())
        .filter(|path| !is_vault_internal_path(path))
        .collect()
}

fn seed_registry_mtime() {
    REGISTRY_MTIME.with(|cell| cell.set(registry_mtime()));
}

async fn run_event_loop(
    mut shutdown: watch::Receiver<bool>,
    state: &Arc<Mutex<WatcherState>>,
    debouncer: &mut Debouncer<notify::RecommendedWatcher, NoCache>,
    mut reload_rx: mpsc::UnboundedReceiver<()>,
) -> Result<(), VaultError> {
    loop {
        if shutdown_requested(&shutdown) {
            break;
        }

        tokio::select! {
            changed = shutdown.changed() => {
                if shutdown_signal_received(&changed, &shutdown) {
                    break;
                }
            }
            Some(()) = reload_rx.recv() => {
                apply_watches(state, debouncer)?;
            }
            () = debouncer_tick() => {
                refresh_watches_if_registry_changed(state, debouncer)?;
            }
        }
    }

    Ok(())
}

fn shutdown_requested(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow()
}

fn shutdown_signal_received(
    changed: &Result<(), watch::error::RecvError>,
    shutdown: &watch::Receiver<bool>,
) -> bool {
    changed.is_ok() && shutdown_requested(shutdown)
}

fn refresh_watches_if_registry_changed(
    state: &Arc<Mutex<WatcherState>>,
    debouncer: &mut Debouncer<notify::RecommendedWatcher, NoCache>,
) -> Result<(), VaultError> {
    if registry_changed() {
        apply_watches(state, debouncer)?;
    }
    Ok(())
}

async fn process_paths(
    state: Arc<Mutex<WatcherState>>,
    abs_paths: Vec<PathBuf>,
    reload_tx: mpsc::UnboundedSender<()>,
) -> Result<(), VaultError> {
    if abs_paths.iter().any(|p| is_registry_related(p)) {
        REGISTRY_MTIME.with(|cell| cell.set(None));
        let _ = reload_tx.send(());
    }
    let batches = {
        let mut guard = state.lock().expect("lock");
        guard.queue_events(abs_paths);
        guard.take_pending_batches()
    };
    for (vault, paths) in batches {
        tokio::task::spawn_blocking(move || worker::commit_batch(&vault, &paths))
            .await
            .map_err(|err| VaultError::Notify(err.to_string()))??;
    }
    if registry_changed() {
        let _ = reload_tx.send(());
    }
    Ok(())
}

fn append_notify_error(err: &VaultError) -> Result<(), VaultError> {
    let path = crate::paths::daemon_log_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = format!("{} notify error: {err}\n", chrono::Utc::now().to_rfc3339());
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())?;
    Ok(())
}

async fn debouncer_tick() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

struct WatcherState {
    router: Router,
    watched_roots: HashSet<PathBuf>,
    registry_mtime: Option<std::time::SystemTime>,
    state_watches_registered: bool,
    pending: Vec<(crate::watcher::router::WatchedVault, Vec<PathBuf>)>,
}

impl WatcherState {
    fn new() -> Result<Self, VaultError> {
        let registry = VaultRegistry::load()?;
        let router = Router::from_registry(&registry)?;
        Ok(Self {
            router,
            watched_roots: HashSet::new(),
            registry_mtime: registry_mtime(),
            state_watches_registered: false,
            pending: Vec::new(),
        })
    }

    fn refresh_router(&mut self) -> Result<(), VaultError> {
        let registry = VaultRegistry::load()?;
        self.router = Router::from_registry(&registry)?;
        self.registry_mtime = registry_mtime();
        Ok(())
    }

    fn new_roots(&mut self) -> Vec<PathBuf> {
        self.router
            .roots()
            .into_iter()
            .filter(|root| self.watched_roots.insert(root.clone()))
            .collect()
    }

    fn take_pending_batches(
        &mut self,
    ) -> Vec<(crate::watcher::router::WatchedVault, Vec<PathBuf>)> {
        std::mem::take(&mut self.pending)
    }

    fn queue_events(&mut self, abs_paths: Vec<PathBuf>) {
        let rel_events: Vec<(PathBuf, PathBuf)> = abs_paths
            .into_iter()
            .filter_map(|abs| {
                let vault = self.router.vault_for(&abs)?;
                let rel = abs.strip_prefix(&vault.root).ok()?.to_path_buf();
                Some((abs, rel))
            })
            .collect();
        let grouped = self.router.group_paths(&rel_events);
        for (root, paths) in grouped {
            let Some(vault) = self.router.vault_by_root(&root) else {
                continue;
            };
            self.pending.push((vault.clone(), paths));
        }
    }

    fn plan_watch_refresh(&mut self) -> Result<(Vec<PathBuf>, bool), VaultError> {
        self.refresh_router()?;
        let new_roots = self.new_roots();
        let register_state_watches = !self.state_watches_registered;
        if register_state_watches {
            self.state_watches_registered = true;
        }
        Ok((new_roots, register_state_watches))
    }
}

fn apply_watches(
    state: &Arc<Mutex<WatcherState>>,
    debouncer: &mut Debouncer<notify::RecommendedWatcher, NoCache>,
) -> Result<(), VaultError> {
    let (new_roots, register_state_watches) = {
        let mut guard = state.lock().expect("lock");
        guard.plan_watch_refresh()?
    };

    if register_state_watches {
        register_global_state_watches(debouncer)?;
    }
    watch_vault_roots(debouncer, new_roots)
}

fn register_global_state_watches(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, NoCache>,
) -> Result<(), VaultError> {
    let state_dir_path = ensure_state_dir()?;
    let registry_file = registry_path()?;
    watch_path(debouncer, &state_dir_path, RecursiveMode::NonRecursive)?;
    if registry_file.is_file() {
        watch_path(debouncer, &registry_file, RecursiveMode::NonRecursive)?;
    }
    Ok(())
}

fn watch_vault_roots(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, NoCache>,
    roots: Vec<PathBuf>,
) -> Result<(), VaultError> {
    for root in roots {
        watch_path(debouncer, &root, RecursiveMode::Recursive)?;
    }
    Ok(())
}

fn watch_path(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, NoCache>,
    path: &std::path::Path,
    mode: RecursiveMode,
) -> Result<(), VaultError> {
    debouncer
        .watch(path, mode)
        .map_err(|e| VaultError::Notify(e.to_string()))
}

fn registry_changed() -> bool {
    let current = registry_mtime();
    REGISTRY_MTIME.with(|cell| {
        let previous = cell.get();
        if previous == current {
            false
        } else {
            cell.set(current);
            true
        }
    })
}

std::thread_local! {
    static REGISTRY_MTIME: std::cell::Cell<Option<std::time::SystemTime>> =
        const { std::cell::Cell::new(None) };
}

fn registry_mtime() -> Option<std::time::SystemTime> {
    registry_path()
        .ok()
        .and_then(|p| p.metadata().ok())
        .and_then(|m| m.modified().ok())
}

fn is_vault_internal_path(path: &std::path::Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == crate::paths::VAULT_DIR)
}

fn is_registry_related(path: &std::path::Path) -> bool {
    let Ok(registry) = registry_path() else {
        return false;
    };
    if path == registry {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("registry"))
        && registry
            .parent()
            .is_some_and(|parent| path.starts_with(parent))
}

#[cfg(test)]
pub async fn run_until_shutdown(shutdown: watch::Receiver<bool>) -> Result<(), VaultError> {
    run(shutdown).await
}
