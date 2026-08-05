---
name: Release Binary Size Reduction
overview: The release binary is 9.53 MB with no release-profile tuning and two dependencies (gix, tokio) pulling in far more than the code uses. Every number below was measured directly (build + `cargo test --release --lib`, 102/102 passing at each step), not estimated.
todos:
  - id: size-profile-tuning
    content: "Add [profile.release] tuning (strip, lto, codegen-units=1) — 9.53MB -> 6.58MB (with item 2), zero runtime regression confirmed via full bench suite. opt-level=\"z\" was tried too (-> 4.10MB) but caused broad runtime regressions (up to +135% on vault_count/registry_load) and was reverted — see RESULTS.md."
    status: completed
  - id: size-trim-gix-tokio-features
    content: "Trim gix to default-features=false + [\"tree-editor\", \"parallel\"], trim tokio to [\"rt-multi-thread\", \"macros\", \"sync\", \"time\", \"signal\"] — removes unused default features (credentials, mailmap, dirwalk, status, worktree-*, regex via revparse-regex; tokio net/fs/io-util/process/test-util)"
    status: completed
  - id: size-panic-abort-decision
    content: "Rejected: panic=\"abort\" (would save ~365KB further) vs current panic=\"unwind\" — daemon is designed to be fire-and-forget, and abort would crash the whole daemon process on any task panic instead of just failing that JoinHandle. The ~365KB isn't worth trading away that property. Keeping panic=\"unwind\"."
    status: rejected
  - id: size-dedup-git2-gix
    content: "Rejected: replacing git2's repack path with gix-pack to drop the duplicate git implementation (~2.4MB static archive). The user confirmed this duplication is a known, conscious tradeoff (\"unfortunate but a conscious decision, leave as is\") — not an oversight to fix. Not pursuing."
    status: rejected
isProject: false
---

# Release binary size reduction

## Context

`vault build --release` currently produces a 9.53 MB binary (`target/release/vault`) with
`Cargo.toml` carrying no `[profile.release]` section at all — Cargo's untuned defaults (no LTO,
16 codegen units, symbols retained, `opt-level = 3`) — and `gix`/`tokio` both declared with far
broader feature sets than the code exercises.

Every number in this plan came from actually building each candidate change on top of the current
`main` and measuring `target/release/vault`'s size, then running `cargo test --release --lib`
(102 tests) and, for the two landed items, a full `cargo bench` before/after runtime comparison
(see [RESULTS.md](RESULTS.md)) to confirm nothing regressed.

**Status: items 1–2 landed on `feat/release-binary-size`, items 3–4 rejected.** See
[RESULTS.md](RESULTS.md) for the runtime verification, including one important course-correction:
`opt-level = "z"` was initially planned as part of item 1 but caused broad runtime regressions
(up to +135% on daemon hot paths) and was dropped before landing — the shipped config is `lto`,
`codegen-units = 1`, `strip` only, `opt-level` left at its default.

## Measured results

| Change (cumulative) | Binary size | Δ from baseline |
|---|---|---|
| baseline (today, no profile tuning) | 9.53 MB | — |
| + `strip = true` | 7.76 MB | −18.5% |
| + `lto = true`, `codegen-units = 1` | 6.68 MB | −29.9% |
| + trim `gix`/`tokio` features (**landed config**) | 6.58 MB | −30.9% |
| + `opt-level = "z"` (tried, reverted — runtime regression) | 4.10 MB | −57.0% |
| + `panic = "abort"` (rejected — fire-and-forget) | 3.74 MB | −60.8% |

All rows compiled clean and passed the full `--lib` test suite (102/102) on this machine
(`arm64-apple-darwin`, rustc via the pinned toolchain). The landed config additionally passed a
full `cargo test --release` (unit + integration) and a full `cargo bench` runtime comparison
against baseline with no regression — see [RESULTS.md](RESULTS.md).

## Priority order

Ranked by risk, not by size impact — the two zero-risk items land first regardless of how the
deferred items are eventually decided:

1. **Profile tuning** — landed as `lto`/`codegen-units = 1`/`strip` only, after `opt-level = "z"`
   was tried and reverted for a broad runtime regression (see RESULTS.md). 9.53 MB → 6.58 MB with
   item 2, confirmed no bench regression.
2. **`gix`/`tokio` feature trimming** — landed; smaller size delta than expected once profile
   tuning is in place (~100 KB), but a real win on compile time and dependency/audit surface; zero
   risk.
3. **`panic = "abort"`** — rejected (see below): contradicts the daemon's fire-and-forget design
   goal, not worth the ~365 KB.
4. **`git2`/`gix` de-duplication** — rejected: known, conscious tradeoff, not pursuing.

## 1. Profile tuning — landed

**Measured:** see table above. `strip`, `lto`, and `codegen-units = 1` combine to take the binary
from 9.53 MB to 6.68 MB (6.58 MB with item 2's feature trims) with no measurable runtime cost —
full `cargo bench` before/after came back within normal criterion noise (±10%, no consistent
direction) across `file_count`, `history_depth`, `vault_count`, and `queue_latency`.

`opt-level = "z"` was tried on top of this (→ 4.10 MB, the single largest individual size lever
tested) on the assumption that the binary's hot paths are file I/O, git object writes, and SQLite
rather than tight CPU loops. **That assumption was wrong for the daemon's hot-reload path**: the
same before/after bench comparison at `opt-level = "z"` showed broad, consistent regressions —
`vault_count/registry_load/10000` +135%, `vault_count/router_from_registry/2000` +70%,
`queue_latency/enqueue` +48%, `history_depth/resolve_at` +45–52% across all N. These are exactly
the paths that run on every daemon hot-reload tick and every `show`/`diff`/`restore`. `opt-level`
was reverted to its release default (`3`); see [RESULTS.md](RESULTS.md) for full numbers.

**Landed:**

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
```

**Verification:** `cargo build --release` (6.58 MB with item 2 applied), `cargo test --release`
(unit + all integration suites, full pass), full `cargo bench` before/after — no regression beyond
noise. See [RESULTS.md](RESULTS.md).

## 2. Trim `gix`/`tokio` feature flags — landed

**Measured:** `cargo tree -e features -i gix` on today's `Cargo.toml` shows the full `default`
feature set active — `extras` (`worktree-stream`, `worktree-archive`, `revparse-regex` which pulls
in the `regex` crate, `mailmap`, `excludes`, `attributes`, `worktree-mutation`, `credentials`,
`interrupt`, `status`, `dirwalk`), plus `comfort`, `basic` (`blob-diff`, `revision`, `index`) —
because `gix = { features = ["tree-editor"] }` never sets `default-features = false`. Grepping
`src/storage/git.rs` and `src/adapters/mod.rs` (the only two files that touch `gix::`) shows usage
limited to `gix::open`, `gix::create::into`, blob/tree writes via the tree editor, `commit_as`,
`head_commit`, and `find_commit` — none of the default-feature functionality above.

Likewise, `tokio = { features = ["full"] }` enables `net`, `fs`, `io-util`, `io-std`, `process`,
`test-util`; grepping all `tokio::` usage in `src/` shows only `rt`/`rt-multi-thread` (via
`#[tokio::main]`), `macros` (`#[tokio::main]`, `#[tokio::test]`, `select!`), `sync` (`mpsc`,
`watch`, `Notify`), `task` (`spawn`, `spawn_blocking`, `JoinHandle`), `time` (`sleep`), and
`signal` (`ctrl_c`).

**Landed:**

```toml
gix = { version = "0.73.0", default-features = false, features = ["tree-editor", "parallel"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "signal"] }
```

Note: `parallel` is required, not optional — without it, `gix::config::Cache` falls back to
`once_cell::unsync::OnceCell`, and `gix::Repository` (and therefore `GitStore` /
`GixObjectStore`) loses `Send`/`Sync`, which fails to compile against `ObjectStore: Send + Sync`
in `src/ports/object_store.rs`. Confirmed by hitting this exact compile error and fixing it during
measurement.

