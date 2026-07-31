# Changelog

## Unreleased

### Added

* Singleton background watcher — one daemon per user watches all registered vaults via `registry.toml` hot reload (`notify`).
* `vault status` — daemon heartbeat, vault count, and last snapshot per registered vault.
* `vault ignore PATTERN` — append ignore globs to `.vault/config.toml`.
* Hidden `vault daemon [--foreground]` — singleton lock, heartbeat writer, and multi-vault watcher loop.
* Global registry (`registry.toml`) with atomic writes under `registry.lock`.
* Snapshot pipeline via gix tree editor + `commit_as` + sqlite transaction; baseline snapshot at `vault init`.
* Systemd user unit adapter (`vault-watcher.service`) with detached-spawn fallback when systemd is unavailable.
* `vault init --no-service` and `VAULT_NO_SERVICE` to skip daemon startup (tests/CI).
* Default ignore pattern `.git/**` for coexistence with foreign git directories at the project root.
* `vault init` — creates `.vault/` with `config.toml`, recovery `README`, gix bare git-dir, and SQLite schema.
* Storage modules: `gix` object store (`src/storage/git.rs`), `rusqlite` metadata index (`src/storage/sqlite.rs`).
* Integration tests for init layout, re-init guard, config defaults, and schema (`tests/init.rs`).
* Cargo library + binary scaffold with async CLI (`clap`, `tokio`).
* Stub subcommands: `init`, `show`, `restore`, `log`, `diff`, `status`, `list`, `ignore`.
* GitHub Actions CI (`lint-test`, `build-test`, `docs`) and local `scripts/ci.sh` mirror.
* Full mdBook site (getting started, architecture, CLI reference, releasing).
* GitHub Pages deploy workflow (`deploy-docs.yml`).
