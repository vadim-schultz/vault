---
name: Make `vault init` idempotent
overview: "`vault init` currently hard-errors (`AlreadyInitialized`) on any second run, even when
  the only real problem is that the daemon died and the user has no other way to restart it short
  of deleting `.vault/` and re-initializing from scratch (destroying history). Reported directly
  from a live `.plans` self-hosted vault: `vault status` showed `Daemon: stopped`, and `vault init`
  refused to help. Bring `vault init` in line with `git init`'s own idempotency: re-running it
  against a healthy vault is a safe no-op (plus ensuring the daemon/registration are in the state
  a first-time run would have left them), and re-running it against a *damaged* vault repairs
  whatever is safe to regenerate without guessing at data recovery."
todos:
  - id: branch
    content: Sync main, create feat/vault-init-idempotent branch
    status: pending
  - id: tdd-state-resolution
    content: "`src/paths.rs`: split `resolve_init`'s error-on-non-Absent behavior out so
      `app::init::run` can see the `VaultState` instead of only getting `Err`. Simplest shape:
      change `resolve_init` to return `Result<(VaultLayout, VaultState), VaultError>`, dropping the
      `AlreadyInitialized`/most of the `PartialVault` short-circuiting (keep constructing the
      layout either way); update its two existing unit tests
      (`resolve_init_defaults_to_cwd_vault`, `resolve_init_rejects_existing_vault`) for the new
      return shape — the second one now asserts `VaultState::Ready` comes back rather than an
      `Err`"
    status: pending
  - id: tdd-outcome-type
    content: "`src/app/init.rs`: introduce `InitOutcome { Created, AlreadyReady(DaemonAction),
      Repaired { filled: Vec<&'static str>, daemon: DaemonAction } }` and `enum DaemonAction {
      AlreadyRunning, Started, SkippedNoService }`. `run()` returns `Result<InitOutcome,
      VaultError>` (was `Result<(), VaultError>`); `initialize()` returns `Result<(VaultLayout,
      InitOutcome), VaultError>` (was `Result<VaultLayout, VaultError>`). `start_watching` grows a
      return value instead of `()` so callers can report which `DaemonAction` happened; its
      existing `daemon::is_running()` guard is the piece that already makes daemon-start
      idempotent today (`src/app/init.rs:143-148`) — reuse it as-is, just thread its outcome
      upward instead of discarding it"
    status: pending
  - id: tdd-ready-branch
    content: "`run()`: branch on the resolved `VaultState`. `Absent` — unchanged existing path
      (provision, baseline, register, start_watching), returns `InitOutcome::Created`. `Ready` —
      skip provisioning and baseline entirely (the vault is already fully formed); still call
      `register_globally` (already idempotent — `VaultRegistry::register`, src/registry.rs:104-116,
      returns `Ok(false)` and skips writing when the root is already listed) and
      `start_watching` (already idempotent per above), then return
      `InitOutcome::AlreadyReady(daemon_action)`. No filesystem writes happen on this path at all
      when the daemon was already running — true no-op, matching `git init`'s own behavior of
      never touching an existing repository's objects/refs on a repeat run"
    status: pending
  - id: tdd-partial-branch
    content: "`run()` `Partial(found)` branch: repair only the two markers that are safe to
      regenerate with zero data risk — `README` (static content, always safe to overwrite) and
      `config.toml` (regenerating `VaultConfig::defaults()` loses custom `watch_roots`/`ignore`
      edits, which is a data-*loss* risk but not a data-*corruption* risk — the user can re-edit;
      document this explicitly in the command's output, e.g. `restored config.toml with defaults
      — re-apply any custom watch_roots/ignore`). If either `.git` or `meta.db` is among the
      *missing* markers, do NOT attempt repair — return `VaultError::PartialVault` as today, but
      reword the message to name what's *missing* (not what's present) since that's what the user
      needs to act on. Reasoning to preserve in the code comment or commit message: `.git` missing
      means `GixObjectStore::init` would create a brand-new empty repo out from under a `meta.db`
      that still references now-nonexistent commit SHAs (corruption, not repair); `meta.db`
      missing means a fresh baseline snapshot against an *existing* non-empty git history is a
      silent no-op for unchanged files (`GitStore::commit_tree` returns `None` when the tree is
      identical, so `snapshot::commit` never calls `record_snapshot` — see
      `src/app/snapshot.rs:39-43` and `src/adapters/gix.rs:58-65`), which would leave `meta.db`
      believing the vault has *no* history at all despite `.git` holding the full log. Rebuilding
      `meta.db` by replaying `.git`'s existing commit history is real, useful, out-of-scope work —
      flag it as a follow-up, not something this plan attempts"
    status: pending
  - id: cli-output
    content: "`src/cli/commands/init.rs`: branch the printed message on `InitOutcome` instead of
      the current unconditional `Vault initialized at {path}`. `Created` keeps today's message.
      `AlreadyReady(daemon)` prints `Vault already initialized at {path}` plus one line naming the
      `DaemonAction` (`Daemon already running (pid N)` / `Daemon was stopped — restarted it` /
      `Daemon start skipped (--no-service)`) — deliberately scoped to *this* vault's daemon status,
      not a full `vault status` dump, since `vault status` reports on every registered vault and
      `vault init` only ever targets one path; reusing the multi-vault report here would be a
      scope mismatch. `Repaired` prints `Vault repaired at {path}` plus which markers were
      restored, then the same daemon-action line. Exit code stays 0 for `Created` and
      `AlreadyReady`; decide (and note in the plan review) whether `Repaired` should also exit 0
      — recommendation: yes, since a `Partial` vault that got fully healed is no longer a failure
      state, matching `git init`'s \"Reinitialized existing Git repository\" success message on a
      repeat run"
    status: pending
  - id: tdd-integration-tests
    content: "`tests/init.rs`: replace `init_rejects_second_run` (currently asserts failure +
      \"already initialized\" in stderr) with `init_second_run_is_idempotent` — asserts *success*,
      stdout contains \"already initialized\", and the daemon guard/heartbeat state is unchanged
      when the daemon was already running (use the same `DaemonGuard::acquire` +
      `STATE_ENV_LOCK`/`STATE_DIR_ENV` pattern as `src/app/init.rs`'s existing
      `does_not_start_when_daemon_already_running` unit test). Add
      `init_second_run_restarts_stopped_daemon` — init once, confirm no daemon process is running
      (or fake the service manager via the existing `RecordingServiceManager` at the `app::init`
      level rather than spawning a real detached process in an integration test), run `vault init`
      again, assert the service manager's `start()` was invoked exactly once. Keep
      `partial_vault_reports_stray_files` (src/tests/init.rs:69-82) exactly as-is — it constructs a
      vault with only `README` present, i.e. `.git`/`meta.db` still missing, which is precisely the
      case this plan keeps erroring on; it should keep failing unchanged and is a useful regression
      guard for that boundary. Add a new `partial_vault_heals_missing_readme_and_config` covering
      the *opposite* corner: `.git` and `meta.db` present, `README`/`config.toml` missing — assert
      success and that both files now exist"
    status: pending
  - id: docs
    content: "docs/src/cli.md `### vault init` section (~line 27-44): replace line 39's
      \"Running `vault init` again in the same directory fails with an `already initialized`
      error.\" with a description of the three outcomes (created / already-initialized-plus-daemon-
      check / repaired-plus-daemon-check), matching whatever final message wording lands in the
      CLI todo above"
    status: pending
  - id: changelog
    content: "CHANGELOG.md Unreleased/Changed entry: `vault init` on an already-initialized vault
      no longer errors — it verifies (and restarts if needed) the daemon and repairs safely-
      regenerable markers (`README`, `config.toml`) instead, matching `git init`'s own idempotency;
      still refuses to guess when `.git` or `meta.db` itself is missing"
    status: pending
  - id: ci
    content: "./scripts/ci.sh lint green"
    status: pending