**Verification:** `cargo build --release` compiles clean, `cargo test --release` full pass (unit +
integration), `cargo tree -e features -i gix` shows the trimmed set, full `cargo bench`
before/after shows no regression (see [RESULTS.md](RESULTS.md)).

## 3. `panic = "abort"` — rejected

**Measured:** would add ~365 KB of additional savings on top of item 1+2 (6.58 MB baseline for
this comparison → 4.10 MB at `opt-level = "z"` alone → 3.74 MB with `panic = "abort"` added).

**Why this isn't bundled with item 1:** `panic = "abort"` is a config one-liner but not a
config-only *behavior* change. `src/daemon.rs`, `src/queue/mod.rs`, `src/watcher/mod.rs`, and
`src/cli/support.rs` all use `tokio::spawn` / `tokio::task::spawn_blocking` for background work
(`worker::commit_batch`, `handlers::run`, queue draining), and grepping the codebase for
`catch_unwind` returns nothing. Today, a panic inside one of those tasks fails just that
`JoinHandle` and (depending on the caller) can be logged and recovered from. Under `panic =
"abort"`, the same panic **terminates the entire daemon process** — a background service that's
meant to keep running unattended.

**Decision: rejected.** The daemon is explicitly designed to be fire-and-forget — it should keep
running unattended even if one background task hits a bug, not take the whole process down with
it. `panic = "abort"` directly contradicts that goal, and ~365 KB isn't worth trading it away for.
Keeping `panic = "unwind"` (the default). This also means the current lack of `catch_unwind`
around spawned tasks is *more* worth revisiting on its own — under `unwind`, a panicking task
still fails its `JoinHandle` silently unless something is watching for it — but that's a
reliability question for the daemon/queue streams, not this binary-size plan.

## 4. `git2` / `gix` de-duplication — rejected

**Measured:** `git2` (`vendored-libgit2`) is used in exactly one file — `src/storage/housekeeping.rs`
— for `Repository::open_bare`, `revwalk`, and `packbuilder` (building a pack file from all objects
reachable from `HEAD` during repack). Every other git operation in the codebase goes through `gix`.
`git2`'s own feature flags are already minimal (`default-features = false`, only
`vendored-libgit2` — no `https`/`ssh`/`cred`), so there's no flag-level trim available there. The
vendored libgit2 C library compiles to a ~2.4 MB static archive (`libgit2.a` in
`target/release/build/libgit2-sys-*/out/build/`) in release builds — a large dependency for the
single narrow slice of libgit2's API surface actually used.

This was raised as a de-duplication opportunity — replacing `write_pack_from_head`'s
revwalk+packbuilder logic with `gix-pack`'s pack-writing API would let `git2`/`libgit2-sys` be
dropped entirely.

**Decision: rejected.** Confirmed with the user: shipping both `git2` and `gix` is a known,
conscious tradeoff — "unfortunate but a conscious decision, leave as is" — not an unnoticed
architectural gap. `housekeeping::repack` is a data-integrity-sensitive path (it deletes loose
objects and old pack files after verifying the new pack), and the existing `git2`-based
implementation is the trusted, tested one; rewriting it onto `gix-pack` for a static-archive size
win isn't worth reopening that risk. Not pursuing.

## Exit criteria

- [x] Items 1 and 2 landed on `feat/release-binary-size`: `target/release/vault` at 6.58 MB
  (−30.9% from 9.53 MB baseline), `cargo test --release` full pass, `cargo bench` before/after
  shows no runtime regression — see [RESULTS.md](RESULTS.md).
- [x] Item 3 is closed: `panic = "abort"` rejected, keeping `panic = "unwind"` — contradicts the
  daemon's fire-and-forget design goal.
- [x] Item 4 is closed: `git2`/`gix` de-duplication rejected — known, conscious tradeoff, leave as
  is.
- [x] `opt-level = "z"` course-correction: tried as part of item 1, caused broad runtime
  regressions on daemon hot paths, reverted before landing — recorded in RESULTS.md so it isn't
  re-proposed without re-litigating why.

This plan is fully closed — no items remain open.
