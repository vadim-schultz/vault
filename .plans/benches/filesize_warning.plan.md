---
name: File Size Warning — Surface the Silent Skip
overview: "Implement dimension-3 fix from the benchmark remediation plan: files over max_file_bytes are correctly excluded from every snapshot but nothing tells the user — vault status gains a visible, enumerated line for currently oversized files. Scoped as a draft for review; nothing here is implemented yet, same convention as optimize.plan.md before it."
todos:
  - id: branch
    content: Sync main, create feat/filesize-warning branch
    status: completed
  - id: tdd-walk-red
    content: "Red: walk.rs tests for collect_oversized — file over max_file_bytes is found; file under the limit and an ignore-matched oversized file are both excluded"
    status: completed
  - id: tdd-walk-green
    content: "Green: implement collect_oversized + collect_oversized_from_root + oversized_entry in walk.rs; refactor collect_from_watch_root/file_change_from_entry to take the new WalkParams struct so every walk fn stays at <=4 params"
    status: completed
  - id: tdd-status-red
    content: "Red: app/status tests — StatusReport for a fixture vault with one oversized file lists it; a vault with none has an empty list; a vault whose root no longer exists returns empty without erroring"
    status: completed
  - id: tdd-status-green
    content: "Green: add oversized: Vec<RelPath> to VaultStatus (model.rs), wire oversized_for(entry, &layout) into vault_status_for_entry (mod.rs) alongside housekeeping_status_for"
    status: completed
  - id: tdd-render-red
    content: "Red: render.rs test — Display for VaultStatus includes an 'oversized (N not tracked)' line with paths when non-empty, omits it entirely when empty"
    status: completed
  - id: tdd-render-green
    content: "Green: extend impl fmt::Display for VaultStatus with the oversized block, following the existing housekeeping block's pattern"
    status: completed
  - id: manual-verify
    content: Manual — drop an 11MB file into a watched vault, confirm vault status surfaces it and vault list still omits it
    status: completed
  - id: showcase-section
    content: Add scripts/showcase.sh section 15 for a live file-size-warning demo (drafted below), inserted before "Final recap"
    status: completed
  - id: docs-ci
    content: CHANGELOG entry; mark opt-filesize-warning completed in optimize.plan.md; add this chapter's row to .plans/benches/README.md; ./scripts/ci.sh lint green
    status: completed
isProject: false
---

# File size — surface the silent skip

## Context

[optimize.plan.md](optimize.plan.md) scoped this as dimension 3, ranked "smallest fix here… cheap
to close" — the skip already happens correctly in `walk.rs`, this chapter only makes it visible.
[RESULTS.md](RESULTS.md) § 3: files over `max_file_bytes` (default 10MB) never appear in
`vault list`, and nothing — not `vault status`, not a log line, not an exit code — indicates why.

**Implemented.** See exit criteria below for verification status.

## Design constraints

Per project convention (`CLAUDE.md`): implementation starts from a fresh branch off a synced
`main`, never on top of `main` directly or a stale branch.

Per TDD: every behavior unit below is a red/green pair — a failing test written against the
intended interface first, then the minimum implementation to pass it. No behavior lands without
a test that failed for the right reason first.

Per Sandi Metz-style sizing (applied directionally, not as a hard linter rule — this repo doesn't
mechanically enforce it, but its existing modules already lean this way):

| Unit | Budget | Note |
|------|--------|------|
| Function body | ~5–12 lines, one job | The repo's own idiom (e.g. `collect_baseline_changes`) already runs a little past a literal 5-line cap; the target is "single responsibility, scannable in one glance," not a hard line count |
| Function parameters | ≤4 | `collect_from_watch_root` and `file_change_from_entry` are already at 4–5; adding a second near-identical walk function without addressing this doubles the smell, so this chapter includes a small refactor (below) rather than copy-pasting the problem |
| File (production code, excluding `#[cfg(test)] mod tests`) | ≤~150 lines before it's worth splitting into a module directory | `walk.rs` is ~70 lines of production code today; this chapter adds ~35–40, landing around 105–115 — still one file, no `walk/` directory split needed yet |
| Type | One reason to change | `VaultStatus` gains one new homogeneous field (`Vec<RelPath>`), not a wrapper struct — unlike `housekeeping`, this data has no "not yet available" state worth modeling as `Option`, so no `VaultOversizedStatus` type is introduced |

## Design decisions

**Reuse `RelPath`, not `PathBuf`, for the new field.** `collect_oversized` walks the same way
`collect_baseline_changes` does and naturally produces `RelPath`s; converting to `PathBuf` for
display would be a needless step with no consumer that needs it.

