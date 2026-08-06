---
name: Humanize `vault show` / `vault log` — scope levels and readable output
overview: Both read commands still leak internal plumbing at the user-facing layer — `vault log`
  prints raw git commit SHAs and, when unscoped, doesn't even report which files changed in each
  snapshot; `vault show` requires an exact file path and a `--at`, and only ever dumps raw bytes,
  with no way to see "what changed" across the vault or a subtree the way `git show` does for a
  commit. Direction from review: where a command has a direct git analog, stay close/identical to
  git's own output conventions (`--stat`-style diffstat, unified-diff hunks, pluralization) rather
  than inventing bespoke formats — strip only genuine plumbing (hashes, refs), not conventions git
  users already recognize. Scope is one-file/one-directory/whole-vault for `show`; multi-path
  `show` is explicitly out of scope. Includes a `scripts/showcase.sh` update, both fixing call
  sites the new `log` format breaks and adding sections that demonstrate the humanized output.
todos:
  - id: branch
    content: Sync main, create feat/humanize-cli-output branch
    status: completed
  - id: tdd-changeset-query
    content: "TDD: MetaIndex query returning every (path, event_kind) pair for one commit/snapshot_id — file_events is currently only ever fetched scoped to a single path (SELECT_SNAPSHOTS_FOR_PATH) or not fetched at all for unscoped log (SELECT_ALL_SNAPSHOTS). Contract test in ports/meta_index.rs's contract module + SqliteMetaIndex/queries.rs implementation. Powers both unscoped vault log and vault show's report mode"
    status: completed
  - id: tdd-previous-version-query
    content: "TDD: MetaIndex query for 'the commit_sha that last touched this path before snapshot_id X', so diff/diffstat rendering can fetch a file's prior content via the existing ObjectStore::read_blob without any git-parent-walking capability on the object store"
    status: completed
  - id: tdd-message-shared
    content: "TDD: extract src/app/snapshot.rs's snapshot_message/single_change_message/verb_for into a shared function callable from both commit-time (git message) and render-time (log/show header), so the two can never drift. Fix the multi-file fallback in the same pass: uniform-kind batches keep '{verb} {N} files', mixed-kind batches get 'change {N} files' instead of always claiming 'update'"
    status: completed
  - id: tdd-diffstat-render
    content: "TDD: diffstat rendering (' path | N +-' per file, 'N file(s) changed, X insertion(s)(+), Y deletion(s)(-)' summary), matching git --stat's own wording/pluralization exactly, computed via tdd-previous-version-query + ObjectStore::read_blob + similar::TextDiff line counts"
    status: completed
  - id: tdd-diff-extract
    content: "TDD: extract the unified-diff rendering in cli/commands/diff.rs (render_content_diff/as_utf8_pair) into a shared helper so vault diff, log --verbose, and show's report mode render diffs identically. Fix the binary-file message to git's literal wording ('Binary files a/<path> and b/<path> differ') instead of the current 'Binary files differ.'"
    status: completed
  - id: tdd-log-render
    content: "TDD: rewrite cli/commands/log.rs's rendering to git --stat shape — drop commit_sha, header from tdd-message-shared (RFC3339 timestamp preserved verbatim so it still copy-pastes into --at per docs/src/cli.md), diffstat lines from tdd-diffstat-render, blank line between commits like git log --stat"
    status: completed
  - id: tdd-log-verbose
    content: "TDD: under --verbose, swap log's diffstat lines for full unified-diff hunks per changed file (git log -p equivalent), reusing tdd-previous-version-query + tdd-diff-extract"
    status: completed
  - id: tdd-show-optional-path
    content: "TDD: make ShowArgs.path optional; disambiguate at runtime off the metadata index alone (no new ObjectStore capability): omitted -> whole-vault report; exact tracked/historical path -> today's raw content dump unchanged; strict prefix of tracked paths -> directory-scoped report; otherwise -> today's PathNotTrackedAt, unchanged"
    status: completed
  - id: tdd-show-report-mode
    content: "TDD: implement show's report-mode renderer (no path, or a directory path), matching plain git show <rev> — resolve the commit via existing resolve_commit, change-set via tdd-changeset-query (prefix-filtered for directories), header via tdd-message-shared, full diff per file via tdd-diff-extract always, no --verbose gate"
    status: completed
  - id: showcase-fix-existing
    content: "Fix scripts/showcase.sh call sites the new log format breaks: AT1_SHA capture (section 6, currently parses column 1 of vault log as a hash — resolve via `git --git-dir=.../. log --format='%H %s'` grepped by the captured timestamp instead), the wc -l commit-count checks in sections 3 and 10 (count header lines via a verb-prefix grep, not raw output lines — a single commit's block is now 3+ lines), the `grep -q modify` check in section 4 (Modify now displays as 'update'), and section 4's narration text describing the old create/modify/delete wording"
    status: completed
  - id: showcase-new-sections
    content: "Add new scripts/showcase.sh sections (before Final recap) demonstrating: vault log's git --stat shape vs. real `git log --stat` side by side, vault log --verbose vs. `git log -p`, vault show with no PATH (whole-vault report) vs. `git show <sha>`, and vault show <dir>/ (directory-scoped report)"
    status: completed
  - id: docs
    content: "Update docs/src/cli.md: new vault show levels + example, humanized vault log format, --verbose diff mention, and a line noting power users can inspect real git history directly via the vault's own git dir (VaultLayout::git_dir_path, .vault/git) rather than a --hash escape hatch on vault log itself"
    status: completed
  - id: docs-ci
    content: "CHANGELOG entry; add this chapter's row to .plans/cli/README.md and mark it landed; ./scripts/ci.sh lint green"
    status: completed
