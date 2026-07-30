# Vault

**Automatic version history.**

Run `vault init` once in a docs directory, then forget about it. Weeks or months later,
retrieve any earlier version:

```bash
vault show README.md --at 2026-06-01
vault restore design.md --at "2026-06-01 23:58"
```

No git knowledge required. Vault watches files in the background and records every change.

## Where to go next

- [Getting started](getting_started.md) — install Rust, build the CLI, run local checks.
- [Architecture](architecture.md) — how `.vault/` is laid out and how snapshots work.
- [CLI reference](cli.md) — commands and flags (stubs until later chapters land).
