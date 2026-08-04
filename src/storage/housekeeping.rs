//! Conditional git repack housekeeping (loose-object and pack thresholds).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use git2::{Oid, Repository};
use serde::{Deserialize, Serialize};

use crate::config::GcConfig;
use crate::domain::VaultLayout;
use crate::error::VaultError;
use crate::paths::HOUSEKEEPING_FILE;

/// Live object-store shape from a cheap directory scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectCounts {
    /// Number of loose object files under `objects/`.
    pub loose: usize,
    /// Number of `.pack` files under `objects/pack/`.
    pub packs: usize,
}

/// Persisted record of the last repack run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepackRecord {
    /// When the repack finished (UTC RFC3339).
    pub ran_at: String,
    /// Objects written into the new pack.
    pub objects_packed: usize,
    /// Loose object files removed after verification.
    pub loose_removed: usize,
    /// Total bytes under `.git/` before the repack.
    pub bytes_before: u64,
    /// Total bytes under `.git/` after cleanup.
    pub bytes_after: u64,
}

/// Persisted housekeeping marker in `.vault/housekeeping.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HousekeepingMarker {
    /// When counts were last checked.
    pub checked_at: String,
    /// Live counts at `checked_at`.
    pub counts: ObjectCounts,
    /// Last repack outcome, if any.
    pub last_repack: Option<RepackRecord>,
}

/// Outcome of a repack run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepackOutcome {
    /// When the repack finished.
    pub ran_at: DateTime<Utc>,
    /// Objects written into the new pack.
    pub objects_packed: usize,
    /// Loose object files removed after verification.
    pub loose_removed: usize,
    /// Total bytes under `.git/` before the repack.
    pub bytes_before: u64,
    /// Total bytes under `.git/` after cleanup.
    pub bytes_after: u64,
}

/// Status returned by [`maybe_run`] and surfaced in `vault status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HousekeepingStatus {
    /// Current loose/pack counts.
    pub counts: ObjectCounts,
    /// Last repack record from the marker, if any.
    pub last_repack: Option<RepackRecord>,
    /// Whether a repack ran during this check.
    pub repack_ran: bool,
}

/// Count loose objects and packfiles under `git_dir`.
///
/// # Errors
///
/// Returns [`VaultError::Io`] when the object directories cannot be read.
pub fn count_objects(git_dir: &Path) -> Result<ObjectCounts, VaultError> {
    Ok(ObjectCounts {
        loose: count_loose_objects(&objects_dir(git_dir))?,
        packs: count_pack_files(&pack_dir(git_dir))?,
    })
}

/// Return whether housekeeping thresholds are exceeded.
#[must_use]
pub fn is_due(
    counts: ObjectCounts,
    last_repack_at: Option<DateTime<Utc>>,
    thresholds: &GcConfig,
    now: DateTime<Utc>,
) -> bool {
    if counts.loose > thresholds.loose_object_limit {
        return true;
    }
    if counts.packs > thresholds.pack_limit {
        return true;
    }
    let Some(last) = last_repack_at else {
        return false;
    };
    let max_age =
        chrono::Duration::seconds(i64::try_from(thresholds.max_age_secs).unwrap_or(i64::MAX));
    now.signed_duration_since(last) > max_age
}

/// Repack reachable objects from `HEAD` into a new pack and remove redundant loose files.
///
/// # Errors
///
/// Returns [`VaultError`] when the repository cannot be opened, packed, or cleaned up.
pub fn repack(git_dir: &Path) -> Result<RepackOutcome, VaultError> {
    let prep = repack_prep(git_dir)?;
    let objects_packed = write_pack_from_head(git_dir)?;
    verify_loose_objects_packed(git_dir, &prep.loose_oids)?;
    cleanup_superseded_storage(git_dir, &prep.old_packs)?;
    Ok(repack_outcome(&prep, objects_packed, dir_size(git_dir)?))
}

/// Run housekeeping when thresholds are exceeded; always refresh the marker.
///
/// # Errors
///
/// Returns [`VaultError`] when counts, repack, or marker I/O fail.
pub fn maybe_run(
    layout: &VaultLayout,
    gc_config: &GcConfig,
) -> Result<HousekeepingStatus, VaultError> {
    let check = housekeeping_check(layout)?;
    if !is_due(check.counts, check.last_repack_at, gc_config, check.now) {
        return finish_without_repack(layout, &check);
    }
    finish_with_repack(layout, &check)
}

