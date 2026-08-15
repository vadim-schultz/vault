//! `vault reindex` use-case — rebuild `meta.db` by replaying `.git`'s commit history.

use std::path::PathBuf;

use crate::adapters::GixObjectStore;
use crate::domain::{
    missing_markers, parse_created_at, parse_single_verb, vault_state, FileChange, FileEventKind,
    HistoryCommit, SnapshotRecord, VaultLayout, VaultState,
};
use crate::error::VaultError;
use crate::paths::{resolve_init, GIT_DIR, META_DB};
use crate::ports::ObjectStore;
use crate::storage::sqlite::MetaDb;

/// Outcome of a `vault reindex` run (or preview, for `--dry-run`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReindexOutcome {
    /// Number of commits replayed.
    pub commits: usize,
    /// `(oldest, newest)` `created_at` among the replayed commits, `None` for an empty history.
    pub span: Option<(String, String)>,
    /// How many commits fell back to `.git`'s own committer time because their message didn't
    /// match vault's `"... @ <created_at>"` format.
    pub lossy_timestamps: usize,
    /// Snapshot rows the existing `meta.db` already had before this run.
    pub existing_snapshot_count: i64,
    /// Whether this was a dry run — `meta.db` was not written.
    pub dry_run: bool,
}

/// Whether [`rebuild`] may overwrite an existing, populated `meta.db`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    /// Refuse (with [`VaultError::MetaDbNotEmpty`]) when `meta.db` already has snapshot rows.
    Refuse,
    /// Rebuild regardless of what `meta.db` already holds — `--force` was passed.
    Force,
}

/// Resolve a vault path from CLI-style arguments, then report what [`rebuild`] would do there
/// without writing anything.
///
/// # Errors
///
/// Returns [`VaultError`] when path resolution or replaying history fails (see [`preview_at`]).
pub fn preview(vault_path: Option<PathBuf>) -> Result<(VaultLayout, ReindexOutcome), VaultError> {
    let (layout, _state) = resolve_init(vault_path)?;
    let outcome = preview_at(&layout)?;
    Ok((layout, outcome))
}

/// Resolve a vault path from CLI-style arguments, then rebuild `meta.db` there.
///
/// # Errors
///
/// Returns [`VaultError`] when path resolution or reindexing fails (see [`rebuild_at`]).
pub fn rebuild(
    vault_path: Option<PathBuf>,
    overwrite: Overwrite,
) -> Result<(VaultLayout, ReindexOutcome), VaultError> {
    let (layout, _state) = resolve_init(vault_path)?;
    let outcome = rebuild_at(&layout, overwrite)?;
    Ok((layout, outcome))
}

/// Report what [`rebuild_at`] would do, without writing `meta.db`.
///
/// # Errors
///
/// Returns [`VaultError::VaultNotFound`]/[`VaultError::PartialVault`] when `.git` is absent, or
/// [`VaultError::Git`]/[`VaultError::NonLinearHistory`]/[`VaultError::Sqlite`] when replaying
/// history fails.
fn preview_at(layout: &VaultLayout) -> Result<ReindexOutcome, VaultError> {
    require_git_present(layout)?;
    let existing_snapshot_count = count_existing_snapshots(layout)?;
    let built = replay_history(layout)?;
    let (commits, span, lossy_timestamps) = replay_stats(&built);
    Ok(ReindexOutcome {
        commits,
        span,
        lossy_timestamps,
        existing_snapshot_count,
        dry_run: true,
    })
}