isProject: false
---

# Humanize `vault show` / `vault log`

## Problem

Both commands were built to prove the time-travel mechanics work (chapter 5), not to read well.
Concretely, today:

- `vault log` (`src/cli/commands/log.rs:44`) renders `{commit_sha} {created_at} {event}` — a raw
  git object id in every line, which is exactly the "internal plumbing" this plan is meant to hide.
- Worse: unscoped `vault log` (no path) calls `list_all_snapshots`
  (`src/storage/sqlite/mod.rs:139`), which only ever selects `commit_sha, created_at` —
  `file_events` isn't joined at all in that path. Run `vault log` today with no argument and you
  cannot tell what changed in any commit; you only get scoped-to-a-path event info via
  `vault log <path>`. This is a real gap, not just a cosmetics problem.
- `vault show` (`src/cli/commands/show.rs`) takes a *required* `PATH` plus `--at`, and always
  dumps raw file bytes to stdout. There is no "what changed across the vault at this time" view —
  that's conceptually adjacent to `vault log`/`vault diff` today, not to `show`.

## Design constraints

Per project convention (`CLAUDE.md`): implementation starts from a fresh branch off a synced
`main`, never on top of `main` directly or a stale branch — done (`feat/humanize-cli-output`,
branched from an up-to-date `main` pull).

Per TDD: every behavior unit in the todo list above is a red/green pair — a failing test written
against the intended interface first, then the minimum implementation to pass it. No behavior
lands without a test that failed for the right reason first.

Per Sandi Metz-style sizing (applied directionally, not as a hard linter rule — this repo doesn't
mechanically enforce it, but its existing modules already lean this way):

