//! Integration tests for concurrent vault commits.

mod common;

use std::sync::{Arc, Barrier};
use std::thread;

use tempfile::TempDir;
use vault::domain::RelPath;
use vault::registry::VaultRegistry;
use vault::watcher::Router;

#[test]
fn two_vaults_commit_in_parallel() {
    let _env = common::VaultEnv::new();
    let dir_a = TempDir::new().expect("dir a");
    let dir_b = TempDir::new().expect("dir b");
    std::fs::write(dir_a.path().join("a.md"), b"a").expect("write");
    std::fs::write(dir_b.path().join("b.md"), b"b").expect("write");
    common::init_in(dir_a.path());
    common::init_in(dir_b.path());

    let registry = VaultRegistry::load().expect("registry");
    let router = Router::from_registry(&registry).expect("router");
    let vault_a = router
        .route(vec![dir_a.path().join("a.md")])
        .into_iter()
        .next()
        .expect("vault a")
        .0;
    let vault_b = router
        .route(vec![dir_b.path().join("b.md")])
        .into_iter()
        .next()
        .expect("vault b")
        .0;

    let barrier = Arc::new(Barrier::new(3));
    let b1 = Arc::clone(&barrier);
    let b2 = Arc::clone(&barrier);
    let handle_a = thread::spawn(move || {
        b1.wait();
        vault::watcher::worker::commit_batch(&vault_a, &[RelPath::parse("a.md")])
    });
    let handle_b = thread::spawn(move || {
        b2.wait();
        vault::watcher::worker::commit_batch(&vault_b, &[RelPath::parse("b.md")])
    });
    barrier.wait();
    handle_a.join().expect("join a").expect("commit a");
    handle_b.join().expect("join b").expect("commit b");
}
