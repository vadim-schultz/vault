# Changelog

## Unreleased

### Added

* `vault init` — creates `.vault/` with `config.toml`, recovery `README`, gix bare git-dir, and SQLite schema.
* Storage modules: `gix` object store (`src/storage/git.rs`), `rusqlite` metadata index (`src/storage/sqlite.rs`).
* Integration tests for init layout, re-init guard, config defaults, and schema (`tests/init.rs`).
* Cargo library + binary scaffold with async CLI (`clap`, `tokio`).
* Stub subcommands: `init`, `show`, `restore`, `log`, `diff`, `status`, `list`, `ignore`.
* GitHub Actions CI (`lint-test`, `build-test`, `docs`) and local `scripts/ci.sh` mirror.
* Full mdBook site (getting started, architecture, CLI reference, releasing).
* GitHub Pages deploy workflow (`deploy-docs.yml`).
