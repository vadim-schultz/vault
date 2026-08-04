//! Dimension 1: history depth (total edits over a vault's lifetime).
//!
//! Hypothesis under test: `snapshots.created_at` has no index (only
//! `file_events(path, snapshot_id)` does), so `resolve_at` — which backs `show`, `diff`, and
//! `restore` — degrades from O(log n) to a full table scan as total snapshot count grows.

#[path = "support.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use vault::adapters::SqliteMetaIndex;
use vault::domain::RelPath;
use vault::ports::MetaIndex;

const SIZES: &[usize] = &[100, 1_000, 10_000, 50_000];
const PATH_POOL: usize = 50;

fn seeded_index(n: usize) -> (support::BenchVault, SqliteMetaIndex) {
    let vault = support::new_vault();
    let index = SqliteMetaIndex::open(vault.layout.meta_db_path()).expect("open meta index");
    support::seed_snapshots(&index, n, PATH_POOL);
    (vault, index)
}

fn bench_resolve_at(c: &mut Criterion) {
    let mut group = c.benchmark_group("history_depth/resolve_at");
    group.sample_size(30);
    for &n in SIZES {
        let (_vault, index) = seeded_index(n);
        let midpoint = support::ts((n / 2) as u64);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| index.resolve_at(&midpoint).expect("resolve_at"));
        });
    }
    group.finish();
}

fn bench_list_snapshots_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("history_depth/list_snapshots_all");
    group.sample_size(30);
    for &n in SIZES {
        let (_vault, index) = seeded_index(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| index.list_snapshots(None).expect("list_snapshots(None)"));
        });
    }
    group.finish();
}

fn bench_list_snapshots_for_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("history_depth/list_snapshots_for_path");
    group.sample_size(30);
    for &n in SIZES {
        let (_vault, index) = seeded_index(n);
        let path = RelPath::parse("file-0001.md");
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                index
                    .list_snapshots(Some(&path))
                    .expect("list_snapshots(Some)")
            });
        });
    }
    group.finish();
}

fn bench_list_tracked_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("history_depth/list_tracked_files");
    group.sample_size(30);
    for &n in SIZES {
        let (_vault, index) = seeded_index(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| index.list_tracked_files().expect("list_tracked_files"));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_resolve_at,
    bench_list_snapshots_all,
    bench_list_snapshots_for_path,
    bench_list_tracked_files
);
criterion_main!(benches);
