---
name: Showcase script — exercise every use case with internal git/sqlite inspection
overview: A narrated, human-facing demo script (scripts/showcase.sh) that drives every vault
  subcommand against a disposable vault and, after each step, prints what actually landed in the
  bare git object store (.vault/.git) and the metadata index (.vault/meta.db). Distinct from
  scripts/ci.sh (lint/build/docs gate) and Chapter 5's planned scripts/smoke_test.sh (pass/fail CI
  check) — this is a teaching/debugging tool a human runs and reads.
todos:
  - id: showcase-prereq
    content: "Blocked on Chapter 5 Phase 5 (CLI wiring): src/cli/mod.rs still stub()s show/log/diff/restore/list (bail!(\"{name} not implemented yet\")). The showcase script cannot exercise five of its nine use cases until that lands on this branch."
    status: complete
  - id: showcase-helpers
    content: "scripts/showcase.sh scaffolding: arg parsing (--keep, --pause, --vault-bin), tempdir setup/trap cleanup, section()/run()/inspect_git()/inspect_sqlite() narration helpers"
    status: complete
  - id: showcase-walkthrough
    content: "Full command sequence: init -> watcher-driven create/modify/delete -> status/list/log/diff/show/restore(+dry-run) -> ignore, with inspect_git/inspect_sqlite after every state-changing step"
    status: complete
  - id: showcase-docs
    content: "Document the script (README + docs/src/getting_started.md or a new docs/src/showcase.md) and note it in .plans/README.md"
    status: complete
isProject: false
---

# Showcase script

**Status: implemented** (`scripts/showcase.sh`), on `feat/ch5-time-travel` rather than its own
branch per §"Sequencing relative to Chapter 5" — the user asked to keep this chapter's CI-fix and
follow-on work on one branch until CI is green, rather than opening a second branch mid-flight.
The open question below (daemon ignore-pattern hot-reload) is resolved: confirmed empirically
that a running daemon does **not** pick up a newly added `vault ignore` pattern without a
restart; the script restarts the daemon and calls this out in its narration rather than papering
over it.

## Why this doesn't already exist

Checked `scripts/` (only `ci.sh`, a lint/build/docs CI mirror — no functional walkthrough) and
grepped the repo for `smoke`/`demo`/`showcase`: nothing. Chapter 5's plan (`chapter_5.plan.md`)
does call for a `scripts/smoke_test.sh`, but that's a different tool with a different job: a
pass/fail CI gate (`init → edit → daemon → show/restore --at`, wired into `ci.yml`), not a
narrated walkthrough meant for a person to read. Both are worth having; this plan is only for the
showcase script.

**Blocking issue found while scoping this**: `git log` shows Chapter 5's app-layer use-cases
landed in `d7dca63` (`show.rs`, `log.rs`, `diff.rs`, `restore.rs`, `list.rs` all exist and are
unit-tested), but `src/cli/mod.rs` still routes all five to `stub(name)`, and
`docs/src/cli.md` still marks them "Stub". So today, `vault show/log/diff/restore/list` all just
print `"<name> not implemented yet"` and exit non-zero. A showcase script is only useful once
Phase 5 (CLI wiring) of the Chapter 5 plan is finished — recommend landing that first, on this
same `feat/ch5-time-travel` branch, before writing `showcase.sh`.

## Goal

One script, `scripts/showcase.sh`, runnable by a human (`./scripts/showcase.sh`), that:

1. Builds (or reuses) the release binary.
2. Creates a disposable vault in a tempdir — never touches the user's real `~/.vault` registry
   state or an existing project.
3. Runs every subcommand at least once, using the **real background daemon** (not a test-only
   bypass) so the watcher path is genuinely exercised, not simulated.
