//! Git object-store filesystem scanning helpers.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use git2::Oid;
use serde::{Deserialize, Serialize};

use crate::error::VaultError;

/// Live object-store shape from a cheap directory scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectCounts {
    /// Number of loose object files under `objects/`.
    pub loose: usize,
    /// Number of `.pack` files under `objects/pack/`.
    pub packs: usize,
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

pub(crate) fn objects_dir(git_dir: &Path) -> PathBuf {
    git_dir.join("objects")
}

pub(crate) fn pack_dir(git_dir: &Path) -> PathBuf {
    objects_dir(git_dir).join("pack")
}

pub(crate) fn dir_size(path: &Path) -> Result<u64, VaultError> {
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

pub(crate) fn collect_loose_oids(git_dir: &Path) -> Result<Vec<Oid>, VaultError> {
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

pub(crate) fn remove_loose_objects(git_dir: &Path) -> Result<(), VaultError> {
    for_each_loose_object(&objects_dir(git_dir), |_, path| {
        fs::remove_file(path)?;
        Ok(())
    })
}

pub(crate) fn list_pack_files(pack: &Path) -> Result<HashSet<PathBuf>, VaultError> {
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

pub(crate) fn remove_pack_files(paths: &HashSet<PathBuf>) -> Result<(), VaultError> {
    for pack in paths {
        fs::remove_file(pack)?;
        let idx = pack.with_extension("idx");
        if idx.is_file() {
            fs::remove_file(idx)?;
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

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
}