/// Rebuild `meta.db` from `.git`'s commit history.
///
/// Safe and unconditional when the existing `meta.db` has zero snapshot rows (missing, or
/// present but empty) — nothing is lost by replacing it, the same way git regenerates a missing
/// pack `.idx` with no confirmation needed. Refuses to overwrite a populated `meta.db` unless
/// `overwrite` is [`Overwrite::Force`] — overwriting real content needs the same explicit opt-in
/// `git branch -f`/`checkout -f` use for "discard existing state, I know what I'm doing". `.git`
/// itself missing is always refused, forced or not: there is nothing to replay from.
///
/// `.git` must be confirmed present (via [`require_git_present`]) before a [`GixObjectStore`]
/// can even be opened, so — unlike `app::snapshot`/`app::restore`, which take an already-open
/// `&dyn ObjectStore` — this constructs its own, the same way `app::init`'s own bootstrapping
/// does (`GixObjectStore::init`/`open` in `src/app/init.rs`) for the same reason: there may be
/// nothing valid to hand in yet.
///
/// # Errors
///
/// Returns [`VaultError::VaultNotFound`]/[`VaultError::PartialVault`] when `.git` is absent,
/// [`VaultError::MetaDbNotEmpty`] when `meta.db` already has rows and `overwrite` is
/// [`Overwrite::Refuse`], or [`VaultError::Git`]/[`VaultError::NonLinearHistory`]/
/// [`VaultError::Sqlite`] when replaying history or writing the rebuilt index fails.
fn rebuild_at(layout: &VaultLayout, overwrite: Overwrite) -> Result<ReindexOutcome, VaultError> {
    require_git_present(layout)?;
    let existing_snapshot_count = count_existing_snapshots(layout)?;
    ensure_overwrite_allowed(layout, existing_snapshot_count, overwrite)?;
    let built = replay_history(layout)?;
    let (commits, span, lossy_timestamps) = replay_stats(&built);
    write_fresh_meta_db(layout, built.into_iter().map(|built| built.record))?;
    Ok(ReindexOutcome {
        commits,
        span,
        lossy_timestamps,
        existing_snapshot_count,
        dry_run: false,
    })
}

/// Require `.git` to be present, reusing the same marker vocabulary (and `PartialVault` error)
/// `vault init`'s repair path already uses, so a damaged `.vault/` reads consistently across both
/// commands. Unlike `vault init`, `reindex` doesn't care whether `README`/`config.toml` are
/// present — it only ever reads `.git`.
fn require_git_present(layout: &VaultLayout) -> Result<(), VaultError> {
    match vault_state(&layout.vault_dir) {
        VaultState::Absent => Err(VaultError::VaultNotFound {
            start: layout.worktree.clone(),
        }),
        VaultState::Ready => Ok(()),
        VaultState::Partial(present) if present.contains(&GIT_DIR) => Ok(()),
        VaultState::Partial(present) => Err(VaultError::PartialVault {
            path: layout.vault_dir.clone(),
            missing: missing_markers(&present).join(", "),
            found: present.join(", "),
        }),
    }
}

/// Snapshot rows the existing `meta.db` already has (0 when it's missing or freshly created).
fn count_existing_snapshots(layout: &VaultLayout) -> Result<i64, VaultError> {
    MetaDb::open(&layout.meta_db_path())?.snapshot_count()
}

/// Refuse overwriting a populated `meta.db` unless `overwrite` is [`Overwrite::Force`].
fn ensure_overwrite_allowed(
    layout: &VaultLayout,
    existing_snapshot_count: i64,
    overwrite: Overwrite,
) -> Result<(), VaultError> {
    if existing_snapshot_count == 0 || overwrite == Overwrite::Force {
        return Ok(());
    }
    Err(VaultError::MetaDbNotEmpty {
        path: layout.meta_db_path(),
        snapshot_count: existing_snapshot_count,
    })
}

/// Walk `.git`'s commit history and build one [`BuiltRecord`] per commit.
fn replay_history(layout: &VaultLayout) -> Result<Vec<BuiltRecord>, VaultError> {
    let object_store = GixObjectStore::open(layout)?;
    Ok(object_store.history()?.iter().map(build_record).collect())
}

/// Derive the `(commits, span, lossy_timestamps)` a replay's outcome reports.
fn replay_stats(built: &[BuiltRecord]) -> (usize, Option<(String, String)>, usize) {
    let lossy_timestamps = built.iter().filter(|r| r.lossy).count();
    (built.len(), span_of(built), lossy_timestamps)
}

struct BuiltRecord {
    record: SnapshotRecord,
    lossy: bool,
}

fn build_record(commit: &HistoryCommit) -> BuiltRecord {
    let (created_at, lossy) = match parse_created_at(&commit.message) {
        Some(at) => (at.to_string(), false),
        None => (commit.committer_time.clone(), true),
    };
    let changes = reclassify_restores(&commit.message, commit.changes.clone());
    BuiltRecord {
        record: SnapshotRecord {
            commit_sha: commit.sha.clone(),
            created_at,
            changes,
        },
        lossy,
    }
}

/// Promote a lone `Create`/`Modify` to `Restore` when the message says so — safe because
/// `app::restore::commit_restore` only ever produces a single-change commit, so there is no
/// batch case where this could misclassify one file among several changed in the same snapshot.
fn reclassify_restores(message: &str, mut changes: Vec<FileChange>) -> Vec<FileChange> {
    if let [only] = changes.as_mut_slice() {
        if parse_single_verb(message) == Some("restore")
            && matches!(only.kind, FileEventKind::Create | FileEventKind::Modify)
        {
            only.kind = FileEventKind::Restore;
        }
    }
    changes
}

