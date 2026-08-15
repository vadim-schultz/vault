---
name: "`vault reindex` — rebuild meta.db from .git history"
overview: "`vault_init_idempotent.plan.md` deliberately left one case erroring: a vault missing
  only `meta.db`, with `.git` fully intact. That plan's non-goal named the real fix — replay
  `.git`'s commit history back into a fresh `meta.db` — as legitimate future work, different scope.
  This plan is that follow-up: a new `vault reindex` command that reconstructs `meta.db` by
  walking `.git`'s commit log and diffing each commit's tree against its parent, the same
  derived-cache-from-source-of-truth relationship git itself uses for the pack `.idx` file and the
  `commit-graph` cache. `meta.db` is provably pure index over `.git` — nothing downstream ever
  reads it to decide what to write to git, only the reverse (`app::snapshot::commit` writes git
  first, indexes second, `src/app/snapshot.rs:44-56`) — so this is real, safe, deterministic
  reconstruction, not data recovery under uncertainty."
todos:
  - id: branch
    content: "Sync main, create feat/vault-reindex branch (done for this plan doc; implementation
      picks up on the same branch)"
    status: completed
  - id: tdd-tree-diff
    content: "New `src/storage/git/tree_diff.rs`, sibling to `tree_edit.rs`
      (src/storage/git/tree_edit.rs) and its logical inverse: given two `gix::Tree` handles (old,
      new), return `Vec<FileChange>` classifying every path as `Create` (in new only), `Modify`
      (in both, differing blob oid), or `Delete` (in old only). Implement by hand-walking entries
      recursively (`Tree::iter()` + recurse into subtrees on name match), mirroring
      `tree_edit.rs`'s existing manual style — do NOT reach for gix's `diff` cargo feature; `gix`
      is pinned with `default-features = false, features = [\"tree-editor\", \"parallel\"]`
      (Cargo.toml:18) specifically to keep the release binary small
      (.plans/release/binary_size.plan.md), and pulling in a new feature for this is an
      avoidable regression against that budget when a manual recursive diff needs nothing new.
      Assume the invariant the write path already guarantees: a `RelPath` is always a blob leaf,
      never a directory, so a path can never be a blob in one tree and a subtree in another —
      `debug_assert!`/error defensively if that invariant is ever violated (e.g. a hand-edited
      `.vault/.git`) rather than silently misclassifying. Unit tests: create/modify/delete
      detected correctly across nested paths; unchanged paths excluded; empty-tree-as-old (root
      commit) treats everything as `Create`"
    status: pending
  - id: tdd-history-walk
    content: "`src/storage/git/mod.rs`: new `GitStore::walk_history(&self) -> Result<Vec<(String,
      String)>, VaultError>` (commit sha hex, raw commit message), oldest-first. Vault's own
      commits are always single-parent by construction — `head_parent_ids`
      (src/storage/git/mod.rs:129-134) never attaches more than one parent — so walk by following
      `commit.parent_ids()` from `HEAD` and erroring (new `VaultError::NonLinearHistory { sha }`)
      if any commit has more than one parent, rather than silently picking first-parent and
      hiding history a manually-mutated `.vault/.git` might contain (the README already documents
      that users can run raw git commands against it, docs mention in
      src/app/init.rs's embedded README text — inspection is supported, mutation isn't vault's
      problem to guess around). No `HEAD` (fresh, zero-commit repo) returns `Ok(vec![])`, matching
      how `parent_tree_id` already treats a headless repo as equivalent to the empty tree
      (src/storage/git/mod.rs:135-141). For each consecutive pair of commits, use `tree_diff`
      (previous todo) between the parent's tree (or the repo's empty tree for the root commit,
      same helper `parent_tree_id` already uses) and the commit's own tree to get its
      `Vec<FileChange>`. Unit tests: 3-commit linear chain reproduces the exact same
      `Vec<FileChange>` per commit that produced it via `commit_tree`; a merge commit (construct
      one directly via gix, bypassing `commit_tree`) is rejected with `NonLinearHistory`"
    status: pending
  - id: tdd-object-store-port
    content: "`src/ports/object_store.rs`: add `fn history(&self) -> Result<Vec<HistoryCommit>,
      VaultError>` to the `ObjectStore` trait. New domain type `HistoryCommit { sha: CommitSha,
      message: String, changes: Vec<FileChange> }` in `src/domain/snapshot.rs`, next to
      `SnapshotRecord` (src/domain/snapshot.rs:24-32) since it feeds directly into building one —
      export from `src/domain/mod.rs` alongside the existing `SnapshotRecord` re-export
      (src/domain/mod.rs:16). `GixObjectStore::history()` (src/adapters/gix.rs) composes
      `walk_history` + per-commit `tree_diff` from the previous two todos, wrapped in the
      existing `with_store`/`WorktreeCwd` pattern already used by `commit`/`read_blob`
      (src/adapters/gix.rs:53-75)"
    status: pending
  - id: tdd-message-parsing
    content: "`src/domain/message.rs`: add the inverse of `snapshot_message`
      (src/domain/message.rs:15-23) — `parse_created_at(message: &str) -> Option<&str>` (rsplit on
      `\" @ \"`, return the tail) and `parse_single_verb(message: &str) -> Option<&str>` (only
      meaningful for single-change commits — see design note below on why batches don't need
      this). Round-trip unit tests: `parse_created_at(&snapshot_message(&changes, ts)) ==
      Some(ts)` for every `FileEventKind` and both single/batch shapes; a hand-authored message
      with no `\" @ \"` returns `None` rather than panicking or silently returning garbage"
    status: pending
  - id: tdd-reindex-usecase
    content: "New `src/app/reindex.rs`, shaped like `src/app/prune.rs`/`src/app/restore.rs`.
      `pub fn run(layout: &VaultLayout, object_store: &dyn ObjectStore, force: bool) ->
      Result<ReindexOutcome, VaultError>`. Steps: (1) require `.git` present — reuse
      `domain::vault_state`/`missing_markers` (src/domain/vault.rs) the same way
      `app::init::repair` already does (src/app/init.rs:178-186) and return the existing
      `VaultError::PartialVault` when it's missing, so the error vocabulary a user sees for a
      damaged `.vault/` stays uniform across `init` and `reindex`; (2) open (or create)
      `meta.db` via `SqliteMetaIndex`/`MetaDb::open` (src/storage/sqlite/mod.rs:30-47) and check
      `snapshot_count()` (src/storage/sqlite/mod.rs:76-82) — zero rows proceeds unconditionally
      (nothing to lose, matches git auto-regenerating a missing pack `.idx` with no flag needed);
      more than zero rows requires `force == true`, else new `VaultError::MetaDbNotEmpty { path,
      snapshot_count }`; (3) call `object_store.history()`, and for each `HistoryCommit` in
      order: `created_at` from `parse_created_at(&msg)`, falling back to the commit's own
      committer time (need a way to read it — extend `walk_history`'s return or add a follow-up
      accessor) with a `lossy_timestamps` counter surfaced in `ReindexOutcome` when the fallback
      is used, since `create_head_commit`'s signature time
      (src/storage/git/mod.rs:113-127) is a second, later `now_utc()` call and can drift from the
      `Clock`-sourced `created_at` embedded in the message by however long tree-building took —
      the message is the exact value that was written to a live `meta.db`, so prefer it always;
      (4) reclassify `Create`/`Modify` changes to `Restore` only when `changes.len() == 1` and
      `parse_single_verb == Some(\"restore\")` — safe because `app::restore::commit_restore`
      (src/app/restore.rs:60-70) only ever produces a single-`FileChange` commit, so there is no
      multi-file batch case where this reclassification is ambiguous; (5) build one
      `SnapshotRecord` per commit and insert in the same oldest-first order `history()` returned.
      Ordering is load-bearing, not cosmetic: `SELECT_PREVIOUS_COMMIT_FOR_PATH` and
      `SELECT_TRACKED_FILES` (src/storage/sqlite/queries.rs) key off autoincrement `id`, not
      `created_at`, to answer \"previous version of this path\" / \"latest event per path\" —
      inserting out of chronological order would leave `resolve_at` correct (it orders by
      `created_at`) while silently breaking `vault show`'s diff-against-previous feature and
      `vault list`. Write into a sibling temp file
      (`meta.db.reindex.tmp` next to the target) and `fs::rename` over `meta.db` only after every
      insert succeeds, mirroring `git index-pack`'s own `pack-*.idx.temp` → rename pattern, so a
      crash or Ctrl-C mid-rebuild leaves the previous `meta.db` (or its absence) untouched rather
      than a half-populated index. `ReindexOutcome { commits: usize, span: Option<(String,
      String)>, lossy_timestamps: usize }`"
    status: pending
  - id: tdd-dry-run
    content: "`reindex::run` grows a `dry_run: bool` alongside `force` (checked first — dry-run
      short-circuits before the marker/emptiness checks even run, since a dry run should report
      what a real run *would* do including a would-be-refused case, not fail identically to a
      real refusal). On dry run, walk history and compute `ReindexOutcome` exactly as a real run
      would, skip the temp-file write and rename, and additionally report whether `--force` would
      be required (existing `meta.db` row count > 0). Precedent: `vault restore --dry-run`
      (src/app/restore.rs:26-34) already resolves and validates without writing"
    status: pending
  - id: cli
    content: "`src/cli/commands/reindex.rs`: `ReindexArgs { #[arg(long)] force: bool, #[arg(long)]
      dry_run: bool }`, `run(global, args)` resolving the layout the same way
      `restore`/`show` do (`paths::resolve_vault`-style — but note `resolve_vault`
      (src/paths.rs:174-187) itself refuses on `Partial`, which is exactly the state `reindex`
      needs to accept, so reuse `resolve_init`'s layout+state resolution
      (src/paths.rs:163-167) instead and let `app::reindex::run` own the marker check, not the
      CLI layer). Wire `Command::Reindex(ReindexArgs)` into the `Command` enum
      (src/cli/mod.rs:30-49) and its `dispatch` match arm (src/cli/mod.rs:74-84). Output, e.g.:
      `Reindexed meta.db at .vault (42 commits, 2026-01-03T.. to 2026-08-06T..)` /
      `meta.db already has 42 snapshot(s) — pass --force to rebuild from .git history` /
      `Would reindex meta.db: 42 commits, 2026-01-03T.. to 2026-08-06T.. (existing meta.db has 42
      snapshot(s) — --force required)` for the three outcomes; a trailing line when
      `lossy_timestamps > 0` naming how many commits fell back to git's own committer time"
    status: pending
  - id: init-tie-in
    content: "Small, low-risk touch to close the loop the previous plan opened: `vault init`'s
      Partial-repair refusal (src/app/init.rs:178-186 constructs `VaultError::PartialVault` when
      `META_DB` is among `missing`) currently gives no next step. Update the `#[error(...)]`
      message on `VaultError::PartialVault` (src/error.rs:11) — or the CLI's rendering of it — to
      append a hint naming `vault reindex` specifically when `missing` is exactly `[\"meta.db\"]`
      (i.e. `.git` is present), since that's now an actionable, safe next command rather than a
      dead end. Do not touch `.git`-missing wording; that case still has no safe automated fix"
    status: pending
  - id: tdd-integration
    content: "New `tests/reindex.rs`, modeled on `tests/init.rs`'s `partial_vault_*` tests
      (tests/init.rs:69-103) and `tests/restore.rs`'s dry-run coverage. Build a real vault via the
      `vault` binary (`vault init`), perform a sequence exercising every `FileEventKind`: create
      two files in one batch (uniform-kind batch), modify one, delete the other, then `vault
      restore` the deleted one (exercises the Restore-reclassification path). Capture `vault log`
      and `vault show` (whole-vault mode) output as the expected baseline. Delete `meta.db`, run
      `vault reindex`, assert success and the commit-count/span message. Re-run `vault log`/`vault
      show` and assert output is byte-identical to the captured baseline — this is the strongest
      test available since it's exactly what a user would compare. Additional cases: `vault
      reindex` with `.git` missing fails naming `.git` as missing (reuses
      `VaultError::PartialVault` wording, assert on that rather than duplicating it); `vault
      reindex` against a `Ready` vault with real snapshots fails without `--force` and succeeds
      with it, producing the same reindexed content; `vault reindex --dry-run` reports without
      modifying `meta.db`'s mtime/content (hash the file before/after)"
    status: pending
  - id: docs
    content: "docs/src/cli.md: new `### vault reindex` section after `### vault init`
      (docs/src/cli.md:27-59), describing the three outcomes (fresh rebuild / refuse-needs-force /
      force-rebuild) and the dry-run flag. Update the existing `vault init` paragraph
      (docs/src/cli.md:49-52, \"missing, vault init still refuses...\") to mention `vault reindex`
      as the follow-up command for the meta.db-missing case"
    status: pending
  - id: changelog
    content: "CHANGELOG.md Unreleased/Added entry: `vault reindex` — rebuilds `meta.db` from
      `.git`'s commit history by replaying each commit's tree diff; safe/automatic when `meta.db`
      is missing or empty, requires `--force` to overwrite an existing populated index; `--dry-run`
      previews without writing. Closes the `meta.db`-missing gap `vault init` deliberately left
      erroring (see vault_init_idempotent.plan.md's non-goals)"
    status: pending
  - id: ci
    content: "./scripts/ci.sh lint green"
    status: pending
isProject: false
---

# `vault reindex` — rebuild `meta.db` from `.git` history

## Problem

`vault_init_idempotent.plan.md` shipped `vault init`'s idempotency for three of four marker
states, but explicitly refused to touch the fourth: a `.vault/` directory with `.git` intact and
`meta.db` missing (deleted by hand, lost to a disk issue, whatever). That plan's reasoning still
holds — blindly running a fresh baseline snapshot against an *existing* git tree is a silent
no-op (`GitStore::commit_tree` returns `None` for an unchanged tree, `src/storage/git/mod.rs:76`),
so the freshly-created `meta.db` would believe the vault has zero history while `.git` holds the
full log. Its own non-goals section named the actual fix: "rebuilding `meta.db` by replaying
`.git`'s existing commit history is real, useful, out-of-scope work — flag it as a follow-up."
This plan is that follow-up.

## Design

### The git analogue, and why it isn't `--force`

`meta.db` is a pure derived index. Nothing in the write path reads it to decide what to write to
git — `app::snapshot::commit` (`src/app/snapshot.rs:33-56`) always writes the git commit *first*
and only calls `meta_index.record_snapshot` once that succeeds. Reads (`vault log`/`show`/`list`/
`restore`'s resolution) go through `meta_index`, never `.git` directly, but everything in
`meta.db` is fully reconstructable from `.git` alone: a commit's tree diffed against its parent's
tree tells you exactly which paths were created, modified, or deleted, and the commit message
(embedded by `app::snapshot::commit` itself) carries the original timestamp and verb.

Git has two structures in exactly this relationship to their own source of truth:

- **The pack `.idx` file.** A byte-for-byte-reconstructable index over a `.pack` file's object
  offsets. If it's missing or corrupt, git regenerates it transparently and automatically the
  moment it needs to read that pack — no flag, no confirmation, because there's no data at risk:
  the pack itself (the source of truth) is untouched, and the index is trivially rebuildable.
- **The `commit-graph` file.** A derived cache of commit metadata/parentage for fast graph
  walks. Unlike the pack index it's *not* auto-regenerated on access — it's opt-in, built
  explicitly via `git commit-graph write`, because computing it is a real cost across a large
  history, not a cheap incidental fixup.

`meta.db` sits with the second one: reconstructing it means walking every commit and diffing every
tree, real work that scales with history size, not something to trigger silently as a side effect
of an unrelated command. So the shape here is `git commit-graph write`'s shape — an explicit,
user-invoked rebuild command — not automatic magic folded into `vault init`.

That leaves the actual question: **is it `--force`?** Half yes, half no, and the split is exactly
where git itself draws it:

- **Rebuilding when `meta.db` is missing or empty needs no flag at all.** There's nothing to
  lose — this is the "pack `.idx` missing, just regenerate it" case. Requiring `--force` here
  would be `git` making you type `--force` to let it rebuild an index it already knows is absent;
  git never does that.
- **Rebuilding when `meta.db` already has real content needs `--force`.** This is no longer "the
  derived cache is absent," it's "overwrite existing local state because I said so" — the exact
  shape `--force` has in `git branch -f`, `git checkout -f`, `git push --force`: not "regenerate
  something missing" but "discard something present without git's normal safety check." Even
  though the *rebuilt* content should reconstruct correctly, an existing `meta.db` might reflect
  history from a `.git` that has since been pruned/rewritten by hand — `vault reindex` has no way
  to know its rebuild is strictly a superset of what's there, so it asks first.

So: `vault reindex` runs unconditionally when `meta.db` is missing or has zero snapshots, and
requires `--force` when it already has rows. `.git` missing is a hard, unconditional refusal
either way — there is nothing to replay from, the same reasoning `vault init`'s own repair path
already uses (`src/app/init.rs:178-186`).

### Reconstructing exact file-event kinds, not just "something changed"

A tree diff alone gives three buckets: added, modified, removed — enough for `Create`/`Modify`/
`Delete`, but `FileEventKind` has a fourth variant, `Restore` (`src/domain/change.rs:14-15`),
recorded when content came back via `vault restore` rather than an organic edit. That distinction
is invisible to a tree diff — restored content and a plain re-create look identical in git's
object model. It survives in the *commit message*, though: `verb_for` (`src/domain/message.rs:43-
50`) maps `Restore` to `"restore"` and both `Create`/`Modify` to `"update"`, distinct strings, and
`app::restore::commit_restore` (`src/app/restore.rs:60-70`) always commits exactly one
`FileChange`, never batched with other edits. So for any single-change commit, parsing the verb
out of the message resolves the ambiguity completely; batched multi-file commits never need to,
because `Restore` never appears in one. Batch commits with mixed kinds already collapse to the
generic verb `"change"` in the message (`src/domain/message.rs:33-39`) — irrelevant here, since
tree-diff alone already tells `Create`/`Modify`/`Delete` apart unambiguously per path within a
batch; the verb is only load-bearing for the `Restore` case.

Timestamps reconstruct via the same message string, more precisely than git's own commit
timestamp would: `create_head_commit` (`src/storage/git/mod.rs:113-127`) stamps the commit with a
*second*, later call to `gix::date::Time::now_utc()`, separate from the `Clock::now()` read that
produced the `created_at` embedded in both the message and the original `meta.db` row
(`src/app/snapshot.rs:44-49`). They're normally milliseconds apart, but the message is the exact
value a live `meta.db` would have held — parse it, don't trust the commit's own timestamp except
as a fallback for a message that doesn't match vault's format (which shouldn't happen for a
vault-owned repo, but "shouldn't happen" isn't "can't," so it degrades instead of erroring).

### Ordering matters for correctness, not just cosmetics

`meta.db`'s autoincrement `id` isn't just a primary key — `SELECT_PREVIOUS_COMMIT_FOR_PATH` and
`SELECT_TRACKED_FILES` (`src/storage/sqlite/queries.rs`) both key "what came before" and "what's
latest per path" off `id`/`snapshot_id` ordering, not `created_at`. Reindexing must insert
snapshot rows in the same oldest-first order the commits actually happened in, or `vault show`'s
diff-against-previous-version feature and `vault list` will silently disagree with `vault log`
(which does order by `created_at`) even though every individual row is otherwise correct.

### Crash safety

Build the replacement `meta.db` in a temp file next to the target and `rename` it into place only
after every row is written successfully — the same pattern `git index-pack` uses for its own
`.idx.temp` → final rename. A crash or interrupt mid-rebuild leaves whatever `meta.db` (or
absence of one) existed before untouched, never a half-populated index masquerading as complete.

## Files touched

| Area | File | Change |
|------|------|--------|
| Git adapter | `src/storage/git/tree_diff.rs` (new) | Recursive tree-diff, hand-rolled (no new gix feature) |
| Git adapter | `src/storage/git/mod.rs` | `GitStore::walk_history` — oldest-first commit walk, rejects merge commits |
| Domain | `src/domain/snapshot.rs` | New `HistoryCommit` type |
| Domain | `src/domain/message.rs` | `parse_created_at`, `parse_single_verb` — inverse of `snapshot_message` |
| Domain | `src/domain/mod.rs` | Export `HistoryCommit` |
| Port | `src/ports/object_store.rs` | `ObjectStore::history()` |
| Adapter | `src/adapters/gix.rs` | `GixObjectStore::history()` |
| Use-case | `src/app/reindex.rs` (new) | Core rebuild logic, `force`/`dry_run`, temp-file + rename |
| Error | `src/error.rs` | New `VaultError::MetaDbNotEmpty`, `VaultError::NonLinearHistory`; tweak `PartialVault` message |
| CLI | `src/cli/commands/reindex.rs` (new), `src/cli/mod.rs` | `vault reindex [--force] [--dry-run]` |
| Tests | `tests/reindex.rs` (new) | End-to-end rebuild-matches-original coverage |
| Docs | `docs/src/cli.md` | New `### vault reindex` section; tie-in note under `vault init` |
| Changelog | `CHANGELOG.md` | Unreleased/Added entry |

## Verification

- Unit: `tree_diff` correctly classifies create/modify/delete across nested paths, including the
  root-commit (empty-tree-as-parent) case; `walk_history` reproduces the exact `Vec<FileChange>`
  each commit was built from and rejects merge commits; `parse_created_at`/`parse_single_verb`
  round-trip against `snapshot_message` for every `FileEventKind`.
- Integration: full cycle (`init` → varied edits including a `restore` → capture `log`/`show` →
  delete `meta.db` → `reindex` → re-capture `log`/`show`) produces byte-identical output;
  `.git`-missing refuses; existing-populated-`meta.db` refuses without `--force`, succeeds with
  it; `--dry-run` never modifies `meta.db`.
- Manual: reproduce by hand — snapshot a few real files, delete `.vault/meta.db`, run `vault
  reindex`, confirm `vault log`/`vault show` read exactly as they did before deletion.
- `./scripts/ci.sh lint` green.

## Non-goals

- Automatic reindexing as part of `vault init`'s repair path. Kept as a separate explicit command
  matching `git commit-graph write`'s shape, not `git`'s auto-regenerated pack `.idx` — rebuilding
  is real work proportional to history size, not a cheap incidental fixup safe to trigger from an
  unrelated command.
- Recovering from a `.git`-missing vault. Still refused unconditionally, same reasoning
  `vault init`'s repair path already documents (`src/app/init.rs:178-186`) — there's nothing to
  replay from.
- Partial/incremental reindex (only replaying commits newer than some point). Every run is a full
  rebuild into a fresh temp file; `meta.db` is small enough relative to `.git` that partial
  reindexing would add real complexity (tracking "where did the old index leave off, and do I
  trust that") for a case this plan has no evidence is needed.
- Recovering the exact original `Restore` classification for any historical commit whose message
  doesn't match vault's own format (e.g. a `.vault/.git` a user has hand-edited outside vault).
  Falls back to `Create`/`Modify` from the tree diff alone, which is correct except for that one
  label.

## Exit criteria

- [ ] `vault reindex` on a vault missing `meta.db` (or with an empty one) rebuilds it with no flag
  required, and `vault log`/`show`/`list` read identically to before the deletion
- [ ] `vault reindex` on a vault with a populated `meta.db` refuses without `--force` and succeeds
  with it
- [ ] `vault reindex` on a vault missing `.git` refuses unconditionally
- [ ] `vault reindex --dry-run` reports without writing
- [ ] `docs/src/cli.md` documents the command and ties it to `vault init`'s meta.db-missing case
- [ ] `CHANGELOG.md` records the addition
- [ ] `./scripts/ci.sh lint` green
