//! Git repack via git2 `PackBuilder`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use git2::{Oid, Repository};

use crate::error::VaultError;

use super::fs::{
    collect_loose_oids, dir_size, list_pack_files, pack_dir, remove_loose_objects,
    remove_pack_files,
};

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

pub(crate) struct RepackPrep {
    bytes_before: u64,
    loose_oids: Vec<Oid>,
    old_packs: HashSet<PathBuf>,
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::adapters::GixObjectStore;
    use crate::config::VaultConfig;
    use crate::domain::{FileChange, FileEventKind, RelPath, VaultLayout};
    use crate::ports::ObjectStore;
    use crate::storage;
    use crate::storage::housekeeping::fs::count_objects;

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
}
