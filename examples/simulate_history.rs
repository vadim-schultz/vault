//! Simulate N sequential organic edits using the exact production commit path
//! (`GixObjectStore` + `SqliteMetaIndex` + `app::snapshot::commit`), without waiting for the
//! watcher's debounce window. Used by `scripts/stress/*.sh` to build an "aged" vault fixture
//! (years of history compressed into seconds) instead of waiting for real debounced edits.
//!
//! Usage: `simulate_history <vault-root> <num-edits> [num-files]`
//!
//! Requires the vault root to already be `vault init`-ed. Edits rotate across `num-files`
//! distinct paths (default 1, i.e. repeatedly editing a single file).

use std::path::PathBuf;

use vault::adapters::{GixObjectStore, SqliteMetaIndex, SystemClock};
use vault::app::snapshot;
use vault::domain::{FileChange, FileEventKind, RelPath, VaultLayout};

fn main() {
    let mut args = std::env::args().skip(1);
    let root: PathBuf = args
        .next()
        .expect("usage: simulate_history <vault-root> <num-edits> [num-files]")
        .into();
    let n: usize = args
        .next()
        .expect("num-edits required")
        .parse()
        .expect("num-edits must be a number");
    let num_files: usize = args
        .next()
        .map(|s| s.parse().expect("num-files must be a number"))
        .unwrap_or(1);

    let layout = VaultLayout::from_worktree(root);
    let object_store = GixObjectStore::open(&layout).expect("open git store");
    let meta_index = SqliteMetaIndex::open(layout.meta_db_path()).expect("open meta index");

    for i in 0..n {
        let file_idx = i % num_files;
        let rel = format!("history-{file_idx:03}.md");
        std::fs::write(layout.worktree.join(&rel), format!("edit #{i}")).expect("write file");
        let changes = vec![FileChange {
            rel: RelPath::parse(&rel),
            kind: FileEventKind::Modify,
        }];
        snapshot::commit(&layout, &changes, &SystemClock, &object_store, &meta_index)
            .expect("commit");
        if i % 5000 == 0 && i > 0 {
            eprintln!("  {i}/{n} commits...");
        }
    }
    println!("done: {n} commits across {num_files} file(s)");
}
