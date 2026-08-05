//! Singleton filesystem watcher for all registered vaults.

pub mod router;
pub mod worker;

use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use tokio::sync::{mpsc, watch};

pub use router::Router;

use crate::daemon;
use crate::error::VaultError;
use crate::paths::{ensure_state_dir, registry_path, REGISTRY_FILE};
use crate::registry::VaultRegistry;

/// Slow safety-net poll when notify misses a registry change.
const REGISTRY_POLL_MS: u64 = 5000;

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
    let mut debouncer = build_debouncer(Arc::clone(&state), reload_tx.clone())?;

    apply_watches(&state, &mut debouncer)?;
    {
        let mut guard = state.lock().expect("lock");
        guard.registry_mtime = registry_mtime();
    }

    run_event_loop(shutdown, &state, &mut debouncer, reload_rx).await
}

fn build_debouncer(
    state: Arc<Mutex<WatcherState>>,
    reload_tx: mpsc::UnboundedSender<()>,
) -> Result<Debouncer<notify::RecommendedWatcher, RecommendedCache>, VaultError> {
    let debounce_ms = min_debounce_ms(&state);
    let handle = tokio::runtime::Handle::current();
    new_debouncer(
        Duration::from_millis(debounce_ms),
        None,
        move |result: DebounceEventResult| {
            on_debounced_events(result, Arc::clone(&state), reload_tx.clone(), &handle);
        },
    )
    .map_err(VaultError::notify)
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
        if let Err(errors) = result {
            let _ = append_notify_error(&VaultError::notify(std::io::Error::other(format!(
                "{errors:?}"
            ))));
        }
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

async fn run_event_loop(
    mut shutdown: watch::Receiver<bool>,
    state: &Arc<Mutex<WatcherState>>,
    debouncer: &mut Debouncer<notify::RecommendedWatcher, RecommendedCache>,
    mut reload_rx: mpsc::UnboundedReceiver<()>,
) -> Result<(), VaultError> {
    loop {
        if *shutdown.borrow() {
            break;
        }

        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    break;
                }
            }
            Some(()) = reload_rx.recv() => {
                let _ = daemon::prune_registry();
                apply_watches(state, debouncer)?;
            }
            () = debouncer_tick() => {
                refresh_watches_if_registry_changed(state, debouncer)?;
            }
        }
    }

    Ok(())
}

fn refresh_watches_if_registry_changed(
    state: &Arc<Mutex<WatcherState>>,
    debouncer: &mut Debouncer<notify::RecommendedWatcher, RecommendedCache>,
) -> Result<(), VaultError> {
    let current = registry_mtime();
    let changed = {
        let mut guard = state.lock().expect("lock");
        let previous = guard.registry_mtime;
        if previous == current {
            false
        } else {
            guard.registry_mtime = current;
            true
        }
    };
    if changed {
        let _ = daemon::prune_registry();
        apply_watches(state, debouncer)?;
    }
    Ok(())
}

async fn process_paths(
    state: Arc<Mutex<WatcherState>>,
    abs_paths: Vec<PathBuf>,
    reload_tx: mpsc::UnboundedSender<()>,
) -> Result<(), VaultError> {
    signal_if_registry_related(&state, &abs_paths, &reload_tx);
    commit_routed_batches(&state, abs_paths).await?;
    signal_if_registry_changed(&state, &reload_tx);
    Ok(())
}

fn signal_if_registry_related(
    state: &Arc<Mutex<WatcherState>>,
    abs_paths: &[PathBuf],
    reload_tx: &mpsc::UnboundedSender<()>,
) {
    if !abs_paths.iter().any(|p| is_registry_related(p)) {
        return;
    }
    let mut guard = state.lock().expect("lock");
    guard.registry_mtime = None;
    drop(guard);
    let _ = reload_tx.send(());
}

async fn commit_routed_batches(
    state: &Arc<Mutex<WatcherState>>,
    abs_paths: Vec<PathBuf>,
) -> Result<(), VaultError> {
    let batches = {
        let guard = state.lock().expect("lock");
        guard.router.route(abs_paths)
    };
    for (vault, paths) in batches {
        tokio::task::spawn_blocking(move || worker::commit_batch(&vault, &paths))
            .await
            .map_err(|_| VaultError::TaskPanicked)??;
    }
    Ok(())
}

fn signal_if_registry_changed(
    state: &Arc<Mutex<WatcherState>>,
    reload_tx: &mpsc::UnboundedSender<()>,
) {
    let changed = {
        let current = registry_mtime();
        let mut guard = state.lock().expect("lock");
        let previous = guard.registry_mtime;
        guard.registry_mtime = current;
        previous != current
    };
    if !changed {
        return;
    }
    let _ = reload_tx.send(());
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
    tokio::time::sleep(Duration::from_millis(REGISTRY_POLL_MS)).await;
}

struct WatcherState {
    router: Router,
    watched_roots: HashSet<PathBuf>,
    registry_mtime: Option<SystemTime>,
    state_watches_registered: bool,
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
        })
    }

    fn refresh_router(&mut self) -> Result<(), VaultError> {
        let registry = VaultRegistry::load()?;
        self.router = Router::from_registry(&registry)?;
        Ok(())
    }

    fn roots_to_watch(&mut self) -> (Vec<PathBuf>, Vec<PathBuf>, bool) {
        let current: HashSet<_> = self.router.roots().into_iter().collect();
        let new_roots: Vec<_> = current
            .iter()
            .filter(|root| !self.watched_roots.contains(*root))
            .cloned()
            .collect();
        let removed: Vec<_> = self
            .watched_roots
            .iter()
            .filter(|root| !current.contains(*root))
            .cloned()
            .collect();
        for root in &new_roots {
            self.watched_roots.insert(root.clone());
        }
        for root in &removed {
            self.watched_roots.remove(root);
        }
        let register_state = !self.state_watches_registered;
        if register_state {
            self.state_watches_registered = true;
        }
        (new_roots, removed, register_state)
    }
}

fn apply_watches(
    state: &Arc<Mutex<WatcherState>>,
    debouncer: &mut Debouncer<notify::RecommendedWatcher, RecommendedCache>,
) -> Result<(), VaultError> {
    let (new_roots, removed, register_state) = {
        let mut guard = state.lock().expect("lock");
        guard.refresh_router()?;
        guard.registry_mtime = registry_mtime();
        guard.roots_to_watch()
    };

    if register_state {
        register_global_state_watches(debouncer)?;
    }
    for root in removed {
        let _ = debouncer.unwatch(&root);
    }
    watch_vault_roots(debouncer, new_roots)
}

fn register_global_state_watches(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, RecommendedCache>,
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
    debouncer: &mut Debouncer<notify::RecommendedWatcher, RecommendedCache>,
    roots: Vec<PathBuf>,
) -> Result<(), VaultError> {
    for root in roots {
        watch_path(debouncer, &root, RecursiveMode::Recursive)?;
    }
    Ok(())
}

fn watch_path(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, RecommendedCache>,
    path: &std::path::Path,
    mode: RecursiveMode,
) -> Result<(), VaultError> {
    debouncer.watch(path, mode).map_err(VaultError::notify)
}

fn registry_mtime() -> Option<SystemTime> {
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
        .is_some_and(|name| name == REGISTRY_FILE || name == crate::paths::REGISTRY_LOCK)
        && registry
            .parent()
            .is_some_and(|parent| path.starts_with(parent))
}
