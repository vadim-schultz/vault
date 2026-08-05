---
name: Shape refactor — Sandi Metz module split
overview: Pure-shape refactor triggered by git-housekeeping growing several files past healthy size. Split multi-type god-files into directory modules and extracted duplicated boilerplate. No behavior change.
todos:
  - id: rules
    content: Extend ~/.cursor/rules/code-structure.mdc with file-split, match-dispatch, guard-clause, and error-mapping helpers
    status: complete
  - id: housekeeping-split
    content: storage/housekeeping.rs → storage/housekeeping/{mod,fs,repack,marker}.rs
    status: complete
  - id: status-split
    content: app/status.rs → app/status/{mod,model}.rs; extract housekeeping_status_for
    status: complete
  - id: fakes-split
    content: adapters/fakes.rs → adapters/fakes/{mod,clock,meta_index,object_store,service,registry}.rs
    status: complete
  - id: daemon-split
    content: daemon.rs → daemon/{mod,guard,heartbeat,queue_snapshot}.rs
    status: complete
  - id: config-split
    content: config.rs → config/{mod,watcher,gc}.rs
    status: complete
  - id: git-split
    content: storage/git.rs → storage/git/{mod,worktree_cwd,tree_edit}.rs
    status: complete
  - id: cli-status-split
    content: cli/commands/status.rs → commands/status/{mod,render}.rs
    status: complete
  - id: helper-dedupe
    content: Dedupe process_task, handlers, watcher process_paths, sqlite optional_row/conn, paths state_file
    status: complete
  - id: ci-green
    content: ./scripts/ci.sh lint + cargo build --release green
    status: complete
isProject: false
---

# Shape refactor

**Status: implemented** on `refactor/shape-cleanup`.

## Problem

After git-housekeeping landed (PR #14), several files exceeded ~100 lines and held multiple independent type definitions. Several functions mixed concerns or repeated identical error-mapping boilerplate.

## What changed

| Before | After |
|--------|-------|
| `storage/housekeeping.rs` (630 LOC, 7 types) | `storage/housekeeping/{mod,fs,marker,repack}.rs` |
| `app/status.rs` (196 LOC, 6 DTOs) | `app/status/{mod,model}.rs` |
| `adapters/fakes.rs` (267 LOC, 5 fakes) | `adapters/fakes/{mod,clock,meta_index,object_store,service,registry}.rs` |
| `daemon.rs` (330 LOC, 4 types) | `daemon/{mod,guard,heartbeat,queue_snapshot}.rs` |
| `config.rs` (173 LOC, 3 types) | `config/{mod,watcher,gc}.rs` |
| `storage/git.rs` (414 LOC) | `storage/git/{mod,worktree_cwd,tree_edit}.rs` |
| `cli/commands/status.rs` (165 LOC) | `cli/commands/status/{mod,render}.rs` |

In-place helper extractions (no file split):

- `queue/mod.rs::process_task` → `mark_complete_finished` / `mark_failed_finished`
- `queue/handlers.rs` → `mismatch_counts`, `log_mismatch`, `log_repack`
- `watcher/mod.rs::process_paths` → `signal_if_registry_related`, `commit_routed_batches`, `signal_if_registry_changed`
- `storage/sqlite/mod.rs` → `conn()`, `optional_row<T>()`
- `paths.rs` → `state_file(name)`

## Rules codified

Extended `~/.cursor/rules/code-structure.mdc` with:

1. One file per independent type (~100 LOC guideline)
2. Match statements only dispatch
3. Guard clauses over wrapping ifs
4. Repeated error-mapping is a helper

## Verification

- All 102 unit tests + integration tests pass unchanged
- `cargo clippy -- -D warnings` green
- `cargo build --release` green
- Zero assertion text changes

## Non-goals

- `domain/queue.rs` constructor triplication (2 variants — rule of 3 not met)
- No new features or API changes
