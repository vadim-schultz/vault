# CLI UX stream

Making the read commands (`show`, `log`, `diff`, ...) read like a tool for people who don't know
git, not a thin wrapper that leaks commit SHAs and raw byte dumps.

| Document | Role |
|----------|------|
| [humanize_show_log.plan.md](humanize_show_log.plan.md) | `vault show` scope levels (file/directory/whole-vault) + humanized `vault log` output (landed) |
| [bare_date_end_of_day.plan.md](bare_date_end_of_day.plan.md) | Fix `--at YYYY-MM-DD` resolving to UTC start-of-day, which made same-day queries fail/silently return the wrong day (landed) |
| [vault_prune.plan.md](vault_prune.plan.md) | `vault prune` — manual escape hatch for `[missing]` registry entries the daemon's reactive prune doesn't reach |
| [vault_init_idempotent.plan.md](vault_init_idempotent.plan.md) | `vault init` on an existing vault — restart a stopped daemon and repair safely-regenerable markers instead of hard-erroring (draft) |
