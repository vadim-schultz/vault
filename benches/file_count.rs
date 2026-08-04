//! Dimension 2: file count (breadth) — vault init baseline cost, and whether a steady-state
//! single-file commit stays cheap regardless of how many files the tree already tracks.

#[path = "support.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

use vault::adapters::GixObjectStore;
use vault::config::VaultConfig;
use vault::domain::{FileChange, FileEventKind, RelPath};
use vault::ports::ObjectStore;
use vault::walk::collect_baseline_changes;

const SIZES: &[usize] = &[100, 1_000, 10_000, 50_000];

fn write_n_files(vault: &support::BenchVault, n: usize) {
    for i in 0..n {
        std::fs::write(
            vault.layout.worktree.join(format!("doc-{i:06}.md")),
            b"seed content",
        )
        .expect("write file");
    }
}

/// Baseline `vault init`: walk N untracked files and commit them all in one shot.
fn bench_baseline_init(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_count/baseline_init");
    group.sample_size(10);
    for &n in SIZES {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || {
                    let vault = support::new_vault();
                    write_n_files(&vault, n);
                    let store = GixObjectStore::open(&vault.layout).expect("git store open");
                    let config = VaultConfig::defaults();
                    (vault, store, config)
                },
                |(vault, store, config)| {
                    let changes = collect_baseline_changes(&vault.layout, &config).expect("walk");
                    store
                        .commit(&changes, "vault: baseline snapshot")
                        .expect("baseline commit");
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// Steady state: the tree already tracks N files; commit one more edit to a single hot path.
/// Tests whether gix's incremental tree editor keeps this independent of N, as assumed.
fn bench_steady_state_single_edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_count/steady_state_single_edit");
    group.sample_size(30);
    for &n in SIZES {
        let vault = support::new_vault();
        let store = support::seed_tracked_files(&vault, n);
        let hot_path = vault.layout.worktree.join("hot.md");
        let mut counter: u64 = 0;
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                counter += 1;
                std::fs::write(&hot_path, format!("edit #{counter}")).expect("write hot file");
                let changes = vec![FileChange {
                    rel: RelPath::parse("hot.md"),
                    kind: FileEventKind::Modify,
                }];
                store
                    .commit(&changes, "vault: edit hot.md")
                    .expect("steady-state commit");
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_baseline_init, bench_steady_state_single_edit);
criterion_main!(benches);