/// Read the housekeeping marker, returning defaults when absent.
///
/// # Errors
///
/// Returns [`VaultError`] when the marker file exists but cannot be parsed.
pub fn read_marker(vault_dir: &Path) -> Result<HousekeepingMarker, VaultError> {
    let path = vault_dir.join(HOUSEKEEPING_FILE);
    if !path.is_file() {
        return Ok(default_marker());
    }
    let contents = fs::read_to_string(path)?;
    let marker: HousekeepingMarker = serde_json::from_str(&contents)?;
    Ok(marker)
}

/// Collect live counts and marker history for `vault status`.
///
/// # Errors
///
/// Returns [`VaultError`] when object counts or the marker cannot be read.
pub fn status_for(layout: &VaultLayout) -> Result<HousekeepingStatus, VaultError> {
    let counts = count_objects(&layout.git_dir_path())?;
    let marker = read_marker(&layout.vault_dir)?;
    Ok(HousekeepingStatus {
        counts,
        last_repack: marker.last_repack,
        repack_ran: false,
    })
}

fn default_marker() -> HousekeepingMarker {
    HousekeepingMarker {
        checked_at: String::new(),
        counts: ObjectCounts { loose: 0, packs: 0 },
        last_repack: None,
    }
}

fn repack_record_from_outcome(outcome: &RepackOutcome) -> RepackRecord {
    RepackRecord {
        ran_at: outcome.ran_at.to_rfc3339(),
        objects_packed: outcome.objects_packed,
        loose_removed: outcome.loose_removed,
        bytes_before: outcome.bytes_before,
        bytes_after: outcome.bytes_after,
    }
}

struct HousekeepingCheck {
    git_dir: PathBuf,
    counts: ObjectCounts,
    marker: HousekeepingMarker,
    last_repack_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
}

fn housekeeping_check(layout: &VaultLayout) -> Result<HousekeepingCheck, VaultError> {
    let git_dir = layout.git_dir_path();
    let counts = count_objects(&git_dir)?;
    let marker = read_marker(&layout.vault_dir)?;
    Ok(HousekeepingCheck {
        git_dir,
        counts,
        last_repack_at: last_repack_at(&marker),
        marker,
        now: Utc::now(),
    })
}

