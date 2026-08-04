# Benchmark & stress-test results

Measured on one dev machine (macOS, APFS), release builds, 2026-08-03. Absolute numbers will
vary by hardware; the *shape* of each curve (flat vs. linear vs. cliff) is what matters and is
expected to reproduce. Raw source: `benches/*.rs` (criterion, `cargo bench --bench <name>`) and
`scripts/stress/*.rs` (real daemon + real filesystem, `bash scripts/stress/<name>.sh`).

This doc is the measurement record for [benchmark.plan.md](benchmark.plan.md).
Fixes are out of scope here — see [optimize.plan.md](optimize.plan.md) for what
to do about each "needs limit" / "needs fix" verdict below.

## Summary

| # | Dimension | Verdict | One-line finding |
|---|-----------|---------|-------------------|
| 1 | History depth | **needs fix** | `resolve_at` scans linearly — confirmed, no surprises |
| 2 | File count (breadth) | **needs fix** | Single-file edits get slower as *total* tracked files grow — assumption refuted |
| 3 | File size | **needs limit** | Over-limit files are silently dropped, zero user-visible signal |
| 4 | Edit burst size | **needs fix — highest priority** | Silent data loss past ~10k simultaneous file events |
| 5 | Vault count | **needs fix** | Registry reload cost is O(n) on every hot-reload tick |
| 6 | Repo growth / no GC | **needs fix** | ~10KB disk per commit for <100 bytes of real content, unbounded |
| 7 | Concurrent readers/writer | **fine as-is** | No lock errors through 32 readers + sustained writer |

---

## 1. History depth — `benches/history_depth.rs`

Hypothesis: `snapshots.created_at` has no index, so `resolve_at` (backs `show`/`diff`/`restore`)
degrades to a full table scan as total snapshot count grows. **Confirmed.**

| Snapshots | `resolve_at` | `list_snapshots(None)` (`log`) | `list_snapshots(Some(path))` | `list_tracked_files` (`list`) |
|-----------|-------------:|----------------------------:|------------------------------:|-------------------------------:|
| 100 | 8.5 µs | 20.8 µs | 4.3 µs | 31.2 µs |
| 1,000 | 57.8 µs | 192 µs | 11.5 µs | 253 µs |
| 10,000 | 545 µs | 1.95 ms | 89.5 µs | 2.82 ms |
| 50,000 | 3.15 ms | 16.0 ms | 1.07 ms | 15.6 ms |

All four scale roughly linearly with total row count. `resolve_at` and `list_tracked_files` are
the two that matter most: every `show`/`diff`/`restore` call pays the `resolve_at` cost, and
`list_tracked_files` (`vault list`) scales with **total edit count**, not distinct file count,
even though the query's job is to return one row per distinct file — the correlated subquery in
`SELECT_TRACKED_FILES` re-evaluates per underlying row, not per distinct path. `list_snapshots`
(both variants) is inherently O(n) in output size regardless of indexing since there's no
pagination — that's a separate, milder issue from the missing index.

At realistic scale (a vault edited daily for a year is ~365-off snapshots) none of this is
user-visible. It becomes visible somewhere in the tens of thousands of total edits — plausible
for a vault watched continuously over several years, or one with very frequent small edits.

## 2. File count (breadth) — `benches/file_count.rs`

### Baseline `vault init`

| Files | Walk + baseline commit |
|-------|------------------------:|
| 100 | 7.1 ms |
| 1,000 | 52.4 ms |
| 10,000 | 591 ms |
| 50,000 | 5.02 s |

Linear, ~0.1ms/file. `vault init` on a 50k-file docs folder blocks for ~5 seconds with **no
progress feedback** — the first place a real user would plausibly think the tool hung.

### Steady-state single-file edit (the assumption this refutes)

The working assumption going in was that gix's tree editor only touches the path being changed,
so a single-file edit should cost roughly the same regardless of how many *other* files the vault
tracks. **Refuted:**

| Total tracked files | Cost of committing one more single-file edit |
|----------------------|------------------------------------------------:|
| 100 | ~5.7 ms (noisy, low sample) |
| 1,000 | 2.85 ms |
| 10,000 | 16.3 ms |
| 50,000 | 76.8 ms |

Clearly grows with total tracked file count. Root cause: the seed fixture puts all files flat in
one directory, and a git tree object is a single serialized listing of every entry in that
directory — changing one blob still means writing out a brand-new tree object containing **all**
entries, sorted, re-hashed. There's no per-directory sharding happening here because there's only
one directory. A real vault with edits spread across many subdirectories would likely see this
cost bounded by the size of the *deepest single directory*, not the whole vault — worth
confirming, but not measured here.

Practical implication: a large, flat-structured vault (single directory, tens of thousands of
files) pays a real and growing tax on every single edit, independent of file count anywhere else
in this benchmark.

## 3. File size — `scripts/stress/filesize.sh`

| Size | Committed? | Latency |
|------|-----------|---------|
| 1KB – 10MB (at/under `max_file_bytes`) | yes | ~2.1–2.6s, flat — dominated by the fixed 2s debounce window, not file size |
| 11MB, 50MB (over `max_file_bytes`) | **no** | n/a |

Confirmed: over-limit files never appear in `vault list`, and `vault status` gives no indication
anything was skipped. Zero errors, zero warnings, zero log lines — a user who drops a 20MB PDF
export into a watched folder gets no signal that Vault silently isn't tracking it.

## 4. Edit burst size — `scripts/stress/edit_burst.sh`