4. After every state-changing step, prints:
   - `git --git-dir=.vault/.git log --oneline --stat` (or a full commit's `git cat-file -p`) — the
     commit graph, messages, and touched blobs.
   - `sqlite3 .vault/meta.db` output for `snapshots` and `file_events` (`.mode column`,
     `.headers on`) — the rows that back `resolve_at`/`list_snapshots`/`list_tracked_files`.
5. Cleans up after itself (tempdir + daemon process) unless `--keep` is passed.

Non-goal: this is not a CI gate. It should not be added to `ci.yml`. It's a dev/onboarding tool —
"run this to see what Vault actually does under the hood."

## Design decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Isolated `mktemp -d` worktree, `vault init` run with `cd` into it. | Never risk the user's real vault registry (`~/.local/share/vault/registry` or platform equivalent) or an existing `.vault/` in this repo. |
| 2 | Drive file changes through a **real running daemon** (`vault daemon --foreground &`), not `commit_batch` test fixtures. | Test fixtures bypass the watcher entirely (see Chapter 5 plan's `tests/common::write_and_commit`) — fine for assertions, wrong for a demo whose whole point is "Vault watches files in the background." Sleep past `debounce_ms` (default 2000ms, `src/config.rs`) between edits instead of polling. |
| 3 | Use `--no-service`/`VAULT_NO_SERVICE=1` on `init` to skip installing a launchd/systemd unit, but still spawn `vault daemon --foreground` ourselves for the script's lifetime. | We want the watcher exercised without mutating the user's real OS service state. |
| 4 | Bare repo has commits on `HEAD` (`storage/git.rs::commit_tree_inner` calls `commit_as(..., "HEAD", ...)`), no linked worktree — so `git --git-dir=.vault/.git <cmd>` works directly; no `--work-tree` or index gymnastics needed. | Confirmed by reading `storage/git.rs`; keeps the inspection one-liners simple. |
| 5 | Require `sqlite3` on `PATH`; fail fast with an install hint if missing, don't silently skip DB inspection. | DB inspection is half the point of the script. macOS ships `sqlite3`; document the Linux package name in the error message. |
| 6 | `--pause` flag: after each `inspect_git`/`inspect_sqlite` block, `read -r -p "Press enter to continue..."` when set. Default is non-interactive (prints and keeps going) so it stays CI-safe to run ad hoc without hanging. | Same script serves "read the output later" and "walk through it live in a meeting" use cases. |
| 7 | `trap cleanup EXIT` kills the daemon PID and (unless `--keep`) `rm -rf` the tempdir, even on early failure (`set -euo pipefail`). | Matches `ci.sh`'s existing style of a strict-mode script; avoids leaking background `vault daemon` processes on failure. |

## Script structure

```text
scripts/showcase.sh
├── usage() / arg parsing: --keep, --pause, --vault-bin PATH, -h/--help
├── require sqlite3, cargo (reuse ci.sh's require_cargo pattern) on PATH
├── build release binary if missing (or honor --vault-bin to skip rebuild)
├── WORKDIR=$(mktemp -d); trap cleanup EXIT
├── narration helpers:
│   ├── section "title"        # banner
│   ├── run "vault ..."        # echo the command, then eval it
│   ├── inspect_git            # git --git-dir=.vault/.git log --oneline --stat -5
│   ├── inspect_sqlite         # sqlite3 .vault/meta.db against snapshots + file_events
│   └── pause                  # only when --pause
└── walkthrough (see table below)
```

## Command sequence

Every row is: run the command(s), then `inspect_git` + `inspect_sqlite`, then (for `show`/`list`/
`log`/`diff`/`status`) also print the command's own stdout since that *is* the demo, not just the
storage side effect.

| Step | Command(s) | What the inspection should show |
|------|-----------|----------------------------------|
| 1 | `vault init --no-service` | `.vault/{config.toml,README,.git/,meta.db}` created; git log has one baseline commit; `snapshots` has 1 row, `file_events` has one row per pre-existing file (likely none in a fresh tempdir — note this or seed a pre-existing file before `init` so baseline isn't empty). |
| 2 | spawn `vault daemon --foreground &`, capture PID | `vault status` shows watcher healthy. |
| 3 | write `notes.md` v1, sleep past debounce | new commit "vault: create notes.md @ ..."; new `snapshots` row; `file_events` row `event_type='create'`. |
| 4 | overwrite `notes.md` v2, sleep | new commit "vault: update notes.md @ ..."; `file_events` row `event_type='modify'`. |
| 5 | write `draft.md`, sleep | another create commit/row — gives `list`/`log` something to filter. |
| 6 | delete `draft.md`, sleep | delete commit; `file_events` row `event_type='delete'`; tree no longer contains `draft.md` (`git ls-tree`/`git cat-file -p HEAD^{tree}`). |
| 7 | `vault status` | daemon healthy, last snapshot time matches step 6. |
| 8 | `vault list` | only `notes.md` (created not deleted); `draft.md` excluded — demonstrates `list_tracked_files` filtering deleted paths. |
| 9 | `vault log` and `vault log notes.md` | full history vs. path-scoped history; capture the RFC3339 timestamp of the v1 commit from output for later steps. |
| 10 | `vault show notes.md --at <v1 timestamp>` | stdout is v1's bytes; cross-check against `git cat-file -p <commit>:notes.md`. |
| 11 | `vault diff notes.md --at <v1> --to <v2 timestamp>` | unified diff; no new commit/rows (read-only). |
| 12 | `vault diff notes.md` (no flags) | last snapshot vs. working tree — edit the file once more on disk without waiting for the daemon first, to show the "uncommitted working tree" side. |
| 13 | `vault restore notes.md --at <v1> --dry-run` | stdout says what would happen; **no** new commit/rows. |
| 14 | `vault restore notes.md --at <v1>` | new commit "vault: restore notes.md @ ..."; `file_events` row `event_type='restore'` (distinct from `modify`) — the whole point of `FileEventKind::Restore`. |
| 15 | `vault ignore '*.tmp'`, write `scratch.tmp`, sleep | `config.toml` gains the pattern; confirm (empirically, during implementation — see open question below) whether the already-running daemon picks it up live or needs a restart; either way, end state: `scratch.tmp` never appears in a commit or `file_events`. |
| 16 | kill daemon, final `inspect_git` + `inspect_sqlite` | full end-to-end history recap. |

**Open question to resolve during implementation, not in this plan**: does a running daemon reload
`config.toml`/ignore patterns without a restart? `src/watcher/mod.rs`'s `reload_tx` fires on
registry-file changes (new vaults appearing), not obviously on a single vault's own config edits.
If it doesn't hot-reload, step 15 needs a daemon restart between `vault ignore` and writing
`scratch.tmp` — verify against actual behavior rather than assuming either way.

## Inspection helper sketches

```bash
inspect_git() {
    section "git: .vault/.git"
    git --git-dir="$VAULT_DIR/.git" log --oneline --stat -5
}

inspect_sqlite() {
    section "sqlite: .vault/meta.db"
    sqlite3 -column -header "$VAULT_DIR/meta.db" \
        "SELECT id, commit_sha, created_at FROM snapshots ORDER BY id DESC LIMIT 5;"
    sqlite3 -column -header "$VAULT_DIR/meta.db" \
        "SELECT f.id, f.snapshot_id, f.path, f.event_type FROM file_events f ORDER BY f.id DESC LIMIT 10;"
}
```

## Files touched

```text
scripts/
└── showcase.sh                 # NEW
README.md                       # + one-line pointer under "Build" or a new "Demo" section
docs/src/getting_started.md     # + pointer to the script (or new docs/src/showcase.md + SUMMARY.md entry)
.plans/README.md                # note this plan once it lands
```

## Sequencing relative to Chapter 5

1. Finish Chapter 5 Phase 5 (CLI wiring in `src/cli/mod.rs`) and Phase 6 (integration tests) on
   the current `feat/ch5-time-travel` branch — this plan doesn't re-scope that work, it just
   depends on it.
2. Land Chapter 5, merge to `main`.
3. Per `CLAUDE.md`, start this as its own chapter-less feature branch off a fresh `main`
   (`git checkout main && git pull && git checkout -b feat/showcase-script`) rather than bolting
   it onto the Chapter 5 branch, since it's a separate concern (dev tooling, not a numbered
   chapter) with its own PR.

## Acceptance

- `./scripts/showcase.sh` runs to completion on a clean checkout with no prior `.vault/` state,
  exit code 0.
- Every one of the 9 subcommands (`init`, `status`, `ignore`, `show`, `restore`, `log`, `diff`,
  `list`, and the daemon path implicitly via `daemon --foreground`) is invoked at least once.
- Output makes the git-commit-per-change and sqlite-row-per-event mapping legible without reading
  source — a new contributor should be able to run it and understand the storage model.
- `--keep` leaves the tempdir path printed at the end for manual poking; default run leaves no
  trace (no stray daemon process, no leftover directory).