fn span_of(built: &[BuiltRecord]) -> Option<(String, String)> {
    let oldest = built.first()?.record.created_at.clone();
    let newest = built.last()?.record.created_at.clone();
    Some((oldest, newest))
}

/// Build a fresh `meta.db` in a sibling temp file and `rename` it over the real path only once
/// every row is written — mirrors `git index-pack`'s own `.idx.temp` → rename pattern, so a
/// crash or interrupt mid-rebuild leaves whatever `meta.db` (or absence of one) existed before
/// untouched rather than a half-populated index.
fn write_fresh_meta_db(
    layout: &VaultLayout,
    records: impl Iterator<Item = SnapshotRecord>,
) -> Result<(), VaultError> {
    let target = layout.meta_db_path();
    let tmp = layout.vault_dir.join(format!("{META_DB}.reindex.tmp"));
    remove_sqlite_files(&tmp);
    {
        let db = MetaDb::open(&tmp)?;
        for record in records {
            db.insert_snapshot(&record)?;
        }
    }
    remove_sqlite_sidecars(&tmp);
    std::fs::rename(&tmp, &target)?;
    Ok(())
}

/// Remove a stale temp db (and its WAL/SHM sidecars) possibly left behind by an interrupted
/// previous run, before starting a fresh one.
fn remove_sqlite_files(tmp: &std::path::Path) {
    let _ = std::fs::remove_file(tmp);
    remove_sqlite_sidecars(tmp);
}