The one that matters most. N files created simultaneously (inside one debounce window — a bulk
rename, branch switch, unzip, find-replace):

| Files touched | Tracked after settling | Loss |
|---------------|------------------------:|-----:|
| 100 | 100 | 0% |
| 1,000 | 1,000 | 0% |
| 5,000 | 5,000 | 0% |
| 10,000 | 8,360 | 16.4% |
| 15,000 | 9,375 | 37.5% |
| 20,000 | 11,432 | 42.8% |

Reproduced twice, consistent knee point between 5,000 and 10,000 simultaneous file-creation
events. **The daemon silently drops a growing fraction of the files — permanently.** There is no
periodic re-walk after `vault init`'s baseline scan, so a file whose creation event never reached
the debouncer or got dropped from an internal buffer stays untracked forever unless it's edited
again individually later. The daemon log shows zero errors or warnings when this happens — it is
indistinguishable, from the user's side, from "everything worked."

Not yet root-caused here (that's for the optimize plan to scope) — plausible culprits: the OS
watch backend (`notify`/FSEvents on macOS) coalescing or capping events under a large burst, or an
internal channel/buffer in `notify-debouncer-full` or the watcher's own event handling dropping
events under backpressure rather than blocking. `ulimit -n` was not the bottleneck (1,048,576 on
this machine).

## 5. Vault count — `benches/vault_count.rs`

| Vaults | `registry.toml` save | `registry.toml` load | `Router::from_registry` | `Router::route` (1 event) |
|--------|----------------------:|----------------------:|--------------------------:|------------------------------:|
| 10 | 4.65 ms | 17.2 µs | 542 µs | 9.4 µs |
| 100 | 4.85 ms | 76.0 µs | 5.72 ms | 19.2 µs |
| 500 / 1,000 | 6.26 ms | 590 µs | 31.0 ms (@500) | 59.2 µs (@500) |
| 2,000 / 10,000 | 13.7 ms | 7.50 ms | 129 ms (@2,000) | 215 µs (@2,000) |

`registry.toml` load/save is linear but small in absolute terms until very large vault counts
(thousands). `Router::from_registry` — which reloads **every** vault's config and compiled ignore
matcher on **every hot-reload tick** — is the one to watch: 129ms at 2,000 registered vaults means
every registry change (e.g. `vault init` somewhere else on the machine) pauses the watcher's event
processing for that long. `Router::route`'s linear scan is small in comparison at this scale but
grows the same way.

At the "a handful of project vaults" scale this ships for today, none of this is visible. It
becomes visible for a power user who runs `vault init` per-repo across a large body of work.

## 6. Repo growth / no GC — `scripts/stress/object_growth.sh` + `examples/simulate_history.rs`

One file, edited repeatedly (`examples/simulate_history` drives the real commit path directly,
bypassing the daemon's debounce, so years of edits compress into seconds):

| Total commits | Loose objects | `.vault/.git` size | `show` | `log` | `status` |
|---------------|---------------:|---------------------:|-------:|------:|---------:|
| 100 | 300 | 1.26 MB | 20 ms | 12 ms | 11 ms |
| 1,000 | 2,800 | 11.0 MB | 18 ms | 12 ms | 10 ms |
| 5,000 | 13,000 | 52.0 MB | 25 ms | 16 ms | 10 ms |
| 20,000 | 50,000 | 200 MB | 31 ms | 24 ms | 9 ms |

(Object count is a little under the naive 3×commits because two `simulate_history` invocations in
a row happened to write identical content at the same loop index, and git's content addressing
deduped it — a benign artifact of the fixture, not of Vault.)

There is no `git gc`/repack anywhere in the codebase, and it shows: **~10KB of disk per commit for
under 100 bytes of actual unique content — roughly two orders of magnitude of loose-object
overhead**, growing without bound for the life of the vault. Encouragingly, real end-to-end CLI
latency (`show`/`log`/`status`, full process spawn included) stayed flat at 9–31ms through 20,000
commits — the `resolve_at` scan cost confirmed in dimension 1 hasn't yet become the dominant term
at this scale relative to fixed process overhead. Disk usage is the concern here, not latency —
yet.

## 7. Concurrent readers vs. writer — `scripts/stress/concurrency.sh`

Sustained writer (one file edited every 300ms for 20–25s) plus parallel readers cycling
`show`/`log`/`list`/`status`:

| Readers | Total CLI calls | p50 | p95 | p99 | max | Lock errors |
|---------|-------------------|-----|-----|-----|-----|-------------|
| 8 | 200 | 16 ms | 29 ms | 45 ms | 46 ms | none |
| 32 | 1,280 | 55 ms | 93 ms | 130 ms | 174 ms | none |

No `SQLITE_BUSY` or git lock errors at either scale. Latency grows with reader count, but that
tracks with OS process-scheduling overhead of running dozens of concurrent CLI processes, not
database or git lock contention. **Verdict: fine as-is** — `busy_timeout=5000` plus WAL mode is
doing its job.

## CI wiring

None of the above is wired into default CI (`scripts/ci.sh`) — the full sweep here took several
minutes wall-clock across all three criterion benches plus four stress scripts, dominated by
setup cost (writing tens of thousands of files, spinning up dozens of processes) rather than
anything worth gating a PR on. `cargo bench` and `scripts/stress/*.sh` stay manual profiling
tools, run on demand. If a smoke-scale regression guard is wanted later, it should use the
smallest size in each dimension's table above (fast enough to run every PR) rather than the full
sweep.
