//! Tree diff — the inverse of `tree_edit.rs`: classify every path that differs between two trees
//! as `Create`, `Modify`, or `Delete`. Backs `vault reindex`'s replay of `.git`'s commit history.

use std::collections::BTreeMap;

use gix::bstr::ByteSlice;
use gix::object::tree::{Entry, EntryKind};

use crate::domain::{FileChange, FileEventKind, RelPath};
use crate::error::VaultError;

/// Diff `old` against `new`, returning one [`FileChange`] per path that differs.
///
/// # Errors
///
/// Returns [`VaultError::Git`] when tree entries cannot be decoded or read, or when a path is a
/// blob (or symlink/submodule) in one tree and a directory in the other — an invariant vault's
/// own write path never produces (a `RelPath` is always a blob leaf, `tree_edit.rs` never upserts
/// a directory), so hitting it means `.vault/.git` was mutated by something other than vault
/// itself.
pub(crate) fn diff_trees(
    old: &gix::Tree<'_>,
    new: &gix::Tree<'_>,
) -> Result<Vec<FileChange>, VaultError> {
    let mut changes = Vec::new();
    diff_into("", old, new, &mut changes)?;
    Ok(changes)
}

fn diff_into(
    prefix: &str,
    old: &gix::Tree<'_>,
    new: &gix::Tree<'_>,
    changes: &mut Vec<FileChange>,
) -> Result<(), VaultError> {
    let old_entries = collect_entries(old)?;
    let new_entries = collect_entries(new)?;

    let mut names: Vec<&String> = old_entries.keys().chain(new_entries.keys()).collect();
    names.sort();
    names.dedup();

    for name in names {
        let path = join(prefix, name);
        match (old_entries.get(name), new_entries.get(name)) {
            (None, Some(entry)) => add_leaves(&path, entry, FileEventKind::Create, changes)?,
            (Some(entry), None) => add_leaves(&path, entry, FileEventKind::Delete, changes)?,
            (Some(old_entry), Some(new_entry)) => {
                diff_pair(&path, old_entry, new_entry, changes)?;
            }
            (None, None) => unreachable!("name came from at least one of the two maps"),
        }
    }
    Ok(())
}

/// Dispatch on what kind of thing `old_entry`/`new_entry` are, now that they're known to differ.
fn diff_pair(
    path: &str,
    old_entry: &Entry<'_>,
    new_entry: &Entry<'_>,
    changes: &mut Vec<FileChange>,
) -> Result<(), VaultError> {
    if old_entry.oid() == new_entry.oid() {
        return Ok(());
    }
    match (old_entry.mode().kind(), new_entry.mode().kind()) {
        (EntryKind::Tree, EntryKind::Tree) => diff_subtrees(path, old_entry, new_entry, changes),
        (EntryKind::Tree, _) | (_, EntryKind::Tree) => Err(type_change_error(path)),
        _ => {
            push_change(path, FileEventKind::Modify, changes);
            Ok(())
        }
    }
}

/// Recurse into two same-named subtrees.
fn diff_subtrees(
    path: &str,
    old_entry: &Entry<'_>,
    new_entry: &Entry<'_>,
    changes: &mut Vec<FileChange>,
) -> Result<(), VaultError> {
    let old_tree = subtree(old_entry)?;
    let new_tree = subtree(new_entry)?;
    diff_into(path, &old_tree, &new_tree, changes)
}

/// Record `entry` (and, if it's a directory, every leaf beneath it) as `kind`.
fn add_leaves(
    path: &str,
    entry: &Entry<'_>,
    kind: FileEventKind,
    changes: &mut Vec<FileChange>,
) -> Result<(), VaultError> {
    if entry.mode().kind() != EntryKind::Tree {
        push_change(path, kind, changes);
        return Ok(());
    }
    let tree = subtree(entry)?;
    for (name, child_entry) in collect_entries(&tree)? {
        add_leaves(&join(path, &name), &child_entry, kind, changes)?;
    }
    Ok(())
}

fn push_change(path: &str, kind: FileEventKind, changes: &mut Vec<FileChange>) {
    changes.push(FileChange {
        rel: RelPath::parse(path),
        kind,
    });
}

fn subtree<'repo>(entry: &Entry<'repo>) -> Result<gix::Tree<'repo>, VaultError> {
    entry
        .object()
        .map_err(VaultError::git)?
        .try_into_tree()
        .map_err(VaultError::git)
}