**No new status wrapper type.** `housekeeping: Option<VaultHousekeepingStatus>` exists because
housekeeping data requires `meta_db_path().is_file()` and bundles two related pieces (counts +
last repack). Oversized detection needs neither — it's a pure filesystem walk against
`config.toml`, gated only by `entry.root.is_dir()` (already a field on `VaultStatus`). A bare
`Vec<RelPath>` (empty when none, or when the root is gone) is the right size for what this is.

**Extract `WalkParams` to hold `collect_from_watch_root`/`file_change_from_entry` (existing) and
`collect_oversized_from_root`/`oversized_entry` (new) to ≤4 params each:**

```rust
struct WalkParams<'a> {
    worktree: &'a Path,
    matcher: &'a IgnoreMatcher,
    max_file_bytes: u64,
}
```

Both the baseline walk and the oversized walk need the same three things per entry (worktree root
for relativizing, the ignore matcher, the size ceiling) and differ only in what they *do* with an
entry that trips the size check — one drops it, the other collects it. Bundling the shared
context is the minimal fix; it is not a speculative abstraction; it is only being introduced
because a second call site with the same param list is being added in this chapter, not in
anticipation of a third.

## Implementation

| Step | File | What |
|------|------|------|
| Detection | `src/walk.rs` | `WalkParams` struct; `collect_oversized(layout, config) -> Result<Vec<RelPath>, VaultError>`, mirroring `collect_baseline_changes`'s shape; `collect_oversized_from_root` + `oversized_entry` walk helpers; refactor `collect_from_watch_root`/`file_change_from_entry` onto `WalkParams` in the same pass so both walks share one shape |
| Model | `src/app/status/model.rs` | `oversized: Vec<RelPath>` field on `VaultStatus`, next to `housekeeping` |
| Wiring | `src/app/status/mod.rs` | `oversized_for(entry: &VaultEntry, layout: &VaultLayout) -> Result<Vec<RelPath>, VaultError>` — returns `Ok(vec![])` when `!entry.root.is_dir()`, otherwise loads `VaultConfig` and calls `walk::collect_oversized`; called from `vault_status_for_entry` alongside `housekeeping_status_for` |
| Render | `src/cli/commands/status/render.rs` | Extend `impl fmt::Display for VaultStatus`: when `self.oversized` is non-empty, print `"    oversized ({n} not tracked):"` followed by one indented line per path (`path.as_str()`); nothing printed when empty |

## Showcase demo

`scripts/showcase.sh` narrates every subcommand against a disposable vault, real watcher
included — `git_housekeeping` got its own live demo there as section 14
(`git_housekeeping.plan.md`'s `showcase-section` todo). This chapter gets section 15, inserted
before `Final recap`, following the same pacing (a `section` header, real `vlt` calls, a `pause`).

Unlike the queue-backed demos (13, 14), there's nothing to poll for here: `collect_oversized`
walks the filesystem live on every `vault status` call — no daemon round-trip, no debounce
window to wait out. The demo can go straight from writing the file to calling `status`; the
`sleep "$DEBOUNCE_WAIT"` below exists only to make the point that waiting doesn't change the
outcome (it isn't "not committed yet," it's "never going to be").

```bash
section "15. File size limit — oversized files are skipped, but vault status says so"
echo "writing a 11MB file (default max_file_bytes is 10MB)"
dd if=/dev/zero of=huge.bin bs=1M count=11 2>/dev/null
sleep "$DEBOUNCE_WAIT"
echo "huge.bin was never committed -- confirm it's absent from list and the git tree:"
vlt list
inspect_git
echo ""
echo "vault status enumerates it instead of staying silent about the skip:"
vlt status
pause
```

## Verification

- Unit tests in `walk.rs` for `collect_oversized`: file over the limit found; file under the
  limit excluded; ignore-matched file excluded even if oversized (matches existing
  `collect_baseline_changes` coverage shape).
- Integration test in `app/status`: a fixture vault with a mix of normal and oversized files
  produces the right `StatusReport`; a vault whose root has been deleted returns an empty list,
  not an error.
- `render.rs` test: `Display` output includes the new block only when `oversized` is non-empty.
- Manual: drop an 11MB file into a watched vault, confirm `vault status` reports it and
  `vault list` still omits it (no behavior change to what's tracked — visibility only).

## Exit criteria

- [x] `collect_oversized` correctly enumerates over-limit, non-ignored files; existing
      `collect_baseline_changes` behavior unchanged after the `WalkParams` refactor
      (all existing `walk.rs` tests still green)
- [x] `vault status` shows a clear count + path list when files are skipped; nothing extra printed
      when none are
- [x] No change to what gets tracked — this chapter is visibility only
- [x] Every new/touched function ≤4 parameters; `walk.rs` production code stays under ~150 lines
- [x] `./scripts/ci.sh lint` green
