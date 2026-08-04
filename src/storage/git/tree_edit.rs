//! Tree edit handlers for snapshot commits.

use std::path::Path;

use gix::object::tree::EntryKind;

use crate::domain::{FileChange, FileEventKind};
use crate::error::VaultError;

/// Inputs shared by tree-edit handlers.
pub(crate) struct TreeEditContext<'a> {
    pub repo: &'a gix::Repository,
    pub worktree: &'a Path,
}

type TreeChangeHandler = fn(
    &TreeEditContext<'_>,
    &mut gix::object::tree::Editor<'_>,
    &FileChange,
) -> Result<(), VaultError>;

pub(crate) fn tree_handler_for(kind: FileEventKind) -> TreeChangeHandler {
    match kind {
        FileEventKind::Create | FileEventKind::Modify | FileEventKind::Restore => {
            upsert_blob_in_tree
        }
        FileEventKind::Delete => remove_path_from_tree,
    }
}

pub(crate) fn apply_tree_changes(
    ctx: &TreeEditContext<'_>,
    editor: &mut gix::object::tree::Editor<'_>,
    changes: &[FileChange],
) -> Result<(), VaultError> {
    for change in changes {
        tree_handler_for(change.kind)(ctx, editor, change)?;
    }
    Ok(())
}

fn upsert_blob_in_tree(
    ctx: &TreeEditContext<'_>,
    editor: &mut gix::object::tree::Editor<'_>,
    change: &FileChange,
) -> Result<(), VaultError> {
    let abs = ctx.worktree.join(change.rel.to_path());
    let data = std::fs::read(&abs).map_err(VaultError::Io)?;
    let oid = ctx.repo.write_blob(&data).map_err(VaultError::git)?;
    editor
        .upsert(change.rel.as_str(), EntryKind::Blob, oid)
        .map_err(VaultError::git)?;
    Ok(())
}

fn remove_path_from_tree(
    _ctx: &TreeEditContext<'_>,
    editor: &mut gix::object::tree::Editor<'_>,
    change: &FileChange,
) -> Result<(), VaultError> {
    editor
        .remove(change.rel.as_str())
        .map_err(VaultError::git)?;
    Ok(())
}
