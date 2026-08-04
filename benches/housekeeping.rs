//! Housekeeping cost — `count_objects` and `repack` at growing object counts.

#[path = "support.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use vault::storage::housekeeping;

const SIZES: &[usize] = &[100, 1_000, 10_000, 50_000];

fn seed_loose_objects(vault: &support::BenchVault, n: usize) {
    let _store = support::seed_tracked_files(vault, n);
}

fn bench_count_objects(c: &mut Criterion) {
    let mut group = c.benchmark_group("housekeeping/count_objects");
    group.sample_size(10);
    for &n in SIZES {
        let vault = support::new_vault();
        seed_loose_objects(&vault, n);
        let git_dir = vault.layout.git_dir_path();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| housekeeping::count_objects(&git_dir).expect("count"));
        });
    }
    group.finish();
}

fn bench_repack(c: &mut Criterion) {
    let mut group = c.benchmark_group("housekeeping/repack");
    group.sample_size(10);
    for &n in SIZES {
        let vault = support::new_vault();
        seed_loose_objects(&vault, n);
        let git_dir = vault.layout.git_dir_path();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| housekeeping::repack(&git_dir).expect("repack"));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_count_objects, bench_repack);
criterion_main!(benches);