fn remove_sqlite_sidecars(db_path: &std::path::Path) {
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = db_path.as_os_str().to_owned();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(std::path::PathBuf::from(sidecar));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::adapters::fakes::FixedClock;
    use crate::adapters::SqliteMetaIndex;
    use crate::config::VaultConfig;
    use crate::domain::RelPath;
    use crate::ports::MetaIndex;
    use crate::storage;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn init_vault(dir: &TempDir) -> VaultLayout {
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git init");
        storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite init");
        VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("write config");
        fs::write(layout.readme_path(), b"test").expect("readme");
        layout
    }

    fn clock_at(hour: u32) -> FixedClock {
        FixedClock::at(
            chrono::Utc
                .with_ymd_and_hms(2026, 6, 1, hour, 0, 0)
                .unwrap(),
        )
    }

    fn commit_change(
        layout: &VaultLayout,
        object_store: &GixObjectStore,
        meta_index: &SqliteMetaIndex,
        hour: u32,
        path: &str,
        content: &[u8],
        kind: FileEventKind,
    ) {
        if kind == FileEventKind::Delete {
            fs::remove_file(layout.worktree.join(path)).expect("remove");
        } else {
            fs::write(layout.worktree.join(path), content).expect("write");
        }
        let changes = vec![FileChange {
            rel: RelPath::parse(path),
            kind,
        }];
        crate::app::snapshot::commit(layout, &changes, &clock_at(hour), object_store, meta_index)
            .expect("commit")
            .expect("sha");
    }

    fn delete_meta_db(layout: &VaultLayout) {
        fs::remove_file(layout.meta_db_path()).expect("remove meta.db");
    }

    #[test]
    fn rebuilds_from_missing_meta_db_without_force() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let object_store = GixObjectStore::open(&layout).expect("git");
        {
            let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("meta");
            commit_change(
                &layout,
                &object_store,
                &meta_index,
                1,
                "a.md",
                b"v1",
                FileEventKind::Create,
            );
            commit_change(
                &layout,
                &object_store,
                &meta_index,
                2,
                "a.md",
                b"v2",
                FileEventKind::Modify,
            );
            commit_change(
                &layout,
                &object_store,
                &meta_index,
                3,
                "a.md",
                b"",
                FileEventKind::Delete,
            );
        }
        delete_meta_db(&layout);

        let outcome = rebuild_at(&layout, Overwrite::Refuse).expect("reindex");
        assert_eq!(outcome.commits, 3);
        assert_eq!(outcome.lossy_timestamps, 0);
        assert_eq!(outcome.existing_snapshot_count, 0);
        assert!(!outcome.dry_run);
        assert_eq!(
            outcome.span,
            Some((
                "2026-06-01T01:00:00+00:00".to_string(),
                "2026-06-01T03:00:00+00:00".to_string()
            ))
        );

        let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("reopen");
        assert_eq!(meta_index.list_snapshots(None).expect("list").len(), 3);
        assert_eq!(meta_index.list_tracked_files().expect("tracked"), vec![]);
    }

    #[test]
    fn refuses_populated_meta_db_without_force() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let object_store = GixObjectStore::open(&layout).expect("git");
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("meta");
        commit_change(
            &layout,
            &object_store,
            &meta_index,
            1,
            "a.md",
            b"v1",
            FileEventKind::Create,
        );

        let err = rebuild_at(&layout, Overwrite::Refuse).expect_err("should refuse");
        assert!(matches!(
            err,
            VaultError::MetaDbNotEmpty {
                snapshot_count: 1,
                ..
            }
        ));
    }

    #[test]
    fn force_overwrites_populated_meta_db() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let object_store = GixObjectStore::open(&layout).expect("git");
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("meta");
        commit_change(
            &layout,
            &object_store,
            &meta_index,
            1,
            "a.md",
            b"v1",
            FileEventKind::Create,
        );
        commit_change(
            &layout,
            &object_store,
            &meta_index,
            2,
            "b.md",
            b"v1",
            FileEventKind::Create,
        );

        let outcome = rebuild_at(&layout, Overwrite::Force).expect("force reindex");
        assert_eq!(outcome.commits, 2);
        assert_eq!(outcome.existing_snapshot_count, 2);
    }

    #[test]
    fn dry_run_previews_without_writing() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let object_store = GixObjectStore::open(&layout).expect("git");
        let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("meta");
        commit_change(
            &layout,
            &object_store,
            &meta_index,
            1,
            "a.md",
            b"v1",
            FileEventKind::Create,
        );
        drop(meta_index);
        let before = fs::read(layout.meta_db_path()).expect("read before");

        let outcome = preview_at(&layout).expect("dry run");
        assert_eq!(outcome.commits, 1);
        assert_eq!(outcome.existing_snapshot_count, 1);
        assert!(outcome.dry_run);

        let after = fs::read(layout.meta_db_path()).expect("read after");
        assert_eq!(before, after);
        assert!(!layout
            .vault_dir
            .join(format!("{META_DB}.reindex.tmp"))
            .exists());
    }

    #[test]
    fn refuses_when_git_is_missing() {
        let dir = TempDir::new().expect("tempdir");
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("write config");
        fs::write(layout.readme_path(), b"test").expect("readme");

        let err = rebuild_at(&layout, Overwrite::Refuse).expect_err("should refuse");
        assert!(matches!(err, VaultError::PartialVault { .. }));
    }

    #[test]
    fn restores_the_restore_classification() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let object_store = GixObjectStore::open(&layout).expect("git");
        {
            let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("meta");
            commit_change(
                &layout,
                &object_store,
                &meta_index,
                1,
                "a.md",
                b"v1",
                FileEventKind::Create,
            );
            commit_change(
                &layout,
                &object_store,
                &meta_index,
                2,
                "a.md",
                b"",
                FileEventKind::Delete,
            );
            let changes = vec![FileChange {
                rel: RelPath::parse("a.md"),
                kind: FileEventKind::Restore,
            }];
            fs::write(layout.worktree.join("a.md"), b"v1").expect("write restore");
            crate::app::snapshot::commit(
                &layout,
                &changes,
                &clock_at(3),
                &object_store,
                &meta_index,
            )
            .expect("commit")
            .expect("sha");
        }
        delete_meta_db(&layout);

        rebuild_at(&layout, Overwrite::Refuse).expect("reindex");

        let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("reopen");
        let history = meta_index
            .list_snapshots(Some(&RelPath::parse("a.md")))
            .expect("list");
        assert_eq!(history[0].event, Some(FileEventKind::Restore));
    }

    #[test]
    fn falls_back_to_committer_time_for_a_non_vault_commit_message() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let object_store = GixObjectStore::open(&layout).expect("git");
        fs::write(layout.worktree.join("a.md"), b"v1").expect("write");
        object_store
            .commit(
                &[FileChange {
                    rel: RelPath::parse("a.md"),
                    kind: FileEventKind::Create,
                }],
                "hand-authored, no timestamp suffix",
            )
            .expect("commit")
            .expect("sha");
        delete_meta_db(&layout);

        let outcome = rebuild_at(&layout, Overwrite::Refuse).expect("reindex");
        assert_eq!(outcome.commits, 1);
        assert_eq!(outcome.lossy_timestamps, 1);
    }
}