| Unit | Budget | Note |
|------|--------|------|
| Function body | ~5–12 lines, one job | Diffstat/message-building helpers should each do one thing (compute one line, look up one prior version) rather than one function producing a whole commit's report |
| Function parameters | ≤4 | The new `previous_commit_for`-style `MetaIndex` query needs care here — path + some notion of "before this point" is 2, but if a snapshot id, a `RelPath`, and an explicit ordering key all end up as separate params it's worth bundling into a small struct rather than growing past 4, the same call this repo already made for `WalkParams` in the file-size-warning chapter |
| File (production code, excluding `#[cfg(test)] mod tests`) | ≤~150 lines before it's worth splitting into a module directory | `cli/commands/log.rs` (55 lines today) and `show.rs` (37 lines today) will grow with `--stat`/report-mode rendering; if either crosses ~150, split rendering into its own file (e.g. `cli/commands/log/render.rs`), following the precedent `cli/commands/status/render.rs` already set rather than letting one file do parsing, dispatch, and rendering at once |
| Type | One reason to change | The shared message-builder function belongs where it can serve both a write-time caller (`app/snapshot.rs`, building the git commit message) and a read-time caller (`log`/`show` rendering) without either side dragging in the other's concerns — a small dedicated module, not bolted onto `snapshot.rs`'s existing single-purpose "commit this batch" responsibility |

## Direction: match git's own conventions, not a new bespoke format

Standing instruction for this plan: for the commands that are directly git-shaped (`log`, `diff`,
`show`), humanizing means hiding what a non-git user can't act on (commit SHAs, ref decorations,
`.vault/` internals) — it does *not* mean replacing git's own established output shapes with a new
invented one. Concretely:

- `vault diff` already matches `git diff`'s unified-diff format (`similar::TextDiff::unified_diff`)
  — keep that, and tighten the one place it currently diverges from git's literal wording (binary
  files — see Part 2).
- `vault log`'s default view should read like `git log --stat` (a per-commit summary header, an
  indented diffstat line per file, a totals line) with the hash/author/ref lines swapped for the
  one line vault already writes as the real commit message — not the flatter one-line-per-file
  format this plan originally proposed. That original proposal optimized for `grep`-ability over
  fidelity to the git convention the user's own illustration was drawn from; this revision reverses
  that call now that "stay close to git" is the explicit steer.
- `vault log --verbose` should read like `git log -p` — full unified-diff hunks per file, in
  addition to the same per-commit header.
- `vault show`'s new report mode should read like plain `git show <rev>` — which, unlike `git log`,
  shows full diffs *by default*, no `-p`/`--verbose` needed. `show` is already scoped to one
  resolved commit, so there's no "wall of diffs across history" problem `log`'s terse default
  exists to avoid.

## Part 1 — `vault show`: what scope levels make sense

The key thing to work out first is that `vault show` currently does one job — "cat this file as it
was at time T" — and that job is inherently single-file: raw bytes on stdout are only useful when
there's exactly one file's content being piped somewhere (`vault show README.md --at ... >
README.old.md`). Concatenating N files' raw bytes has no sensible consumer. So "show changes for
all files" isn't a bigger version of the same feature — it's a second, different feature
(a change *report*: which files, what kind of change, optionally a diff) that happens to share the
`--at` resolution logic.

Git already draws exactly this line: `git show <rev>` (no path) prints the commit's metadata + a
diff for every file it touched; `git show <rev>:<path>` prints one blob's raw content. Same command
name, two genuinely different output shapes, selected by whether a path was given. That's the
model worth copying:

| Level | Input | Output | Mechanism | Cost |
|-------|-------|--------|-----------|------|
| File (today) | `vault show a.md --at T` | raw bytes | unchanged — `resolve_commit` + `read_blob` | done |
| Whole vault | `vault show --at T` (no path) | `git show`-shaped report: header line + full unified diff per file touched by the resolved commit, always (no `--verbose` gate — see Direction above) | new: per-commit change-set query (`tdd-changeset-query`) | small |
| One directory | `vault show docs/ --at T` | same `git show`-shaped report, filtered to paths under that prefix | same query, filtered in Rust by prefix — no SQL/index change needed | small, layers on the above |
| Many files/dirs (`vault show a.md docs/ --at T`) | *out of scope for this plan* | — | not building this now |

