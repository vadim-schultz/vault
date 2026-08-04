# Vault project rules

## Starting a new chapter

Before beginning implementation of any plan under `.plans/` (e.g. `.plans/benches/optimize.plan.md`
or a chapter plan in `.plans/mvp/chapters/`), always sync with `main` and branch from a clean,
up-to-date tip:

```bash
git checkout main && git pull
git checkout -b feat/<short-chapter-name>
```

Never start implementing a new chapter directly on `main`, and never build on top of a stale or
forgotten branch left over from earlier work. Verify `git status` is clean before switching.
