---
name: Chapter 2 — Documentation skeleton
overview: Expand the Chapter 1 mdBook stub into a full documentation site (getting started, architecture, CLI reference, releasing), deploy to GitHub Pages via Actions, and add README docs badge.
todos:
  - id: ch2-mdbook-pages
    content: "SUMMARY + getting_started, architecture, cli, releasing; refresh index.md"
    status: pending
  - id: ch2-book-toml
    content: "site-url /vault/, edit-url-template in docs/book.toml"
    status: pending
  - id: ch2-deploy-workflow
    content: ".github/workflows/deploy-docs.yml (upload-pages-artifact + deploy-pages)"
    status: pending
  - id: ch2-readme-badges
    content: "docs badge + Documentation section; update CONTRIBUTING.md"
    status: pending
  - id: ch2-changelog-plans
    content: "Update CHANGELOG.md and .plans/README.md"
    status: pending
  - id: ch2-verify
    content: "./scripts/ci.sh all green"
    status: pending
---

# Chapter 2 — Documentation skeleton

## Context

**Prerequisite:** [chapter_1.plan.md](chapter_1.plan.md) merged to `main` — Cargo scaffold, async CLI stubs, three-job CI, one-page mdBook stub.

**Parent plan:** [chapter_0.plan.md](chapter_0.plan.md) § Chapter 2.

---

## Goal

Published user docs at `https://vadim-schultz.github.io/vault/` with architecture and CLI reference stubs. CI verifies `mdbook build`; merge to `main` triggers deploy.

---

## Exit criteria

| Check | Command / URL |
|-------|----------------|
| mdBook builds locally | `mdbook build docs/` |
| Full local CI | `./scripts/ci.sh all` |
| PR CI green | All three jobs pass |
| Docs site live | `https://vadim-schultz.github.io/vault/` returns 200 after merge |
| README badges | CI, docs (link to Pages), license |
| Architecture page | `.vault/` layout + inspect-without-vault guide |
| CLI page | All stub subcommands documented |

---

## Git workflow

```bash
cd /home/schult_v/projects/vault
git checkout main && git pull
git checkout -b feat/ch2-docs-skeleton
# ... implement ...
./scripts/ci.sh all
git push -u origin feat/ch2-docs-skeleton
# Open PR → merge when CI green. No tag.
```

Commit message: `docs: expand mdbook site and deploy to GitHub Pages`

---

## Implementation summary

1. Expand `docs/src/` — `SUMMARY.md`, `getting_started.md`, `architecture.md`, `cli.md`, `releasing.md`, refresh `index.md`.
2. `architecture.md` — `.vault/` layout, store table, inspect-without-vault guide, mermaid diagrams.
3. `docs/book.toml` — `site-url = "/vault/"`, `edit-url-template`.
4. `.github/workflows/deploy-docs.yml` — deploy on `main` push.
5. README docs badge; CONTRIBUTING Pages setup note.
6. CHANGELOG + `.plans/README.md` updates.

---

## Deferred to later chapters

| Item | Chapter |
|------|---------|
| `vault init`, runtime `.vault/README` | 3 |
| Watcher / systemd / `status` behavior docs | 4 |
| `show`, `log`, `diff`, `restore`, `list` behavior | 5 |
| `release.yml`, `publish.yml`, crates.io badge | 6 |