The "one directory" tier is cheaper than it looks specifically because of how the report is built:
once the whole-vault case fetches the *full* per-commit change-set, directory scoping is just a
`starts_with` filter on that same in-memory list. There's no need to touch `file_events`' schema,
its `UNIQUE(snapshot_id, path)` shape, or its `idx_file_events_path_time` index (which is built for
exact-path lookups, not prefix scans) — a `LIKE 'dir/%'` query would have been the tempting but
more invasive route.

**Disambiguating what `PATH` means** doesn't need any new capability on `ObjectStore` either
(no "is this a blob or a tree at commit C" lookup). The metadata index already knows every path
ever tracked, so the rule can run entirely off it:

1. `PATH` omitted → whole-vault report.
2. `PATH` matches a tracked path exactly (ever, not just currently) → today's content dump,
   byte-for-byte unchanged. This preserves the existing scriptable contract — nothing that pipes
   `vault show`'s stdout today breaks.
3. `PATH` is a strict prefix of one or more tracked paths → directory-scoped report.
4. Otherwise → `VaultError::PathNotTrackedAt`, same as today.

Rule 2 before rule 3 matters: a file and a directory can't collide in a real tree, but a *deleted*
file's path might no longer be at its current on-disk location, so the check has to run against
history in the metadata index, not against the working tree.

