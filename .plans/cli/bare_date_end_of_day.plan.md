---
name: Fix bare-date `--at` semantics — end of local day, not start of UTC day
overview: "`--at YYYY-MM-DD` currently resolves to UTC midnight (start of that day), and
  show/restore/diff resolve to the latest snapshot at-or-before that instant. Net effect: querying
  a bare date never shows anything that happened *on* that date — only the day before. For a vault
  whose first-ever snapshot lands after 00:00 UTC on its init day (the common case), `--at
  <init-day>` fails outright with NoSnapshotAt, and for any later day, the query silently returns
  the previous day's state instead of an error, which is worse. Fix: bare dates resolve to the end
  of that calendar day in the host's local timezone (23:59:59.999999999 local, converted to UTC),
  matching the timezone the existing `HH:MM` format already uses and matching what a non-git user
  actually means by \"show me the file as of this date.\""
todos:
  - id: branch
    content: Sync main, create feat/bare-date-end-of-day branch
    status: pending
  - id: tdd-end-of-day
    content: "TDD: rewrite AtDate::from_calendar_date (src/at_date.rs:52-59) to resolve YYYY-MM-DD
      to 23:59:59.999999999 in the host's local timezone, converted to UTC, instead of UTC
      midnight. Reuse the same Local::from_local_datetime(...).single() pattern
      from_local_date_time already uses; on an ambiguous/nonexistent local instant (DST edge,
      vanishingly unlikely at 23:59:59 but not impossible in every timezone), fall back to
      .latest() rather than erroring, since day-granularity queries don't need exact-instant DST
      correctness the way an explicit HH:MM does"
    status: pending
  - id: tdd-unit-tests
    content: "Update src/at_date.rs's own tests: from_calendar_date_is_utc_midnight ->
      from_calendar_date_is_end_of_local_day, asserting against a value computed via
      chrono::Local itself (self-consistent across the test runner's TZ, same pattern
      from_local_date_time_converts_host_timezone_to_utc already uses) rather than a hardcoded
      UTC literal. parse_accepts_all_three_formats and from_str_delegates_to_parse need no
      behavior change, just re-verification"
    status: pending
  - id: tdd-integration-tests
    content: "tests/show.rs:10-33 (show_returns_content_at_or_before_date) is the one integration
      test that actually exercises the day-boundary: --at 2026-06-02 currently asserts it resolves
      to the 06-01 09:00 commit (v1); under end-of-day semantics it must now resolve to the 06-02
      09:00 commit itself (v2), since 2026-06-02T09:00:00Z falls before end-of-day 06-02. Pin the
      subprocess's timezone via .env(\"TZ\", \"UTC\") on the assert_cmd Command builder (scoped to
      that one subprocess, not the test harness process) so the UTC-authored fixture timestamps
      and the local-day boundary agree regardless of the host running the test suite. Update the
      inline comment accordingly. Sanity-check the other four tests in the file (untracked path,
      whole-vault report, directory scope, single-file dump) still pass unmodified — each only has
      a single snapshot before the queried date, so the boundary shift doesn't change their
      resolved commit"
    status: pending
  - id: tdd-restore-diff-regression
    content: "tests/restore.rs:132 and tests/diff.rs use bare dates only for the
      before-any-snapshot-exists case (2020-01-01, 2026-06-02 as the 'no --at' usage-error
      sentinel) — confirm both still pass unchanged (end-of-day 2020-01-01 is still long before
      any 2026 snapshot) and add one new restore.rs case mirroring show.rs's same-day-resolves
      behavior if restore doesn't already exercise it elsewhere"
    status: pending
  - id: docs
    content: "Update docs/src/cli.md's Date formats table (~line 178-182): 'YYYY-MM-DD | Date;
      start of day UTC' -> 'YYYY-MM-DD | Date; end of day, local timezone'. Add a short line
      clarifying that a bare date shows the latest state as of that day (inclusive), which is why
      it now differs from the exact-instant RFC3339 form vault log prints (that one is still exact,
      unaffected)"
    status: pending
  - id: changelog
    content: "CHANGELOG.md Unreleased/Changed entry describing the semantics fix, calling out that
      it's a behavior change for anyone already relying on bare-date --at (unlikely given the old
      behavior was effectively unusable for 'today', but worth flagging since it is observable)"
    status: pending
  - id: ci
    content: "./scripts/ci.sh lint green"
    status: pending
isProject: false
---

# Fix bare-date `--at` semantics

## Problem

Reported directly by a user running the tool day-to-day (`.plans` self-hosted vault): the vault
was initialized 2026-08-05, edited again the same day, but `vault show README.md --at 2026-08-05`
failed with `Error: no snapshot at or before 2026-08-05T00:00:00+00:00`.

Root cause, traced end to end:

- `AtDate::from_calendar_date` (`src/at_date.rs:52-59`) parses a bare `YYYY-MM-DD` as **UTC
  midnight, start of day**. This is documented (`docs/src/cli.md`'s Date formats table) and
  intentional per the module doc (`src/at_date.rs:1-8`).
- `resolve_at` (`src/storage/sqlite/mod.rs:111-122`, backed by `SELECT_COMMIT_AT_OR_BEFORE`) finds
  the latest commit whose `created_at <= at`, via plain string comparison on UTC RFC3339 text.
- Combined effect, and explicitly locked in by `tests/show.rs:22-25`'s own comment: `--at
  2026-06-02` resolves to whatever was true at the **start** of June 2nd — i.e. effectively the
  **previous** day's last snapshot, never anything that happened on the 2nd itself.
- The vault's first-ever snapshot in the repro landed 2026-08-05T16:34:05 UTC — after that day's
  UTC midnight. So `--at 2026-08-05` resolves to an instant strictly before the vault's own
  existence, and correctly (per current semantics) finds nothing. There is no bare date the user
  could type that shows "state as of today" or even "state as of the day the vault started."

This isn't a data-loss or corruption bug — the underlying git/sqlite history is intact and `vault
log` shows every snapshot correctly. It's a semantics bug: the one CLI ergonomic this tool exists
to provide ("no git knowledge required," per the README) silently does the opposite of what a
bare-date query reads as meaning, and fails outright for the single most common query a new user
tries first — "show me what this looked like on the day I started."

## Fix

Bare `YYYY-MM-DD` now resolves to the **end of that calendar day, in the host's local timezone**
(`23:59:59.999999999` local, converted to UTC on output — same normalize-to-UTC-string storage
`AtDate` already uses internally, so `resolve_at`'s string comparison stays correct unchanged).

Two deliberate choices worth recording:

- **Local, not UTC.** The `HH:MM` form (`src/at_date.rs:67-81`) already interprets its input in
  the host's local timezone. Keeping bare dates on UTC while `HH:MM` is local was already an
  inconsistency; fixing bare dates while leaving them on a different timezone basis than `HH:MM`
  would be a second, smaller version of the same inconsistency. Local also fixes the reported case
  for *any* UTC offset — an end-of-UTC-day boundary would still undercount for users at negative
  offsets (e.g. UTC-8, where local "Aug 5" only *starts* at 08:00 UTC and doesn't finish until
  08:00 UTC on the 6th).
- **End of day, not start.** "Show me X as of this date" naturally reads as inclusive of that
  date's activity, matching how someone would describe a day's-end state without git in mind. The
  exact-instant RFC3339 form (what `vault log` prints, and what round-trips back into `--at`) is
  unaffected — power users needing precision already have it.

Full RFC3339 (`AtDate::from_rfc3339`) and the `HH:MM` local-time form are unaffected by this
change.

## Testing note: timezone-dependent integration tests

`tests/show.rs`'s `show_returns_content_at_or_before_date` is the only integration test that
actually exercises the day boundary (multiple snapshots straddling the queried date). Under the
new semantics its expected output flips (`--at 2026-06-02` now resolves to the *same-day* 06-02
09:00 commit, not the prior day's), and — because the boundary is now computed in the local
timezone of whatever machine runs the test — the fixture must pin the subprocess's timezone
explicitly (`.env("TZ", "UTC")` on the `assert_cmd` `Command`) rather than assume the CI/dev
machine happens to run UTC. Unit tests in `src/at_date.rs` avoid this problem already by asserting
against a value computed through `chrono::Local` at test time (see
`from_local_date_time_converts_host_timezone_to_utc`) instead of a hardcoded literal; the new
`from_calendar_date` test should follow the same pattern.

## Files touched

| Area | File | Change |
|------|------|--------|
| Core parsing | `src/at_date.rs` | `from_calendar_date`: UTC midnight -> local end-of-day |
| Unit tests | `src/at_date.rs` (`#[cfg(test)]`) | Rewrite the UTC-midnight assertion to a self-consistent local-end-of-day one |
| Integration tests | `tests/show.rs` | Fix the one boundary-dependent test's expectation + pin `TZ=UTC` on that subprocess |
| Integration tests | `tests/restore.rs`, `tests/diff.rs` | Confirm unaffected (bare dates there only exercise "before any snapshot exists" / usage-error paths); add one same-day-resolves case to `restore.rs` if not already covered |
| Docs | `docs/src/cli.md` | Date formats table + a line on inclusive same-day semantics |
| Changelog | `CHANGELOG.md` | Unreleased/Changed entry |

`show`, `restore`, and `diff` all route through the same `AtDate` type (`src/cli/commands/{show,
restore, diff}.rs`), so this is a single-point fix — no per-command changes needed.

## Verification

- Unit: `from_calendar_date` resolves to local end-of-day, self-consistently checked against
  `chrono::Local` rather than a hardcoded offset.
- Integration: `tests/show.rs`'s boundary test asserts same-day resolution now succeeds and
  resolves to the same-day commit; `TZ=UTC` pinned on that subprocess.
- Manual repro check: in a fresh vault initialized and edited entirely on one calendar day,
  `vault show <file> --at <that day>` returns the latest same-day content instead of
  `NoSnapshotAt`.
- `./scripts/ci.sh lint` green.

## Exit criteria

- [ ] `--at <today>` succeeds and reflects same-day edits for a vault initialized and edited today
- [ ] `docs/src/cli.md`'s Date formats table matches the new behavior
- [ ] `CHANGELOG.md` records the semantics change
- [ ] `./scripts/ci.sh lint` green
