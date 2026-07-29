---
name: Chapter 1 — Empty Repo, Green CI
overview: Scaffold the Rust library+binary crate, minimal async CLI (`--version` + stub subcommands), green CI (GitHub Actions + scripts/ci.sh), and a one-page mdBook stub so the docs job passes.
todos:
  - id: ch1-cargo-scaffold
    content: "cargo new --lib; Cargo.toml with clap/tokio/anyhow; rustfmt.toml; src/bin/vault.rs"
    status: pending
  - id: ch1-minimal-cli
    content: "src/lib.rs, src/cli.rs with stub subcommands; #[tokio::main] entry"
    status: pending
  - id: ch1-root-files
    content: "README.md, CHANGELOG.md, CONTRIBUTING.md; extend .gitignore"
    status: pending
  - id: ch1-mdbook-stub
    content: "docs/book.toml + single-page stub for CI docs job"
    status: pending
  - id: ch1-ci-pipeline
    content: ".github/workflows/ci.yml, dependabot.yml, scripts/ci.sh"
    status: pending
  - id: ch1-tests-verify
    content: "cli version test; ./scripts/ci.sh all green"
    status: pending
---

# Chapter 1 — Empty Repo, Green CI

## Context

**Prerequisite:** [chapter_0.plan.md](chapter_0.plan.md) Chapter 0 complete — `cargo --version`, `rustc --version`, `rustfmt` and `clippy` components installed.

**Parent plan:** [chapter_0.plan.md](chapter_0.plan.md) (master bootstrap overview).

**Current repo state:** initial commit with `LICENSE`, Rust-default `.gitignore`, and `.plans/`. No `Cargo.toml`, no `src/`, no CI.

---

## Goal

Smallest shippable milestone: a library-first Rust crate with a `vault` binary that prints `--version`, stub subcommands visible in `--help`, and CI that passes on every push/PR to `main`.

**Not in scope for Chapter 1:** real `init`/storage logic (Ch 3), full mdBook site (Ch 2), release workflows (Ch 6), `smoke_test.sh` (Ch 5).

---

## Exit criteria

| Check | Command |
|-------|---------|
| Format | `cargo fmt --all -- --check` |
| Lint | `cargo clippy -- -D warnings` |
| Tests | `cargo test` |
| Release build | `cargo build --release && ./target/release/vault --version` |
| Docs job | `mdbook build docs/` |
| Local CI mirror | `./scripts/ci.sh all` |
| Remote CI | GitHub Actions green on PR merge to `main` |

Expected `--version` output: `vault 0.1.0` (version from `CARGO_PKG_VERSION`).

---

## Git workflow

```bash
cd /home/schult_v/projects/vault
git checkout main && git pull
git checkout -b feat/ch1-empty-ci
# ... implement ...
./scripts/ci.sh all
git push -u origin feat/ch1-empty-ci
# Open PR → merge when CI green. No tag.
```

Commit message (conventional): `feat(cli): scaffold cargo project with minimal CLI and CI`

---

## Implementation steps

### Step 1 — Cargo scaffold

From repo root:

```bash
cargo new --lib .
```

- Remove any `src/main.rs` if generated (library-first layout uses `src/bin/vault.rs` only).
- Add `src/bin/vault.rs` as the sole binary entry point.
- Commit `Cargo.lock` (application crate convention).

#### `Cargo.toml` (key fields)

```toml
[package]
name = "vault"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
description = "Automatic document versioning — init once, retrieve any version later"
license = "MIT"
repository = "https://github.com/vadim-schultz/vault"
readme = "README.md"

[dependencies]
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = "warn"
pedantic = "warn"
```

**Chapter 1 deps only:** `clap`, `tokio`, `anyhow`. Defer `thiserror`, `gix`, `rusqlite`, `notify`, `chrono`, `toml`, `serde` to Chapters 3–5.

#### `rustfmt.toml`

```toml
edition = "2021"
max_width = 100
```

---

### Step 2 — Source layout (minimal modules)

```
src/
├── lib.rs          # pub mod cli; pub use cli::run
├── cli.rs          # clap Parser + async dispatch
└── bin/
    └── vault.rs    # #[tokio::main] → vault::run().await
```

#### `src/lib.rs`

- Module docstring: one-line crate summary.
- `pub mod cli;`
- `pub use cli::run;` — async entry for the binary and future integration tests.

#### `src/cli.rs`

Use `clap` derive with:

**Global flags:**
- `--version` / `-V` via `#[command(version)]` (reads `CARGO_PKG_VERSION`)
- `-v` / `--verbose` (stored, unused until later chapters)
- `--vault-path PATH` (optional, unused until Ch 3)

**Stub subcommands** (visible in `--help`, return `bail!("not implemented yet")`):

| Subcommand | Notes |
|------------|-------|
| `init` | Placeholder for Ch 3 |
| `show PATH --at DATE` | Placeholder for Ch 5 |
| `restore PATH --at DATE` | `--dry-run` flag stub |
| `log [PATH]` | |
| `diff PATH` | `--at`, `--to` flags stub |
| `status` | |
| `list` | |
| `ignore PATTERN` | |

`run()` signature: `pub async fn run() -> anyhow::Result<()>` — async from day one; stub handlers are trivial `async` fns so Ch 3+ can add real I/O without signature changes.

#### `src/bin/vault.rs`

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vault::run().await
}
```

---

### Step 3 — Tests

**`tests/cli_version.rs`** — integration test via `assert_cmd`:

```rust
Command::cargo_bin("vault")?.arg("--version").assert().success()
    .stdout(predicate::str::contains("vault 0.1.0"));
```

---

### Step 4 — Root files

| File | Content |
|------|---------|
| `README.md` | Tagline: "document version history, not secrets management"; vision; build instructions; CI badge |
| `CHANGELOG.md` | `## Unreleased` → scaffold CLI, CI |
| `CONTRIBUTING.md` | Rust setup; `./scripts/ci.sh`; individual cargo commands |
| `.gitignore` | Add `.vault/`, `docs/book/` |

---

### Step 5 — mdBook stub (CI docs job only)

```
docs/
├── book.toml
└── src/
    ├── SUMMARY.md
    └── index.md
```

Add `docs/book/` to `.gitignore`. Chapter 2 expands content and enables GitHub Pages.

---

### Step 6 — GitHub Actions CI

Three parallel jobs: `lint-test`, `build-test`, `docs`. See `.github/workflows/ci.yml`.

Weekly `dependabot.yml` for `cargo` + `github-actions`.

---

### Step 7 — `scripts/ci.sh`

```
Usage: scripts/ci.sh [lint|build|docs|all]
```

| Command | Steps |
|---------|-------|
| `lint` | `cargo fmt --check`, `cargo clippy`, `cargo test` |
| `build` | `cargo build --release`, `./target/release/vault --version` |
| `docs` | `mdbook build docs/` |
| `all` | lint → build → docs (default) |

---

## Verification checklist

```bash
cargo fmt --all
cargo clippy -- -D warnings
cargo test
cargo build --release
./target/release/vault --version
./target/release/vault --help
./target/release/vault init              # exits non-zero, "not implemented"
mdbook build docs/
./scripts/ci.sh all
```

---

## Deferred to later chapters

| Item | Chapter |
|------|---------|
| Full mdBook pages + GitHub Pages | 2 |
| `error.rs`, `config.rs`, `storage/` modules | 3 |
| `gix`, `rusqlite`, `thiserror` deps | 3 |
| `scripts/smoke_test.sh` in CI | 5 |
| `release.yml`, `publish.yml` | 6 |
| Hidden `internal-watch` subcommand | 4 |