**Decided: many discontiguous files/dirs (the "L3" row) is out of scope for this plan**, not just
a later phase of it. It's a strict generalization of directory scoping — a `Vec<String>` of
prefixes instead of one — so nothing above forecloses adding it later, but it also adds real
ambiguity to the CLI surface (does `vault show a.md b.md` mean "filter to these two" or
accidentally shadow the single-file content-dump case if someone passes one path today and a
typo'd second token tomorrow?) that isn't worth resolving without a concrete workflow asking for
it. Whole-vault, one-directory, and one-file are the full scope here.

**Decided: file-vs-directory disambiguation as specified above is good enough** — no trailing-slash
convention or `--dir` flag needed.

## Part 2 — humanizing `vault log`

Target format: `git log --stat` shaped, hash/author/ref lines swapped for vault's own commit
message, timestamp kept full-precision RFC3339 so `docs/src/cli.md`'s existing promise still
holds ("`vault log` prints exact RFC3339 timestamps... copy a line from `vault log` straight into
`--at`"):

```
update notes.md @ 2026-08-05T12:58:27.962477+00:00
 notes.md | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

restore notes.md @ 2026-08-05T12:58:15.669883+00:00
 notes.md | 1 +
 1 file changed, 1 insertion(+)

delete draft.md @ 2026-08-05T12:58:25.342921+00:00
 draft.md | 1 -
 1 file changed, 1 deletion(-)
```

The important realization here: **`snapshot_message`/`single_change_message`/`verb_for` in
`src/app/snapshot.rs:57-77` already generate exactly this header wording** — that's what actually
ends up in the real git commit message today (confirmed by the raw `git log` sample in the original
ask: `vault: restore notes.md @ 2026-08-05T12:58:27...` is that function's output verbatim, just
with a `vault: ` prefix and the hash git adds on top). So the header line for `log` shouldn't be a
second, independently-written formatter — it should call the same function the commit path already
calls (extracted so both call sites share it; see `tdd-message-shared`), guaranteeing the line
`vault log` prints for a commit can never drift from that commit's real git message.

**Decided: fix `snapshot_message`'s multi-file wording as part of this pass**, since the new
diffstat block makes the existing gap more visible, not less. Today its fallback always says
`"vault: update {N} files @ ..."` regardless of whether the batch actually mixed
creates/modifies/deletes — a batch of one modify and one delete currently gets mislabeled
"update 2 files." Fix: when every change in the batch shares one `FileEventKind`, keep the
existing `"{verb} {N} files"` wording; when kinds are mixed, fall back to a neutral
`"change {N} files"` instead of overclaiming "update." No need to enumerate per-kind counts in the
header itself — a create-only file's diffstat line is all `+`, a delete-only file's is all `-`, so
the per-file kind is already visible in the body, same as real `git --stat` never spells out
create/delete in its stat lines either (that only shows up in a full `git show`'s extended header).

The diffstat lines (`path | N ±`, `N file(s) changed, X insertion(s)(+), Y deletion(s)(-)`) are
computed at render time the same way git computes them — by diffing content, not from a stored
field. `tdd-previous-version-query` finds "the previous `snapshot_id` that touched this path"
straight out of the metadata index (which already orders `file_events` by `(path, snapshot_id)`),
`ObjectStore::read_blob` fetches both sides, and `similar::TextDiff` (already a dependency, already
used in `cli/commands/diff.rs`) supplies the line-level insertion/deletion counts. Same primitive
`vault diff` already leans on — no new capability, just reused at a new call site.

`--verbose` (already a global flag, currently only used by `vault init`'s diagnostic line) swaps
the diffstat lines for full unified-diff hunks — `git log -p`, not `git log --stat -p` — using the
same renderer `vault diff` already has (see `tdd-diff-extract`). One more small fidelity fix while
touching that renderer: its current binary-file message (`"Binary files differ.\n"`) doesn't match
git's literal wording (`Binary files a/<path> and b/<path> differ`) — worth aligning now that
"match git's own conventions" is the explicit standard for this pass, not just for the parts this
plan adds.

**Commit SHA dropped entirely**, still not moved behind a `--hash`/porcelain flag — that part of
the original plan stands. The project's own stated vision (`mvp/README.md`) is that `.vault/`
"uses standard `.git/` layout... inspectable without the tool" — the vault's git directory
(`VaultLayout::git_dir_path`, `.vault/git`) is already a real, ordinary git repo, so a power user
who wants the SHA already has `git --git-dir .vault/git log --stat` available verbatim (which, per
the Direction above, should now look nearly identical to `vault log`'s own output modulo the hash
line — a nice side effect of matching git's conventions this closely).

## Shared groundwork

`vault show`'s new report mode (Part 1) and `vault log`'s per-line rendering (Part 2) end up
needing the same two things: a per-commit change-set, and diff rendering keyed off "previous
version of this path." That's `git show` vs. `git log -p` again — same underlying data, one shows
a single commit in full, the other walks many. Concretely:

- `tdd-changeset-query` (new `MetaIndex` method) backs both unscoped `vault log` and `vault show`'s
  whole-vault/directory report.
- `tdd-message-shared` (extracted from `src/app/snapshot.rs`) backs the header line both places, so
  it's identical to the real commit message by construction, not by convention.
- `tdd-diff-extract` pulls the unified-diff rendering already written once for `vault diff`
  (`src/cli/commands/diff.rs`) into something both `log --verbose`/`log`'s diffstat lines and
  `show`'s report mode call, instead of a third copy growing next to it.

The one deliberate asymmetry: `log` stays `--stat`-terse by default and only goes full-diff under
`--verbose`, while `show`'s report mode is full-diff always. That mirrors real git (`git log` vs.
`git log --stat` vs. `git log -p` are three separate opt-ins; `git show` defaults straight to a
patch) — `log` walks potentially many commits so it needs a terse default, `show` is pinned to one
already-resolved commit so there's nothing to be terse about.

Building `show`'s report mode without also fixing unscoped `log`'s missing change-set data (or vice
versa) would mean writing the same query twice under different names — worth sequencing
`tdd-changeset-query` first, then landing both call sites against it.

## Implementation

| Step | File | What |
|------|------|------|
| Changeset query | `src/ports/meta_index.rs`, `src/adapters/sqlite.rs`, `src/storage/sqlite/{mod,queries}.rs` | New `MetaIndex` method + `SELECT` joining `file_events` to one `snapshot_id`/commit, returning every `(path, event_kind)` touched by that commit |
| Previous-version query | same files | New `MetaIndex` method: latest `file_events` row for a path with `snapshot_id` less than the given one, ordered by the existing `(path, snapshot_id)` index |
| Shared message builder | `src/app/snapshot.rs` → extracted to a new small module | `snapshot_message`/`single_change_message`/`verb_for` moved somewhere both `app::snapshot::commit` (write) and the `log`/`show` renderers (read) can call; mixed-kind fallback fixed to `"change {N} files"` |
| Diffstat renderer | new shared location (alongside the extracted diff renderer) | Per-file ` path \| N +-` line + trailing `N file(s) changed, ...` summary, built from `similar::TextDiff` line counts over (previous version, current version) |
| Diff renderer extraction | `src/cli/commands/diff.rs` → shared location | `render_content_diff`/`as_utf8_pair` become a shared helper; binary-file wording fixed to git's literal text |
| `vault log` default | `src/cli/commands/log.rs` | `render_line`/`render_report` rewritten to the `--stat` shape using the changeset query, shared message builder, and diffstat renderer; no `commit_sha` |
| `vault log --verbose` | `src/cli/commands/log.rs` | Diffstat lines swapped for the full diff renderer's output per file |
| `vault show` optional path | `src/cli/commands/show.rs`, `src/app/show.rs` | `ShowArgs.path: Option<PathBuf>`; disambiguation logic (exact / prefix / neither) against the metadata index |
| `vault show` report mode | `src/app/show.rs`, `src/cli/commands/show.rs` | New renderer: header (shared message builder) + full diff per file (shared diff renderer), always, for the whole-vault or directory-scoped cases |
| Docs | `docs/src/cli.md` | New `show` levels + example; humanized `log` format; `--verbose` note; power-user git-dir callout |
| Showcase | `scripts/showcase.sh` | Fix broken call sites; add sections demonstrating the new output (see below) |

## Showcase demo

`scripts/showcase.sh` narrates every subcommand against a disposable vault with a real watcher,
cross-checking vault's output against raw git/sqlite state — this chapter changes what `vault log`
prints, so two things are needed: fixing what the format change breaks, and adding sections that
actually demonstrate the new behavior (the same pattern `filesize_warning.plan.md` section 15 and
`git_housekeeping.plan.md` section 14 followed).

**What the new `log` format breaks, concretely:**

- Section 6/7's SHA capture (`showcase.sh:226-235`) reads `vault log`'s column 1 as a commit hash:
  `AT1_SHA="$("$VAULT_BIN" log notes.md | tail -n1 | awk '{print $1}')"`. Column 1 is now the verb
  (`update`/`delete`/`restore`), not a hash. Fix by resolving the SHA from real git instead,
  keyed off the timestamp vault's own commit message still contains verbatim:
  ```bash
  AT1_SHA="$(git --git-dir="$VAULT_DIR/.git" log --format='%H %s' --all | grep -F "$AT1" | awk '{print $1}')"
  ```
- The `AT1`/`AT2` timestamp capture itself (`tail -n1`/`head -n1 | awk '{print $2}'`) assumed one
  line per commit. A commit's block is now header + diffstat line(s) + a blank separator, so
  `head`/`tail` need to target header lines specifically:
  ```bash
  AT1="$("$VAULT_BIN" log notes.md | grep -E '^(update|delete|restore|change) ' | tail -n1 | awk '{print $NF}')"
  ```
- Section 3's and section 10's `wait_for` checks (`showcase.sh:199`, `:249`) count commits via
  `$(vault log notes.md | wc -l) -ge N` — now over-counts since each commit spans multiple lines.
  Replace with a header-line count: `$(vault log notes.md | grep -cE '^(update|delete|restore|change) ') -ge N`.
- Section 4's `wait_for` (`showcase.sh:204`) greps for `modify`: `'$VAULT_BIN' log draft.md | grep -q modify`.
  `FileEventKind::Modify` now displays as `update` in the header (per `verb_for`), so this becomes
  `grep -q update`. `showcase.sh:212`'s `grep -q delete` is untouched — `delete` is still `delete`.
- Section 4's narration (`showcase.sh:205-207`) describes the old raw `create`/`modify` wording
  that used to appear via `event.as_str()`. Rewrite it to explain the two-tier vocabulary instead:
  `file_events.event_type` still stores `create`/`modify`/`delete`/`restore` (visible via
  `inspect_sqlite`'s `file_events` dump, unchanged), while the humanized header groups
  `create`/`modify` under `update` — a good aside for the demo, not just a wording nit.

**New sections** (inserted before `Final recap`, following the existing numbering):

```bash
section "16. Humanized vault log -- git --stat shape, --verbose for full diffs"
vlt log notes.md
echo ""
echo "cross-check shape against real git log --stat (hash present there, absent above):"
git --git-dir="$VAULT_DIR/.git" log --stat notes.md
pause
echo "--verbose swaps the diffstat block for full unified-diff hunks, like git log -p:"
"$VAULT_BIN" --verbose log notes.md
pause

section "17. Humanized vault show -- whole-vault and directory report modes"
echo "no PATH: report for the resolved commit, full diff always (no --verbose needed):"
vlt show --at "$AT2"
echo ""
echo "cross-check against real git show:"
AT2_SHA="$(git --git-dir="$VAULT_DIR/.git" log --format='%H %s' --all | grep -F "$AT2" | awk '{print $1}')"
git --git-dir="$VAULT_DIR/.git" show "$AT2_SHA"
pause
echo "seed a subdirectory so directory-scoped show has something to filter to:"
mkdir -p sub
echo "sub file" >sub/child.md
wait_for "sub/child.md snapshot" bash -c "'$VAULT_BIN' log sub/child.md | grep -q update"
AT3="$("$VAULT_BIN" log | grep -E '^(update|delete|restore|change) ' | head -n1 | awk '{print $NF}')"
echo "a directory path scopes the same report to that subtree only:"
vlt show sub --at "$AT3"
pause
```

## Verification

- Contract tests for the two new `MetaIndex` methods (`ports/meta_index.rs`'s `contract` module,
  exercised against `SqliteMetaIndex`), matching the existing pattern for `list_snapshots`.
- Unit tests for the shared message builder: single create/modify/delete/restore messages
  unchanged; uniform-kind multi-file batches keep `"{verb} {N} files"`; mixed-kind batches produce
  `"change {N} files"`.
- Unit tests for the diffstat renderer: single-line insertion/deletion counts and pluralization
  (`1 file changed, 1 insertion(+)` vs. `2 files changed, 3 insertions(+), 1 deletion(-)`).
- Integration tests (`tests/log.rs`, `tests/show.rs`): unscoped `log` with a mixed-kind multi-file
  commit; no bare SHA anywhere in `log`/`show` output; `log --verbose` includes unified-diff hunks;
  `show` with no path and with a directory path; the disambiguation edge case where `PATH` is
  neither an exact tracked path nor a directory prefix still returns `PathNotTrackedAt`.
- Manual: run `scripts/showcase.sh` end to end after the fixes above land, confirm every section
  completes (no broken `wait_for`/SHA-capture step) and sections 16/17 visibly match their `git`
  cross-checks.

## Exit criteria

- [x] `vault log` (default) reads as `git log --stat` with the hash/author/ref lines replaced by
      vault's own commit message line; no commit SHA anywhere in default output
- [x] `vault log --verbose` reads as `git log -p` (full diff hunks, same header)
- [x] `vault show` with no `PATH`, or a directory `PATH`, prints a `git show`-shaped report
      (header + full diff per file) unconditionally; a single-file `PATH` is byte-for-byte
      unchanged from today
- [x] `snapshot_message`'s mixed-kind batches no longer say "update" when they aren't all updates
- [x] Binary-file diff wording matches git's literal text
- [x] `scripts/showcase.sh` runs clean end to end, including the two new sections
- [x] `./scripts/ci.sh lint` green
