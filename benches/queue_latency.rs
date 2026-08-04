//! Queue latency — cost of enqueue vs synchronous reconcile_walk execution.

#[path = "support.rs"]
mod support;

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use vault::adapters::{GixObjectStore, InMemoryQueueStore, SqliteMetaIndex, SystemClock};
use vault::app::snapshot;
use vault::config::VaultConfig;
use vault::domain::TaskKind;
use vault::queue::{handlers, WorkQueue};
use vault::walk::collect_baseline_changes;

const SIZES: &[usize] = &[100, 1_000, 10_000, 50_000];

fn seed_vault_for_reconcile(vault: &support::BenchVault, n: usize) {
    let _ = support::seed_tracked_files(vault, n);
    let config = VaultConfig::defaults();
    let changes = collect_baseline_changes(&vault.layout, &config).expect("walk");
    let object_store = GixObjectStore::open(&vault.layout).expect("git");
    let meta_index = SqliteMetaIndex::open(vault.layout.meta_db_path()).expect("meta");
    snapshot::commit(
        &vault.layout,
        &changes,
        &SystemClock,
        &object_store,
        &meta_index,
    )
    .expect("baseline commit");
}

fn bench_reconcile_walk_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("queue_latency/reconcile_walk_sync");
    group.sample_size(10);
    for &n in SIZES {
        let vault = support::new_vault();
        seed_vault_for_reconcile(&vault, n);
        let root = vault.layout.worktree.clone();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                handlers::run(&TaskKind::reconcile_walk_once(root.clone())).expect("reconcile");
            });
        });
    }
    group.finish();
}

fn bench_enqueue(c: &mut Criterion) {
    let mut group = c.benchmark_group("queue_latency/enqueue");
    group.sample_size(30);
    for &n in SIZES {
        let vault = support::new_vault();
        seed_vault_for_reconcile(&vault, n);
        let root = vault.layout.worktree.clone();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let queue = WorkQueue::new(Arc::new(InMemoryQueueStore::new()));
                queue
                    .enqueue(TaskKind::reconcile_walk_once(root.clone()))
                    .expect("enqueue");
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_reconcile_walk_sync, bench_enqueue);
criterion_main!(benches);