fn last_repack_at(marker: &HousekeepingMarker) -> Option<DateTime<Utc>> {
    marker
        .last_repack
        .as_ref()
        .and_then(|record| DateTime::parse_from_rfc3339(&record.ran_at).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn finish_without_repack(
    layout: &VaultLayout,
    check: &HousekeepingCheck,
) -> Result<HousekeepingStatus, VaultError> {
    let last_repack = check.marker.last_repack.clone();
    persist_marker(layout, check, check.counts, last_repack.clone())?;
    Ok(HousekeepingStatus {
        counts: check.counts,
        last_repack,
        repack_ran: false,
    })
}

fn finish_with_repack(
    layout: &VaultLayout,
    check: &HousekeepingCheck,
) -> Result<HousekeepingStatus, VaultError> {
    let outcome = repack(&check.git_dir)?;
    let counts = count_objects(&check.git_dir)?;
    let record = repack_record_from_outcome(&outcome);
    persist_marker(layout, check, counts, Some(record.clone()))?;
    Ok(HousekeepingStatus {
        counts,
        last_repack: Some(record),
        repack_ran: true,
    })
}

fn persist_marker(
    layout: &VaultLayout,
    check: &HousekeepingCheck,
    counts: ObjectCounts,
    last_repack: Option<RepackRecord>,
) -> Result<(), VaultError> {
    write_marker(
        &layout.vault_dir,
        &HousekeepingMarker {
            checked_at: check.now.to_rfc3339(),
            counts,
            last_repack,
        },
    )
}

fn write_marker(vault_dir: &Path, marker: &HousekeepingMarker) -> Result<(), VaultError> {
    let path = vault_dir.join(HOUSEKEEPING_FILE);
    let contents = serde_json::to_string_pretty(marker)?;
    fs::write(path, contents)?;
    Ok(())
}

fn objects_dir(git_dir: &Path) -> PathBuf {
    git_dir.join("objects")
}

fn pack_dir(git_dir: &Path) -> PathBuf {
    objects_dir(git_dir).join("pack")
}

struct RepackPrep {
    bytes_before: u64,
    loose_oids: Vec<Oid>,
    old_packs: HashSet<PathBuf>,
}

fn repack_prep(git_dir: &Path) -> Result<RepackPrep, VaultError> {
    Ok(RepackPrep {
        bytes_before: dir_size(git_dir)?,
        loose_oids: collect_loose_oids(git_dir)?,
        old_packs: list_pack_files(&pack_dir(git_dir))?,
    })
}

fn open_bare(git_dir: &Path) -> Result<Repository, VaultError> {
    Repository::open_bare(git_dir).map_err(VaultError::git)
}

fn head_oid(repo: &Repository) -> Result<Oid, VaultError> {
    repo.head()
        .map_err(VaultError::git)?
        .target()
        .ok_or_else(|| VaultError::git(std::io::Error::other("HEAD has no oid")))
}

fn write_pack_from_head(git_dir: &Path) -> Result<usize, VaultError> {
    let repo = open_bare(git_dir)?;
    let mut revwalk = repo.revwalk().map_err(VaultError::git)?;
    revwalk.push(head_oid(&repo)?).map_err(VaultError::git)?;

    let mut packbuilder = repo.packbuilder().map_err(VaultError::git)?;
    packbuilder
        .insert_walk(&mut revwalk)
        .map_err(VaultError::git)?;
    let objects_packed = packbuilder.object_count();

    let output = pack_dir(git_dir);
    fs::create_dir_all(&output)?;
    packbuilder
        .write(output.as_path(), 0o644)
        .map_err(VaultError::git)?;
    Ok(objects_packed)
}

fn verify_loose_objects_packed(git_dir: &Path, loose_oids: &[Oid]) -> Result<(), VaultError> {
    let repo = open_bare(git_dir)?;
    for oid in loose_oids {
        repo.find_object(*oid, None).map_err(VaultError::git)?;
    }
    Ok(())
}

fn cleanup_superseded_storage(
    git_dir: &Path,
    old_packs: &HashSet<PathBuf>,
) -> Result<(), VaultError> {
    remove_loose_objects(git_dir)?;
    remove_pack_files(old_packs)
}

fn repack_outcome(prep: &RepackPrep, objects_packed: usize, bytes_after: u64) -> RepackOutcome {
    RepackOutcome {
        ran_at: Utc::now(),
        objects_packed,
        loose_removed: prep.loose_oids.len(),
        bytes_before: prep.bytes_before,
        bytes_after,
    }
}

fn count_loose_objects(objects: &Path) -> Result<usize, VaultError> {
    let mut loose = 0;
    for_each_loose_object(objects, |_, _| {
        loose += 1;
        Ok(())
    })?;
    Ok(loose)
}

fn is_loose_shard_name(name: &str) -> bool {
    name.len() == 2 && name.chars().all(|c| c.is_ascii_hexdigit())
}

fn for_each_loose_object<F>(objects: &Path, mut visit: F) -> Result<(), VaultError>
where
    F: FnMut(&str, &Path) -> Result<(), VaultError>,
{
    if !objects.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(objects)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let prefix = entry.file_name().to_string_lossy().into_owned();
        if !is_loose_shard_name(&prefix) {
            continue;
        }
        for obj in fs::read_dir(entry.path())? {
            let obj = obj?;
            if !obj.file_type()?.is_file() {
                continue;
            }
            visit(&prefix, &obj.path())?;
        }
    }
    Ok(())
}

fn count_pack_files(pack: &Path) -> Result<usize, VaultError> {
    if !pack.is_dir() {
        return Ok(0);
    }
    let mut packs = 0;
    for entry in fs::read_dir(pack)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && is_pack_file(&entry.path()) {
            packs += 1;
        }
    }
    Ok(packs)
}

fn is_pack_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pack"))
}

fn dir_size(path: &Path) -> Result<u64, VaultError> {
    let mut total = 0;
    if !path.is_dir() {
        return Ok(0);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            total += dir_size(&entry.path())?;
        } else {
            total += meta.len();
        }
    }
    Ok(total)
}

fn collect_loose_oids(git_dir: &Path) -> Result<Vec<Oid>, VaultError> {
    let mut oids = Vec::new();
    for_each_loose_object(&objects_dir(git_dir), |prefix, path| {
        let suffix = path
            .file_name()
            .ok_or_else(|| {
                VaultError::git(std::io::Error::other("loose object path has no filename"))
            })?
            .to_string_lossy();
        let hex = format!("{prefix}{suffix}");
        oids.push(Oid::from_str(&hex).map_err(VaultError::git)?);
        Ok(())
    })?;
    Ok(oids)
}

fn remove_loose_objects(git_dir: &Path) -> Result<(), VaultError> {
    for_each_loose_object(&objects_dir(git_dir), |_, path| {
        fs::remove_file(path)?;
        Ok(())
    })
}

fn list_pack_files(pack: &Path) -> Result<HashSet<PathBuf>, VaultError> {
    let mut files = HashSet::new();
    if !pack.is_dir() {
        return Ok(files);
    }
    for entry in fs::read_dir(pack)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && is_pack_file(&entry.path()) {
            files.insert(entry.path());
        }
    }
    Ok(files)
}

