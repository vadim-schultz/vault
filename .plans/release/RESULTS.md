# Release binary size — before/after results

Measured on one dev machine (macOS, arm64), release builds, 2026-08-05. Raw source:
`benches/*.rs` (criterion, `cargo bench`). This is the verification record for
[binary_size.plan.md](binary_size.plan.md) items 1–2.

## Binary size

| Config | Size | Δ from baseline |
|---|---:|---:|
| baseline (no `[profile.release]`, full `gix`/`tokio` features) | 9.53 MB | — |
| + `strip = true` | 7.76 MB | −18.5% |
| + `lto = true`, `codegen-units = 1` | 6.68 MB | −29.9% |
| + trimmed `gix`/`tokio` features | 6.58 MB | −30.9% |
| + `opt-level = "z"` (tried, then reverted — see below) | 4.10 MB | −57.0% |

**Landed config:** `lto = true`, `codegen-units = 1`, `strip = true` + trimmed `gix`/`tokio`
features. **6.58 MB, a 30.9% reduction**, `opt-level` left at its release default (`3`).

## Runtime: `opt-level = "z"` caused broad regressions — reverted

First pass included `opt-level = "z"` (it was the single largest size contributor). A full
`cargo bench` before/after comparison at that setting showed **consistent, often severe**
regressions across every bench group, not just size-sensitive hot loops:

| Benchmark | Default `opt-level` | `opt-level = "z"` | Δ |
|---|---:|---:|---:|
| `history_depth/resolve_at` (all N) | ~2.0 µs | ~3.0 µs | **+45–52%** |
| `history_depth/list_tracked_files/50000` | 1.69 ms | 2.50 ms | **+48%** |
| `vault_count/registry_load/10000` | 6.21 ms | 14.60 ms | **+135%** |
| `vault_count/router_from_registry/2000` | 125.7 ms | 213.8 ms | **+70%** |
| `queue_latency/enqueue` (all N) | ~300 ns | ~447 ns | **+48%** |
| `file_count/steady_state_single_edit/50000` | 73.1 ms | 81.3 ms | **+11%** |

That's real cost on the daemon's hot paths (`registry_load`, `router_from_registry` run on every
hot-reload tick; `resolve_at` backs `show`/`diff`/`restore`) — not worth 2.5 MB more of binary
size. `opt-level = "z"` was reverted.

## Runtime: `lto` + `codegen-units = 1` + `strip` (landed config) — no regression

Re-ran the same bench groups with `opt-level` back at its default (`3`), everything else
unchanged from the reverted pass. All deltas fall within normal criterion noise (roughly ±10%,
no consistent direction):

| Benchmark | Baseline | Landed config | Δ |
|---|---:|---:|---:|
| `history_depth/resolve_at/50000` | 2.06 µs | 2.09 µs | +1.7% |
| `history_depth/list_tracked_files/50000` | 1.69 ms | 1.63 ms | −3.7% |
| `vault_count/registry_load/10000` | 6.21 ms | 5.58 ms | −10.2% |
| `vault_count/router_from_registry/2000` | 125.7 ms | 122.0 ms | −2.9% |
| `queue_latency/enqueue/100` | 321 ns | 281 ns | −12.4% |
| `file_count/steady_state_single_edit/50000` | 73.1 ms | 71.2 ms | −2.6% |
| `file_count/baseline_init/50000` | 5.05 s | 5.08 s | +0.6% |

No group showed a directional regression beyond noise. `lto`/`codegen-units = 1` are, if
anything, mildly *helpful* for a few paths (more cross-crate inlining), consistent with them
being pure codegen-strategy changes rather than a speed/size trade like `opt-level`.

## Pre-existing issue found, unrelated to this change

`housekeeping/repack` (in `benches/housekeeping.rs`) panics on `Benchmarking
housekeeping/repack/100`'s warm-up iteration with `Git(Error { code: -3, klass: 9, message:
"object not found..." })`, in **both** the baseline and every post-change run — confirmed
identical failure before any `Cargo.toml` edits. Cargo's default `cargo bench` stops running
further bench targets after a panic in one, which is why `vault_count` and `queue_latency` had to
be re-run separately with explicit `--bench` flags to get numbers for this comparison. This is a
harness bug (`storage::housekeeping::tests::repack_packs_loose_objects_and_gix_can_read_back`
exercises the same repack path via the unit-test suite and passes cleanly, in both `cargo test
--release --lib` runs) — filed as follow-up, out of scope for this plan.

## Test suite

`cargo test --release` (unit + all integration suites, 102 lib tests + all `tests/*.rs`) — full
pass on the landed config.
