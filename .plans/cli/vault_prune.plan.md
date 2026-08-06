---
name: Add `vault prune` — manual registry cleanup for missing vaults
overview: "`registry.toml` accumulates an entry every time `vault init` runs anywhere (including
  scratch/temp directories), and once a registered root is deleted the entry lingers forever as
  `[missing]` in `vault status` output. The only existing cleanup path
  (`VaultRegistry::prune_stale`, wired through `app::prune::prune`) is invoked exclusively inside
  the background daemon's watcher, reactively, only when it detects `registry.toml`'s mtime
  change — so a user whose daemon is stopped (or who just hasn't touched another vault since) has
  no way to clear stale entries at all. This is the direct git analogue of `git worktree prune`:
  git also leaves stale linked-worktree metadata around after a worktree directory is deleted by
  hand, and exposes an explicit prune command rather than only cleaning up as a side effect of
  other operations. Add the same escape hatch here: a `vault prune` subcommand that calls the
  existing, already-tested `prune_stale` logic directly and reports what it removed."
todos:
  - id: branch
    content: Sync main, create feat/vault-prune branch
    status: completed
  - id: tdd-registry-return-paths
    content: "TDD: `VaultRegistry::prune_stale` (src/registry.rs:123-131) currently returns
      `Result<usize, VaultError>` — just a count. Change it to `Result<Vec<PathBuf>, VaultError>`,
      returning the roots that were removed (count is `.len()`), so the CLI command can print
      *which* vaults it cleaned up rather than just a number. Update the `RegistryStore` trait
      (src/ports/registry.rs:22), the `TomlRegistry` adapter (src/adapters/toml_registry.rs:26-29),
      and `app::prune::prune` (src/app/prune.rs:11-13) to match. `daemon::prune_registry`
      (src/daemon/mod.rs:129-131) and both call sites in src/watcher/mod.rs (lines 121, 149)
      already discard the return value (`let _ = ...`) and need no change beyond compiling"
    status: completed
  - id: tdd-registry-unit-tests
    content: "Update src/registry.rs's prune_stale_removes_missing_roots test and
      app/prune.rs's removes_missing_roots test to assert on the returned Vec<PathBuf> (contains
      the expected root) instead of a bare length, while still covering the zero-removed case
      (empty Vec, registry.toml untouched — prune_stale already skips the save() call when nothing
      was removed, keep that optimization)"
    status: completed
  - id: cli-command
    content: "New `src/cli/commands/prune.rs`: `Command::Prune` variant in src/cli/mod.rs (no
      args, alongside Status/List), dispatched to `commands::prune::run().await`. Implementation
      loads TomlRegistry directly (matching status::run's run_blocking(status::report_default)
      pattern in src/cli/commands/status/mod.rs), calls app::prune::prune, and prints either
      'No missing vaults to prune.' (empty result) or a header line ('Removed N missing vault(s):')
      followed by one indented path per line — deliberately mirroring vault status's own
      `[missing]` listing so the two commands read as a matched pair"
    status: completed
  - id: cli-integration-test
    content: "New tests/prune.rs: register two vault roots (one real tempdir, one path that is
      then removed via fs::remove_dir_all before running the command — reuse the VAULT_STATE_DIR
      env-pointing pattern other registry tests use), run `vault prune` via assert_cmd, assert the
      missing root's path appears in stdout and the real vault's path does not, and that a second
      `vault prune` run afterward prints the 'No missing vaults to prune.' message. Do not touch
      the daemon's own reactive pruning path — this test only exercises the new manual command"
    status: completed
  - id: docs
    content: "docs/src/cli.md: add `prune` | Implemented | - row to the Subcommands table
      (~line 24) and a `### vault prune` section (after `### vault status`, ~line 51) describing
      what it does and giving the sample output shape. Also add one sentence to the existing
      'Not in v0.1' line (~line 207) clarifying that 'retention/prune policies' there refers to
      snapshot/history retention, not registry cleanup, to avoid the two being conflated now that
      a `prune` command exists"
    status: completed
  - id: changelog
    content: "CHANGELOG.md Unreleased/Added entry for `vault prune`, cross-referencing that
      registry pruning previously only ran inside the daemon (per the existing 'vault status is
      read-only' Changed entry from the prior release)"
    status: completed
  - id: ci
    content: "./scripts/ci.sh lint green"
    status: completed
isProject: false
---

# Add `vault prune`

## Problem

Reported directly by a user running `vault status` day-to-day (`.plans` self-hosted vault):

```
Vaults: 3
  /private/var/folders/vk/.../tmp.lo9YTvXOjT [missing] last snapshot: never
  /private/tmp/claude-502/vault-mvp-check [missing] last snapshot: never
  /Users/vadim/projects/vault/.plans [ok] last snapshot: 2026-08-06T05:29:55.322674+00:00
```

Both `[missing]` entries are vaults that were `vault init`'d in throwaway locations (an OS temp
dir, a scratch check directory) that have since been deleted by something else. `registry.toml`
still lists them because nothing has gone back to remove the entries — there's no way to do that
today:

- `vault status` (`src/app/status/mod.rs:19`) is explicitly read-only, per its own doc comment and
  the CHANGELOG entry that introduced it ("`vault status` is read-only — registry pruning moved to
  daemon reload").
- The actual removal logic, `VaultRegistry::prune_stale` (`src/registry.rs:123-131`), only runs
  inside the background daemon, and only reactively — triggered when the watcher notices
  `registry.toml`'s mtime change (`src/watcher/mod.rs:121,149` → `daemon::prune_registry`,
  `src/daemon/mod.rs:129-131`). If the daemon is stopped (as in the reported case — `vault status`
  showed `Daemon: stopped`), or simply hasn't been prompted to reload since the stale entry
  appeared, nothing prunes it.
- The CLI has no `prune`/`forget`/`remove`/`unregister` subcommand. The original chapter 4 plan
  (`.plans/mvp/chapters/chapter_4.plan.md:9`) scoped "register/unregister/prune" but only
  register + prune landed; unregister was dropped and never replaced with a manual escape hatch.

This is the same shape of problem `git worktree` has: `.git/worktrees/<name>/` metadata survives a
worktree directory being deleted by hand, `git worktree list` marks it `prunable`, and nothing
cleans it up automatically — you run `git worktree prune`. Vault should offer the equivalent.

## Design

Add `vault prune`, a thin CLI wrapper around the registry's existing, already-tested
`prune_stale` logic — no new pruning behavior, just a manual trigger and human-readable output.

**Return the removed paths, not just a count.** `prune_stale` currently returns
`Result<usize, VaultError>`. Changing it to `Result<Vec<PathBuf>, VaultError>` lets `vault prune`
tell the user *which* vaults it cleaned up, matching how `vault status` already names each vault by
full path. This threads through the `RegistryStore` trait and its `TomlRegistry` adapter; the two
existing daemon call sites discard the return value today (`let _ = daemon::prune_registry();`)
and are unaffected beyond recompiling.

**Output shape**, mirroring `vault status`'s own `[missing]` listing so the two commands read as a
pair:

```
$ vault prune
Removed 2 missing vault(s):
  /private/var/folders/vk/.../tmp.lo9YTvXOjT
  /private/tmp/claude-502/vault-mvp-check

$ vault prune
No missing vaults to prune.
```

**No new flags.** No `--dry-run` — `vault status` already shows exactly which entries are
`[missing]` before you run `prune`, so a preview mode would just duplicate that. No `--force` or
confirmation prompt — deleting a registry entry only forgets that vault ever existed to the
daemon/CLI; it does not touch the vault's own `.vault/` data (which is already gone, since the
whole root is missing) or anything under a still-existing root. This mirrors `git worktree prune`
also running without confirmation.

**Daemon behavior is unchanged.** The reactive prune-on-reload path in `src/watcher/mod.rs` keeps
working exactly as today; `vault prune` is purely an additional, manual way to invoke the same
underlying `app::prune::prune` use-case immediately, for the (common) case where the daemon isn't
running or hasn't reloaded.

## Files touched

| Area | File | Change |
|------|------|--------|
| Core | `src/registry.rs` | `prune_stale`: `Result<usize, _>` → `Result<Vec<PathBuf>, _>` |
| Port | `src/ports/registry.rs` | `RegistryStore::prune_stale` signature update |
| Adapter | `src/adapters/toml_registry.rs` | `TomlRegistry::prune_stale` signature update |
| Use-case | `src/app/prune.rs` | `prune()` signature update |
| CLI | `src/cli/mod.rs` | New `Command::Prune` variant + dispatch |
| CLI | `src/cli/commands/prune.rs` (new) | `run()` — load registry, prune, render report |
| Tests | `src/registry.rs`, `src/app/prune.rs` (`#[cfg(test)]`) | Assert on returned paths instead of count |
| Tests | `tests/prune.rs` (new) | End-to-end: missing root removed, present root kept, idempotent second run |
| Docs | `docs/src/cli.md` | Subcommands table row + `### vault prune` section + "Not in v0.1" clarification |
| Changelog | `CHANGELOG.md` | Unreleased/Added entry |

`daemon::prune_registry` and its two `watcher/mod.rs` call sites need no logic change — only to
keep compiling against the new return type.

## Verification

- Unit: `prune_stale` returns the removed roots (not just their count) for a mix of missing and
  present entries, and returns an empty `Vec` (with no `save()` call) when nothing is stale.
- Integration: `tests/prune.rs` — a registered-but-deleted root is reported and removed; a
  registered-and-present root is left alone and not mentioned; running `vault prune` again reports
  nothing left to prune.
- Manual repro check: reproduce the reported scenario (two vaults registered in now-deleted temp
  dirs) and confirm `vault prune` removes both and `vault status` afterward no longer lists them.
- `./scripts/ci.sh lint` green.

## Exit criteria

- [x] `vault prune` removes registry entries whose root no longer exists and reports each by path
- [x] Running `vault prune` with nothing to prune prints a clear no-op message instead of silence
- [x] `vault status` no longer lists a vault immediately after `vault prune` removes it
- [x] `docs/src/cli.md` documents the new subcommand
- [x] `CHANGELOG.md` records the addition
- [x] `./scripts/ci.sh lint` green