isProject: false
---

# Make `vault init` idempotent

## Problem

Reported directly against the `.plans` self-hosted vault:

```
$ vault status
Daemon: stopped
Service: unsupported
Heartbeat age: 10414s
Vaults: 1
  /Users/vadim/projects/vault/.plans [ok] last snapshot: 2026-08-06T05:29:55.322674+00:00
    housekeeping: 58 loose objects, 0 pack (never repacked)

$ vault init
Error: vault already initialized at /Users/vadim/projects/vault/.plans
```

The daemon is dead, `vault status` can *see* that, but there is no command that can *fix* it short
of deleting `.vault/` (destroying history) and re-initializing from nothing. `vault init` is the
only command that path-checks "does this need bootstrapping" and takes remedial action — it
already contains exactly the logic needed to restart a stopped daemon
(`start_watching`, `src/app/init.rs:143-148`, guarded by `daemon::is_running()`) — but
`resolve_init` (`src/paths.rs:162-174`) short-circuits with `VaultError::AlreadyInitialized` before
that logic is ever reached, whenever `.vault/`'s four init markers (`config.toml`, `meta.db`,
`.git/`, `README`) are all present.

This is also explicitly documented today as the intended behavior (`docs/src/cli.md:39`: "Running
`vault init` again in the same directory fails with an `already initialized` error"), so this is a
deliberate change of contract, not a bug fix.

## Design

`git init` is the model to match: running it again against an existing repository is always safe
— it never touches objects, refs, or history, and merely reports `Reinitialized existing Git
repository in ...`. `vault init` should adopt the same shape, branching on the existing
`VaultState` (`src/domain/vault.rs:63-86`) instead of treating anything but `Absent` as fatal:

| `VaultState` | Today | New behavior |
|---|---|---|
| `Absent` | Full init (provision, baseline snapshot, register, start daemon) | Unchanged |
| `Ready` | `Err(AlreadyInitialized)` | No-op on the vault's data; still ensures registration (already idempotent) and daemon/service are running (already idempotent, just unreachable); reports what it found |
| `Partial(found)` | `Err(PartialVault)` | Repairs only the markers that are safe to regenerate (`README`, `config.toml`); still refuses when `.git` or `meta.db` itself is among the missing markers |

**Why `Ready` needs no filesystem writes at all.** `register_globally` already returns `Ok(false)`
without writing `registry.toml` when the root is already listed
(`VaultRegistry::register`, `src/registry.rs:104-116`). `start_watching` already checks
`daemon::is_running()` before calling into the service manager
(`src/app/init.rs:143-148`). Both of `vault init`'s two side effects beyond provisioning are
already idempotent — they're just never reached today because `resolve_init` errors first. Wiring
this case through only requires *not erroring*, not new idempotency logic.

**Why `Partial` repair stops at `README`/`config.toml`.** The other two markers are data-bearing,
and "repairing" them by blindly recreating an empty one is actively dangerous rather than helpful:

- Missing `.git` with `meta.db` intact: `GixObjectStore::init` (`src/adapters/gix.rs:33-38`)
  creates a **brand-new, empty** bare repository. `meta.db` would then hold commit SHAs that no
  longer exist anywhere — silent corruption, not repair.
- Missing `meta.db` with `.git` intact: a fresh baseline walk (`collect_baseline_changes`) against
  an *existing*, unchanged git tree produces no new commit (`commit_tree` returns `None` for an
  unchanged tree, so `snapshot::commit` never calls `record_snapshot` —
  `src/app/snapshot.rs:39-43`). The freshly-created `meta.db` would end up believing the vault has
  *zero* history, even though `.git` holds the full log. `vault show`/`log`/`restore` all read
  through `meta.db`, so this would silently hide real history rather than losing it — arguably
  worse than an honest error, since nothing would look wrong until a user went looking for an old
  snapshot that git still has but the index can't find.

A real fix for the `meta.db`-missing case is "rebuild the index by replaying `.git`'s commit log,"
which is legitimate future work but a meaningfully different feature (a `meta.db` reindex/rebuild
path) from *idempotent init*. This plan explicitly does not attempt it — `Partial` states that
include `.git` or `meta.db` in the *missing* set keep failing, with a clearer message about what's
actually absent.

**Output.** `vault init`'s scope is always a single path; `vault status`'s is every registered
vault. Reusing `vault status`'s renderer for the already-initialized/repaired cases would report on
vaults the user didn't ask about, so the new messages stay narrowly scoped to the target vault and
its daemon, e.g.:

```
$ vault init
Vault already initialized at /Users/vadim/projects/vault/.plans
Daemon was stopped — restarted it

$ vault init
Vault already initialized at /Users/vadim/projects/vault/.plans
Daemon already running (pid 4821)
```

```
$ vault init          # README and config.toml were both deleted by hand
Vault repaired at /Users/vadim/projects/vault/.plans (restored: README, config.toml)
restored config.toml with defaults — re-apply any custom watch_roots/ignore
Daemon already running (pid 4821)
```

```
$ vault init          # .git was deleted by hand
Error: incomplete vault at /Users/vadim/projects/vault/.plans (missing: .git) — refusing to
regenerate: existing meta.db may reference history that would no longer exist. Manual recovery
needed.
```

## Files touched

| Area | File | Change |
|------|------|--------|
| Core | `src/paths.rs` | `resolve_init` returns `(VaultLayout, VaultState)` instead of erroring on non-`Absent` |
| Core | `src/domain/vault.rs` | None (state model already sufficient) |
| Use-case | `src/app/init.rs` | New `InitOutcome`/`DaemonAction` types; `run`/`initialize`/`start_watching` return outcomes instead of `()`; branch on `VaultState` |
| CLI | `src/cli/commands/init.rs` | Print message based on `InitOutcome` instead of unconditional "Vault initialized at" |
| Tests | `src/paths.rs`, `src/app/init.rs` (`#[cfg(test)]`) | Update for new return shapes; new tests for `Ready`/`Partial` branches |
| Tests | `tests/init.rs` | Replace `init_rejects_second_run`; add idempotent-daemon-restart and safe-partial-repair coverage; keep `partial_vault_reports_stray_files` unchanged as the data-marker-missing regression guard |
| Docs | `docs/src/cli.md` | Rewrite the "fails with an already initialized error" line |
| Changelog | `CHANGELOG.md` | Unreleased/Changed entry |

## Verification

- Unit: `resolve_init` returns `VaultState::Ready`/`Partial` instead of erroring; `app::init::run`
  takes the `Ready` branch without any filesystem writes when the daemon is already running
  (extend the existing `does_not_start_when_daemon_already_running` test to also assert no writes
  to `config.toml`/`README`); `Partial` branch repairs `README`/`config.toml` when missing but
  returns an error naming the *missing* marker when `.git` or `meta.db` is absent.
- Integration: second `vault init` run exits 0 and reports "already initialized"; when the daemon
  was stopped, a second run restarts it and a subsequent `vault status` (or heartbeat check) shows
  it running; a vault missing only `README`/`config.toml` self-heals; a vault missing `.git` or
  `meta.db` still fails with a message naming that specific marker.
- Manual repro check: reproduce the reported scenario (stopped daemon, `Ready` vault) and confirm
  `vault init` restarts the daemon instead of erroring, and that a follow-up `vault status` shows
  `Daemon: running`.
- `./scripts/ci.sh lint` green.

## Non-goals

- Rebuilding `meta.db` by replaying existing `.git` commit history when `meta.db` is the only
  missing marker. Real feature, different scope — tracked here as a named follow-up, not attempted.
- Any change to `vault status`'s own output or its (already read-only) semantics.
- A `--force` flag to blow away and fully re-provision a `Partial` or `Ready` vault. Not requested,
  and it would reintroduce the exact data-loss risk this plan is designed to avoid.

## Exit criteria

- [ ] `vault init` on a `Ready` vault exits 0, performs no filesystem writes, and restarts the
  daemon if (and only if) it was stopped
- [ ] `vault init` on a `Partial` vault missing only `README`/`config.toml` self-heals and exits 0
- [ ] `vault init` on a `Partial` vault missing `.git` or `meta.db` still fails, with a message
  naming the missing marker
- [ ] `docs/src/cli.md` no longer documents the old "fails on second run" contract
- [ ] `CHANGELOG.md` records the behavior change
- [ ] `./scripts/ci.sh lint` green
