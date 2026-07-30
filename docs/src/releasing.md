# Releasing

Version is defined in `Cargo.toml` (`[package].version`, currently `0.1.0`).

## Changelog

Release notes live in [CHANGELOG.md](https://github.com/vadim-schultz/vault/blob/main/CHANGELOG.md)
at the repository root.

## Current status

Automated release workflows are planned for **Chapter 6** (v0.1.0):

- GitHub Release with Linux binary (`vault-x86_64-unknown-linux-gnu`)
- `cargo publish` to crates.io
- `release.yml` and `publish.yml` workflows

## Maintainer process (stub)

Full detail will be added in Chapter 6:

1. Bump `version` in `Cargo.toml`.
2. Update `CHANGELOG.md` (move `Unreleased` entries under the new version heading).
3. Merge to `main`.
4. Tag `v<version>` and let CI create the GitHub Release (Chapter 6).

## Install (future)

After v0.1.0:

```bash
cargo install vault
```

Until then, build from source — see [Getting started](getting_started.md).
