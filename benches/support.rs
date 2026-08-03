//! Shared fixtures for criterion benches. Bench-only — not part of the published crate.
//!
//! Compiled fresh into each bench binary via `#[path = "support.rs"] mod support;`, so any
//! single binary only uses a subset of what's here — unused-item warnings are expected noise.
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::OnceLock;

use chrono::{TimeZone, Utc};
use tempfile::TempDir;

use vault::adapters::{GixObjectStore, SqliteMetaIndex};
use vault::domain::{CommitSha, FileChange, FileEventKind, RelPath, SnapshotRecord, VaultLayout};
use vault::ports::{MetaIndex, ObjectStore};
use vault::registry::{VaultEntry, VaultRegistry};
use vault::storage;

/// A disposable, fully-initialized vault on disk. `dir` must outlive any use of `layout`.
pub struct BenchVault {
    pub dir: TempDir,
    pub layout: VaultLayout,
}

/// Create an empty but fully-initialized vault (git + sqlite + config + readme) in a tempdir.
pub fn new_vault() -> BenchVault {
    let dir = TempDir::new().expect("tempdir");
    let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
    std::fs::create_dir_all(&layout.vault_dir).expect("mkdir vault_dir");
    storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git init");
    storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite init");
    vault::config::VaultConfig::defaults()
        .write_to(&layout.config_path())
        .expect("write config");
    std::fs::write(layout.readme_path(), b"bench vault").expect("write readme");
    BenchVault { dir, layout }
}

/// Deterministic, monotonically increasing RFC3339 timestamp, `i` seconds after a fixed epoch.
#[must_use]
#[allow(clippy::cast_possible_wrap)] // bench sizes never approach i64::MAX seconds
pub fn ts(i: u64) -> String {
    Utc.timestamp_opt(1_700_000_000 + i as i64, 0)
        .unwrap()
        .to_rfc3339()
}

/// Seed `meta.db` with `n` snapshot rows, touching one of `path_pool` distinct paths per
/// snapshot round-robin (first touch is a `Create`, later ones are `Modify`).
///
/// # Panics
///
/// Panics if a snapshot fails to record.
pub fn seed_snapshots(index: &SqliteMetaIndex, n: usize, path_pool: usize) {
    let mut seen = HashSet::with_capacity(path_pool);
    for i in 0..n {
        let path = format!("file-{:04}.md", i % path_pool);
        let kind = if seen.insert(path.clone()) {
            FileEventKind::Create
        } else {
            FileEventKind::Modify
        };
        let record = SnapshotRecord {
            commit_sha: CommitSha(format!("{i:040x}")),
            created_at: ts(i as u64),
            changes: vec![FileChange {
                rel: RelPath::parse(&path),
                kind,
            }],
        };
        index.record_snapshot(&record).expect("seed snapshot");
    }
}

/// Write `n` small files under `vault.layout.worktree` and commit them as one snapshot,
/// mirroring what `vault init`'s baseline snapshot does. Returns the object store so callers
/// can keep committing against the same tree.
///
/// # Panics
///
/// Panics if writing a seed file or committing fails.
#[must_use]
pub fn seed_tracked_files(vault: &BenchVault, n: usize) -> GixObjectStore {
    let store = GixObjectStore::open(&vault.layout).expect("git store open");
    let changes: Vec<FileChange> = (0..n)
        .map(|i| {
            let rel = format!("doc-{i:06}.md");
            std::fs::write(vault.layout.worktree.join(&rel), b"seed content")
                .expect("write seed file");
            FileChange {
                rel: RelPath::parse(&rel),
                kind: FileEventKind::Create,
            }
        })
        .collect();
    store
        .commit(&changes, "bench: seed tracked files")
        .expect("seed commit");
    store
}

/// Register `n` vaults in a registry, without creating real vault directories on disk.
/// Suitable for benchmarking pure `registry.toml` load/save cost.
#[must_use]
pub fn synthetic_registry(n: usize) -> VaultRegistry {
    let mut registry = VaultRegistry::default();
    for i in 0..n {
        registry.vault.push(VaultEntry {
            root: std::path::PathBuf::from(format!("/synthetic/vault-{i:06}")),
            registered_at: Utc::now(),
            enabled: true,
        });
    }
    registry
}

static BENCH_STATE_DIR: OnceLock<TempDir> = OnceLock::new();

/// Redirect `VAULT_STATE_DIR` to an isolated tempdir so registry benches never read or write
/// the real per-user `registry.toml`. Idempotent — safe to call from every bench function.
///
/// # Panics
///
/// Panics if the isolated state tempdir cannot be created.
pub fn isolate_state_dir() {
    let dir = BENCH_STATE_DIR.get_or_init(|| TempDir::new().expect("state tempdir"));
    std::env::set_var(vault::paths::STATE_DIR_ENV, dir.path());
}

/// Create `n` real, fully-initialized vaults on disk and a registry pointing at them.
/// Slower than [`synthetic_registry`] — only real vaults load through `Router::from_registry`.
#[must_use]
pub fn real_registry(n: usize) -> (Vec<BenchVault>, VaultRegistry) {
    let mut vaults = Vec::with_capacity(n);
    let mut registry = VaultRegistry::default();
    for _ in 0..n {
        let vault = new_vault();
        registry.vault.push(VaultEntry {
            root: vault.layout.worktree.clone(),
            registered_at: Utc::now(),
            enabled: true,
        });
        vaults.push(vault);
    }
    (vaults, registry)
}