fn remove_pack_files(paths: &HashSet<PathBuf>) -> Result<(), VaultError> {
    for pack in paths {
        fs::remove_file(pack)?;
        let idx = pack.with_extension("idx");
        if idx.is_file() {
            fs::remove_file(idx)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::TimeZone;
    use tempfile::TempDir;

    use super::*;
    use crate::adapters::GixObjectStore;
    use crate::config::VaultConfig;
    use crate::domain::{FileChange, FileEventKind, RelPath, VaultLayout};
    use crate::ports::ObjectStore;
    use crate::storage;

    fn fabricate_loose_tree(base: &Path, loose: usize, packs: usize) {
        let objects = base.join("objects");
        fs::create_dir_all(&objects).expect("mkdir objects");
        for i in 0..loose {
            let hex = format!("{i:040x}");
            let dir = objects.join(&hex[..2]);
            fs::create_dir_all(&dir).expect("mkdir shard");
            fs::write(dir.join(&hex[2..]), b"obj").expect("write loose");
        }
        let pack = objects.join("pack");
        fs::create_dir_all(&pack).expect("mkdir pack");
        for i in 0..packs {
            fs::write(pack.join(format!("pack-{i}.pack")), b"pack").expect("write pack");
        }
    }

    #[test]
    fn count_objects_reads_fabricated_tree() {
        let dir = TempDir::new().expect("tempdir");
        fabricate_loose_tree(dir.path(), 5, 2);
        let counts = count_objects(dir.path()).expect("count");
        assert_eq!(counts.loose, 5);
        assert_eq!(counts.packs, 2);
    }

    #[test]
    fn is_due_respects_each_threshold() {
        let thresholds = GcConfig {
            loose_object_limit: 10,
            pack_limit: 3,
            max_age_secs: 3600,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let recent = now - chrono::Duration::minutes(30);

        assert!(!is_due(
            ObjectCounts {
                loose: 10,
                packs: 3
            },
            Some(recent),
            &thresholds,
            now
        ));
        assert!(is_due(
            ObjectCounts {
                loose: 11,
                packs: 0
            },
            Some(recent),
            &thresholds,
            now
        ));
        assert!(is_due(
            ObjectCounts { loose: 0, packs: 4 },
            Some(recent),
            &thresholds,
            now
        ));
        assert!(is_due(
            ObjectCounts { loose: 0, packs: 0 },
            Some(now - chrono::Duration::seconds(3601)),
            &thresholds,
            now
        ));
        assert!(!is_due(
            ObjectCounts { loose: 5, packs: 1 },
            None,
            &thresholds,
            now
        ));
    }

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

    #[test]
    fn repack_packs_loose_objects_and_gix_can_read_back() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let store = GixObjectStore::open(&layout).expect("open store");
        let mut last_sha = None;
        for i in 0..5 {
            let rel = format!("doc-{i}.md");
            fs::write(layout.worktree.join(&rel), format!("content {i}")).expect("write");
            let changes = vec![FileChange {
                rel: RelPath::parse(&rel),
                kind: FileEventKind::Create,
            }];
            last_sha = store
                .commit(&changes, &format!("commit {i}"))
                .expect("commit");
        }
        let before = count_objects(&layout.git_dir_path()).expect("count before");
        assert!(before.loose > 0);

        let outcome = repack(&layout.git_dir_path()).expect("repack");
        assert!(outcome.objects_packed > 0);
        let after = count_objects(&layout.git_dir_path()).expect("count after");
        assert_eq!(after.loose, 0);
        assert_eq!(after.packs, 1);

        let sha = last_sha.expect("commit sha");
        let bytes = store
            .read_blob(&sha, &RelPath::parse("doc-4.md"))
            .expect("read")
            .expect("blob");
        assert_eq!(bytes, b"content 4");
    }

    #[test]
    fn maybe_run_repacks_when_loose_limit_low() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let store = GixObjectStore::open(&layout).expect("open store");
        for i in 0..3 {
            let rel = format!("f-{i}.md");
            fs::write(layout.worktree.join(&rel), b"x").expect("write");
            let changes = vec![FileChange {
                rel: RelPath::parse(&rel),
                kind: FileEventKind::Create,
            }];
            store.commit(&changes, "seed").expect("commit");
        }

        let gc = GcConfig {
            loose_object_limit: 1,
            ..GcConfig::default()
        };
        let status = maybe_run(&layout, &gc).expect("maybe_run");
        assert!(status.repack_ran);
        assert_eq!(status.counts.loose, 0);
        assert_eq!(status.counts.packs, 1);

        let status = maybe_run(&layout, &gc).expect("maybe_run again");
        assert!(!status.repack_ran);
    }
}
