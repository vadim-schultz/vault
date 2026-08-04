//! Dimension 5: vault count (vaults registered to one daemon).
//!
//! Hypothesis under test: `registry.toml` is read/parsed/rewritten whole-file on every mutation,
//! and `Router::from_registry` reloads every vault's config + compiled ignore matcher on every
//! hot-reload tick, with `Router::vault_for` doing a linear scan per routed path.

#[path = "support.rs"]
mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use vault::registry::VaultRegistry;
use vault::watcher::Router;

/// Cheap to set up even at large N — no real vault directories on disk.
const REGISTRY_SIZES: &[usize] = &[10, 100, 1_000, 10_000];
/// Router reload needs a fully-initialized vault per entry, so this stays smaller.
const ROUTER_SIZES: &[usize] = &[10, 100, 500, 2_000];

fn bench_registry_save(c: &mut Criterion) {
    support::isolate_state_dir();
    let mut group = c.benchmark_group("vault_count/registry_save");
    group.sample_size(20);
    for &n in REGISTRY_SIZES {
        let registry = support::synthetic_registry(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| registry.save().expect("registry save"));
        });
    }
    group.finish();
}

fn bench_registry_load(c: &mut Criterion) {
    support::isolate_state_dir();
    let mut group = c.benchmark_group("vault_count/registry_load");
    group.sample_size(20);
    for &n in REGISTRY_SIZES {
        support::synthetic_registry(n).save().expect("seed save");
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| VaultRegistry::load().expect("registry load"));
        });
    }
    group.finish();
}

fn bench_router_from_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("vault_count/router_from_registry");
    group.sample_size(10);
    for &n in ROUTER_SIZES {
        let (_vaults, registry) = support::real_registry(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| Router::from_registry(&registry).expect("router from registry"));
        });
    }
    group.finish();
}

fn bench_router_route(c: &mut Criterion) {
    let mut group = c.benchmark_group("vault_count/router_route_single_event");
    group.sample_size(20);
    for &n in ROUTER_SIZES {
        let (vaults, registry) = support::real_registry(n);
        let router = Router::from_registry(&registry).expect("router from registry");
        // Route an event for the *last* registered vault — worst case for a linear scan.
        let target = vaults.last().expect("at least one vault");
        let path = target.layout.worktree.join("doc-000000.md");
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| router.route(vec![path.clone()]));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_registry_save,
    bench_registry_load,
    bench_router_from_registry,
    bench_router_route
);
criterion_main!(benches);
