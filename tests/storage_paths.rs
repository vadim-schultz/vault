//! Integration tests for git/sqlite path spelling consistency.

mod common;

use rusqlite::Connection;
use tempfile::TempDir;
use vault::domain::{FileChange, FileEventKind, RelPath, VaultLayout};
use vault::paths::VAULT_DIR;
use vault::snapshot::commit_changes;

#[test]
fn git_and_sqlite_agree_on_nested_path() {
    let _env = common::VaultEnv::new();
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("notes.md"), b"root").expect("write");
    common::init_in(dir.path());

    std::fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
    std::fs::write(dir.path().join("sub").join("b.md"), b"b").expect("write");

    let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
    let rel = RelPath::parse("sub/b.md");
    let changes = vec![FileChange {
        rel: rel.clone(),
        kind: FileEventKind::Create,
    }];
    commit_changes(&layout, &changes)
        .expect("commit")
        .expect("some");

    let conn =
        Connection::open(dir.path().join(VAULT_DIR).join(vault::paths::META_DB)).expect("open db");
    let sqlite_path: String = conn
        .query_row(
            "SELECT path FROM file_events ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("query");
    assert_eq!(sqlite_path, rel.as_str());
}