fn collect_entries<'repo>(
    tree: &gix::Tree<'repo>,
) -> Result<BTreeMap<String, Entry<'repo>>, VaultError> {
    let mut map = BTreeMap::new();
    for entry in tree.iter() {
        let entry = entry.map_err(VaultError::git)?;
        let name = entry
            .filename()
            .to_str()
            .map_err(VaultError::git)?
            .to_string();
        map.insert(name, entry.to_owned());
    }
    Ok(map)
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn type_change_error(path: &str) -> VaultError {
    VaultError::git(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("path {path} changed between a file and a directory across commits"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Build a real repo through the crate's own `init`, not `gix::create::into` directly — bare
    /// repo creation resolves some paths relative to the process cwd, and `init`'s
    /// `WorktreeCwd::enter` guard (`worktree_cwd.rs`) is what makes that safe under parallel test
    /// execution, where the ambient cwd left behind by an unrelated test is not to be trusted.
    fn open_repo(dir: &TempDir) -> gix::Repository {
        let worktree = dir.path().join("project");
        let git_dir = dir.path().join(".vault").join(".git");
        std::fs::create_dir_all(&worktree).expect("worktree");
        super::super::init(&git_dir, &worktree).expect("init").repo
    }

    fn write_tree(repo: &gix::Repository, files: &[(&str, &str)]) -> gix::ObjectId {
        let mut editor = repo
            .edit_tree(repo.empty_tree().id().detach())
            .expect("editor");
        for (path, content) in files {
            let oid = repo.write_blob(content.as_bytes()).expect("blob");
            editor.upsert(*path, EntryKind::Blob, oid).expect("upsert");
        }
        editor.write().expect("write").detach()
    }

    fn tree_at(repo: &gix::Repository, id: gix::ObjectId) -> gix::Tree<'_> {
        repo.find_object(id).expect("find").into_tree()
    }

    fn sorted(mut changes: Vec<FileChange>) -> Vec<FileChange> {
        changes.sort_by(|a, b| a.rel.as_str().cmp(b.rel.as_str()));
        changes
    }

    fn change(path: &str, kind: FileEventKind) -> FileChange {
        FileChange {
            rel: RelPath::parse(path),
            kind,
        }
    }

    #[test]
    fn detects_create_modify_delete_across_nested_paths() {
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        let old_id = write_tree(
            &repo,
            &[
                ("keep/a.md", "same"),
                ("gone/b.md", "bye"),
                ("change/c.md", "old"),
            ],
        );
        let new_id = write_tree(
            &repo,
            &[
                ("keep/a.md", "same"),
                ("change/c.md", "new"),
                ("fresh/d.md", "hi"),
            ],
        );
        let old = tree_at(&repo, old_id);
        let new = tree_at(&repo, new_id);

        let changes = sorted(diff_trees(&old, &new).expect("diff"));

        assert_eq!(
            changes,
            vec![
                change("change/c.md", FileEventKind::Modify),
                change("fresh/d.md", FileEventKind::Create),
                change("gone/b.md", FileEventKind::Delete),
            ]
        );
    }

    #[test]
    fn unchanged_paths_are_excluded() {
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        let id = write_tree(&repo, &[("a.md", "same"), ("nested/b.md", "also same")]);
        let tree = tree_at(&repo, id);

        assert_eq!(diff_trees(&tree, &tree).expect("diff"), vec![]);
    }

    #[test]
    fn root_commit_against_empty_tree_is_all_creates() {
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        let new_id = write_tree(&repo, &[("a.md", "1"), ("nested/deep/b.md", "2")]);
        let empty = repo.empty_tree();
        let new = tree_at(&repo, new_id);

        let changes = sorted(diff_trees(&empty, &new).expect("diff"));

        assert_eq!(
            changes,
            vec![
                change("a.md", FileEventKind::Create),
                change("nested/deep/b.md", FileEventKind::Create),
            ]
        );
    }

    #[test]
    fn deleting_a_whole_subtree_lists_every_leaf() {
        let dir = TempDir::new().expect("tempdir");
        let repo = open_repo(&dir);
        let old_id = write_tree(&repo, &[("dir/a.md", "1"), ("dir/nested/b.md", "2")]);
        let new_id = write_tree(&repo, &[]);
        let old = tree_at(&repo, old_id);
        let new = tree_at(&repo, new_id);

        let changes = sorted(diff_trees(&old, &new).expect("diff"));

        assert_eq!(
            changes,
            vec![
                change("dir/a.md", FileEventKind::Delete),
                change("dir/nested/b.md", FileEventKind::Delete),
            ]
        );
    }
}
